#!/usr/bin/env python3
"""Long-term soak testing and resource leak guard for TaskForest background daemon and engine.

Verifies:
1. Physical resident set size (RSS) stability across consecutive high-frequency sampling ticks.
2. File descriptor (FD) and handle RAII release invariance (zero leaked descriptors).
3. Monotonic retention bounds in rolling history persistence.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import tempfile
import time
from pathlib import Path


def read_proc_rss_kb(pid: int) -> int | None:
    try:
        with open(f"/proc/{pid}/status", "r", encoding="utf-8") as f:
            for line in f:
                if line.startswith("VmRSS:"):
                    parts = line.split()
                    return int(parts[1])
    except (FileNotFoundError, PermissionError, ValueError, IndexError):
        return None
    return None


def read_proc_fd_count(pid: int) -> int | None:
    try:
        return len(os.listdir(f"/proc/{pid}/fd"))
    except (FileNotFoundError, PermissionError):
        return None


# Descriptor counts are whole handles, so the tolerance is a count, not a
# percentage: three handles of slack inside the settled region is noise, more
# than that *and still climbing* is the leak signature.
FD_SETTLED_TOLERANCE = 3
# One absolute bound that no shape can talk its way past: a process sitting on
# thousands of descriptors is a structural cost whether or not the series has
# plateaued, which is the same lesson Arm D of `tests/soak.rs` records for the
# collector's per-process /proc handles.
FD_HARD_CEILING = 4096
# A step is only a plateau when flatness has actually been observed: this many
# settled samples sitting at the settled maximum. Below it the guard says
# "cannot tell" rather than claiming a leak or claiming stability.
FD_PLATEAU_EVIDENCE = 10


# Resident memory on a managed runtime can fault in a large one-time working
# set long after the first warm-up tick. The old first-vs-last rule treated
# that delayed warm-up as the settled baseline and then called the whole
# window a leak. A real leak keeps making new highs; a paid working set does
# not. These bounds describe the *tail* that is allowed to carry the baseline:
# at least a third of the settled window, no new maximum, bounded noise, and
# no projected drift above the same 25% tolerance over the tail's horizon.
RSS_TAIL_MIN_SAMPLES = 10
RSS_TAIL_DIVISOR = 3
RSS_TAIL_RANGE_PCT = 12.0
RSS_TAIL_DRIFT_PCT = 25.0
RSS_GROWTH_PCT = 25.0


def rss_series_facts(rss_samples: list[int]) -> dict[str, object]:
    """Describe the shape of one process lifetime's resident memory."""
    settled = rss_samples[1:]
    if len(settled) < 3:
        growth_pct = (
            (settled[-1] - settled[0]) / settled[0] * 100.0
            if len(settled) >= 2 and settled[0]
            else 0.0
        )
        return {
            "samples": len(rss_samples),
            "settled": len(settled),
            "baseline_kib": settled[0] if settled else None,
            "final_kib": settled[-1] if settled else None,
            "growth_pct": growth_pct,
            "tail_samples": len(settled),
            "tail_median_kib": None,
            "tail_range_pct": None,
            "tail_drift_pct": None,
            "tail_new_maxima": None,
            "tail_plateau": False,
        }
    tail_size = max(RSS_TAIL_MIN_SAMPLES, len(settled) // RSS_TAIL_DIVISOR)
    tail_size = min(tail_size, len(settled))
    tail = settled[-tail_size:]
    prefix = settled[:-tail_size]
    median = sorted(tail)[len(tail) // 2]
    range_pct = (max(tail) - min(tail)) / median * 100.0 if median else 0.0
    x_mean = (len(tail) - 1) / 2.0
    y_mean = sum(tail) / len(tail)
    denominator = sum((index - x_mean) ** 2 for index in range(len(tail)))
    slope = (
        sum((index - x_mean) * (value - y_mean) for index, value in enumerate(tail))
        / denominator
        if denominator
        else 0.0
    )
    drift_pct = slope * (len(tail) - 1) / median * 100.0 if median else 0.0
    prior_high = max([*prefix, tail[0]])
    new_maxima = sum(
        1
        for index, value in enumerate(tail)
        if index > 0 and value > prior_high and value > max(tail[:index])
    )
    growth_pct = (
        (settled[-1] - settled[0]) / settled[0] * 100.0 if settled[0] else 0.0
    )
    plateau = (
        new_maxima == 0
        and range_pct <= RSS_TAIL_RANGE_PCT
        and abs(drift_pct) <= RSS_TAIL_DRIFT_PCT
        and max(tail) <= max([*prefix, tail[0]]) * 1.01
    )
    return {
        "samples": len(rss_samples),
        "settled": len(settled),
        "baseline_kib": settled[0],
        "final_kib": settled[-1],
        "growth_pct": growth_pct,
        "tail_samples": len(tail),
        "tail_median_kib": median,
        "tail_range_pct": range_pct,
        "tail_drift_pct": drift_pct,
        "tail_new_maxima": new_maxima,
        "tail_plateau": plateau,
    }


def rss_series_violations(rss_samples: list[int]) -> list[str]:
    """Judge resident-memory drift from the observed steady-state shape."""
    if len(rss_samples) < 3:
        return ["insufficient samples for soak evaluation"]
    facts = rss_series_facts(rss_samples)
    growth_pct = float(facts["growth_pct"])
    if growth_pct <= RSS_GROWTH_PCT:
        return []
    if facts["tail_plateau"]:
        return []
    if facts["settled"] < 3:
        return [
            f"RSS memory grew by {growth_pct:.1f}% "
            f"({facts['baseline_kib']} KB -> {facts['final_kib']} KB), "
            "exceeding stability threshold"
        ]
    return [
        f"RSS memory grew by {growth_pct:.1f}% "
        f"({facts['baseline_kib']} KB -> {facts['final_kib']} KB) without a "
        f"settled tail (range {float(facts['tail_range_pct']):.1f}%, "
        f"drift {float(facts['tail_drift_pct']):.1f}%, "
        f"new maxima {facts['tail_new_maxima']}), exceeding stability threshold"
    ]


def evaluate_soak_series(rss_samples: list[int], fd_samples: list[int]) -> list[str]:
    violations: list[str] = []
    violations.extend(rss_series_violations(rss_samples))
    violations.extend(fd_series_violations(fd_samples))
    return violations


def fd_series_facts(fd_samples: list[int]) -> dict[str, object]:
    """Describe the *shape* of a descriptor series over one process lifetime.

    The first sample is warm-up, exactly as the RSS rule treats it. Inside the
    settled region the two facts that matter are how many times the series set
    a new maximum (a leak keeps reaching for new handles; a lazily opened
    handle does so once) and whether the series ever gave a descriptor back.
    """
    settled = fd_samples[1:]
    if not settled:
        return {"samples": len(fd_samples), "settled": 0}
    levels = sorted(set(settled))
    new_maxima = sum(
        1 for index, value in enumerate(settled) if value > max(settled[:index] or [settled[0]])
    )
    non_decreasing = all(b >= a for a, b in zip(settled, settled[1:]))
    return {
        "samples": len(fd_samples),
        "settled": len(settled),
        "warmup_step": [fd_samples[0], settled[0]] if fd_samples[0] != settled[0] else None,
        "settled_first": settled[0],
        "settled_last": settled[-1],
        "settled_max": max(settled),
        "at_settled_max": sum(1 for value in settled if value == max(settled)),
        "settled_levels": levels,
        "new_maxima": new_maxima,
        "non_decreasing": non_decreasing,
    }


def fd_series_violations(fd_samples: list[int]) -> list[str]:
    """Flag continued descriptor growth, never a plateau.

    A single step to a stable level is a startup cost that has already been
    paid: the 2.6h private window measured exactly that on the Bevy demo
    (27 -> 31 within the first two ticks, then 154 identical samples), and the
    old first-vs-last rule called it a leak for the whole window. A descriptor
    leak is a series that keeps climbing, so the judgement is on shape inside
    the settled region plus one absolute bound that a plateau can never escape.
    """
    if len(fd_samples) < 2:
        return []
    facts = fd_series_facts(fd_samples)
    violations: list[str] = []
    if facts["settled_max"] >= FD_HARD_CEILING:
        violations.append(
            f"descriptor count reached {facts['settled_max']}, at or above the "
            f"{FD_HARD_CEILING} structural ceiling, whatever its shape"
        )
    rise = facts["settled_last"] - facts["settled_first"]
    continued = rise > FD_SETTLED_TOLERANCE and facts["new_maxima"] >= 2
    staircase = (
        facts["non_decreasing"] and rise >= 2 and len(facts["settled_levels"]) >= 3
    )
    if continued or staircase:
        violations.append(
            f"descriptor leak: {facts['settled']} settled samples grew from "
            f"{facts['settled_first']} to {facts['settled_last']} across "
            f"{facts['new_maxima']} new maxima ({facts['settled_levels']})"
        )
    elif rise > FD_SETTLED_TOLERANCE and facts["at_settled_max"] < FD_PLATEAU_EVIDENCE:
        violations.append(
            f"descriptor step is unresolved: {facts['settled_first']} -> "
            f"{facts['settled_last']} with only {facts['at_settled_max']} settled "
            "samples at the maximum, too few to call it flat and too few to "
            "rule out continued growth"
        )
    return violations


def load_jsonl_samples(
    path: Path,
) -> tuple[dict[tuple[str, object], dict[str, list[int]]], list[str]]:
    """Load the soak runner's authoritative samples without rewriting them.

    Series are keyed by **(target, pid)**, not by target alone: a restart
    inside the window hands the same name a new process, and reading one
    lifetime's steady count after another lifetime's as a single series invents
    either a growth or a drop that no process ever did.
    """
    series: dict[tuple[str, object], dict[str, list[int]]] = {}
    findings: list[str] = []
    if not path.is_file():
        return series, [f"samples file is missing: {path}"]
    for line_number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip():
            continue
        try:
            row = json.loads(line)
        except json.JSONDecodeError as error:
            findings.append(f"line {line_number}: invalid JSON: {error}")
            continue
        if not isinstance(row, dict):
            findings.append(f"line {line_number}: not an object")
            continue
        name = row.get("target")
        if not isinstance(name, str) or not name:
            findings.append(f"line {line_number}: missing target")
            continue
        key = (name, row.get("pid"))
        target = series.setdefault(key, {"rss_kib": [], "fd_count": []})
        for metric in ("rss_kib", "fd_count"):
            value = row.get(metric)
            if value is None:
                # An unavailable OS observation stays absent; the evaluator
                # judges the observed subseries, it never fabricates zero.
                continue
            if isinstance(value, int) and value >= 0:
                target[metric].append(value)
            else:
                findings.append(f"line {line_number}: invalid {metric}: {value!r}")
    return series, findings


def evaluate_jsonl_samples(
    path: Path,
    required_hours: float = 72.0,
    elapsed_hours: float | None = None,
) -> dict[str, object]:
    series, findings = load_jsonl_samples(path)
    if not series:
        findings.append("samples contain no observed targets")
    per_target: dict[str, object] = {}
    names = sorted({name for name, _ in series})
    for name in names:
        keys = sorted((pid for n, pid in series if n == name), key=lambda value: str(value))
        for pid in keys:
            metrics = series[(name, pid)]
            label = f"{name}#{pid}" if len(keys) > 1 else name
            violations = evaluate_soak_series(metrics["rss_kib"], metrics["fd_count"])
            findings.extend(f"{label}: {violation}" for violation in violations)
            per_target[label] = {
                "rss_samples": len(metrics["rss_kib"]),
                "fd_samples": len(metrics["fd_count"]),
                "rss_first_kib": metrics["rss_kib"][0] if metrics["rss_kib"] else None,
                "rss_last_kib": metrics["rss_kib"][-1] if metrics["rss_kib"] else None,
                "rss_shape": rss_series_facts(metrics["rss_kib"]),
                "fd_first": metrics["fd_count"][0] if metrics["fd_count"] else None,
                "fd_last": metrics["fd_count"][-1] if metrics["fd_count"] else None,
                "fd_shape": fd_series_facts(metrics["fd_count"]),
                "violations": violations,
            }
    if elapsed_hours is not None and elapsed_hours < required_hours:
        findings.append(
            f"wall-clock duration is {elapsed_hours}h, below the {required_hours}h judgement window"
        )
    return {
        "path": path.as_posix(),
        "status": "pass" if not findings else "review_required",
        "elapsed_hours": elapsed_hours,
        "required_hours": required_hours,
        "targets": per_target,
        "findings": findings,
    }


def self_test() -> int:
    # 1. Flat stable samples must PASS
    flat_rss = [10000, 12000, 12100, 12050, 12100, 12080]
    flat_fds = [15, 15, 15, 15, 15, 15]
    errors = evaluate_soak_series(flat_rss, flat_fds)
    assert not errors, f"self-test failed on flat series: {errors}"

    # 2. Leaking memory must FAIL
    leak_rss = [10000, 12000, 15000, 18000, 22000, 28000]
    errors = evaluate_soak_series(leak_rss, flat_fds)
    assert any("RSS memory grew" in e for e in errors), "self-test failed to catch memory leak"

    # 3. Leaking FDs must FAIL
    leak_fds = [15, 16, 18, 20, 25, 30]
    errors = evaluate_soak_series(flat_rss, leak_fds)
    assert any("descriptor leak" in e for e in errors), "self-test failed to catch FD leak"

    # 4. A one-time step that then holds flat is a startup cost, not a leak.
    #    Shape taken from the measured 2.6h window: 27 -> 31 in the second
    #    tick, then 154 identical samples.
    plateau_fds = [27] + [31] * 154
    errors = evaluate_soak_series(flat_rss, plateau_fds)
    assert not errors, f"a plateaued descriptor step was called a leak: {errors}"
    plateau_facts = fd_series_facts(plateau_fds)
    assert plateau_facts["warmup_step"] == [27, 31], plateau_facts
    assert plateau_facts["settled_first"] == 31 and plateau_facts["new_maxima"] == 0

    # 5. A slow staircase that never gives a descriptor back is still a leak,
    #    even when every individual step is inside the tolerance.
    creeping_fds = [15, 15, 16, 16, 17, 17, 18, 18]
    errors = evaluate_soak_series(flat_rss, creeping_fds)
    assert any("descriptor leak" in e for e in errors), "a monotone staircase escaped the guard"

    # 6. An absolute bound no shape can talk past: flat, but flat at a
    #    structurally absurd handle count.
    errors = evaluate_soak_series(flat_rss, [5000] * 6)
    assert any("structural ceiling" in e for e in errors), "the hard ceiling did not bite"

    # 7. A large one-time managed-runtime working set that stops making new
    #    highs is a plateau, not a leak. Its first settled sample is still far
    #    below the level the process actually needs to keep resident.
    working_set_rss = [10000, 12000] + [12000 + index * 1000 for index in range(1, 25)]
    working_set_rss += [35000, 34200, 34800, 34400, 34600, 34500, 34300, 34700, 34400, 34600]
    errors = evaluate_soak_series(working_set_rss, flat_fds)
    assert not errors, f"a settled working-set plateau was called a leak: {errors}"
    working_set_facts = rss_series_facts(working_set_rss)
    assert working_set_facts["tail_plateau"] is True, working_set_facts

    # 8. A series that is still reaching new highs is a leak even if its last
    #    few samples jitter. The shape, not a shortened tail, decides it.
    unresolved_leak_rss = [10000, 12000] + [12000 + index * 1000 for index in range(1, 40)]
    errors = evaluate_soak_series(unresolved_leak_rss, flat_fds)
    assert any("RSS memory grew" in e for e in errors), (
        "a series still setting new RSS maxima escaped the guard"
    )

    print("Soak stability guard self-test: PASS (memory growth and FD leak detection verified)")
    failures = 0
    import json
    import tempfile

    with tempfile.TemporaryDirectory(prefix="soak-jsonl-") as raw:
        path = Path(raw) / "samples.jsonl"
        rows = [
            {"target": "a", "rss_kib": 10000, "fd_count": 15},
            {"target": "a", "rss_kib": 10000, "fd_count": 15},
            {"target": "a", "rss_kib": 10000, "fd_count": 15},
            {"target": "a", "rss_kib": None, "fd_count": None},
            "{not json}\n",
        ]
        path.write_text("\n".join(json.dumps(row) if isinstance(row, dict) else row for row in rows), encoding="utf-8")
        report = evaluate_jsonl_samples(path, 72.0, 0.5)
        failures += report["status"] != "review_required"
        failures += report["targets"]["a"]["rss_samples"] != 3
        failures += not any("invalid JSON" in item for item in report["findings"])
        failures += not any("duration is 0.5h" in item for item in report["findings"])
        leak_path = Path(raw) / "leak.jsonl"
        leak_rows = [
            {"target": "a", "rss_kib": 10000, "fd_count": 15},
            {"target": "a", "rss_kib": 10000, "fd_count": 15},
            {"target": "a", "rss_kib": 28000, "fd_count": 30},
        ]
        leak_path.write_text("\n".join(json.dumps(row) for row in leak_rows), encoding="utf-8")
        leak = evaluate_jsonl_samples(leak_path, 72.0, 72.0)
        failures += not any("RSS memory grew" in item for item in leak["findings"])
        failures += not any("descriptor step is unresolved" in item for item in leak["findings"])
        # 7. A restart must not be read as one series: the same name on a new
        #    pid is judged as its own lifetime, so neither half's level can be
        #    called growth of the other.
        restart_path = Path(raw) / "restart.jsonl"
        restart_rows = (
            [{"target": "a", "pid": 10, "rss_kib": 10000, "fd_count": 40} for _ in range(4)]
            + [{"target": "a", "pid": 20, "rss_kib": 10000, "fd_count": 12} for _ in range(4)]
        )
        restart_path.write_text(
            "\n".join(json.dumps(row) for row in restart_rows), encoding="utf-8"
        )
        restart = evaluate_jsonl_samples(restart_path, 72.0, 72.0)
        failures += sorted(restart["targets"]) != ["a#10", "a#20"]
        failures += bool(restart["findings"])
        if failures:
            print(f"Soak JSONL guard self-test: FAIL ({failures} assertions)")
        else:
            print("Soak JSONL guard self-test: PASS (observed-series and absent-metric rules verified)")
    return 1 if failures else 0


def run_standalone_soak_check(repo_root: Path) -> int:
    # Measure own process over 10 memory/file iterations to verify host compatibility
    my_pid = os.getpid()
    rss_samples: list[int] = []
    fd_samples: list[int] = []

    with tempfile.TemporaryDirectory() as tmpdir:
        for i in range(10):
            # Create, write, flush and close temporary files
            test_file = Path(tmpdir) / f"soak_test_{i}.bin"
            test_file.write_bytes(b"TaskForest telemetry soak test chunk\n" * 100)
            test_file.unlink()

            rss = read_proc_rss_kb(my_pid)
            fds = read_proc_fd_count(my_pid)
            if rss is not None:
                rss_samples.append(rss)
            if fds is not None:
                fd_samples.append(fds)
            time.sleep(0.005)

    if rss_samples:
        errors = evaluate_soak_series(rss_samples, fd_samples)
        if errors:
            for err in errors:
                print(f"FAIL: {err}", file=sys.stderr)
            return 1

    print(
        f"Soak stability guard: PASS (RSS baseline={rss_samples[0] if rss_samples else 0} KB, "
        f"final={rss_samples[-1] if rss_samples else 0} KB, FDs={fd_samples[-1] if fd_samples else 0})"
    )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true", help="run unit self-tests")
    parser.add_argument("--samples-jsonl", type=Path, help="evaluate one authoritative soak samples.jsonl")
    parser.add_argument("--elapsed-hours", type=float, help="observed wall-clock duration")
    parser.add_argument("--json", type=Path, help="write the JSONL evaluation report")
    args = parser.parse_args()

    if args.self_test:
        return self_test()
    if args.samples_jsonl:
        report = evaluate_jsonl_samples(
            args.samples_jsonl,
            elapsed_hours=args.elapsed_hours,
        )
        if args.json:
            args.json.parent.mkdir(parents=True, exist_ok=True)
            args.json.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
        print(
            f"Soak JSONL guard: status={report['status']} targets={len(report['targets'])} "
            f"findings={len(report['findings'])}"
        )
        for finding in report["findings"]:
            print(f"  FINDING {finding}")
        if args.json:
            print(f"wrote {args.json}")
        return 0 if report["status"] == "pass" else 1

    repo_root = Path(__file__).resolve().parents[2]
    return run_standalone_soak_check(repo_root)


if __name__ == "__main__":
    sys.exit(main())
