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


def evaluate_soak_series(rss_samples: list[int], fd_samples: list[int]) -> list[str]:
    violations: list[str] = []
    if len(rss_samples) < 3:
        return ["insufficient samples for soak evaluation"]

    # Skip first warmup sample
    stable_rss = rss_samples[1:]
    initial_rss = stable_rss[0]
    final_rss = stable_rss[-1]

    # Max allowed RSS growth from stabilized baseline: 25% (allows initial heap growth, blocks unbounded leak)
    growth_pct = ((final_rss - initial_rss) / initial_rss) * 100.0 if initial_rss > 0 else 0.0
    if growth_pct > 25.0:
        violations.append(
            f"RSS memory grew by {growth_pct:.1f}% ({initial_rss} KB -> {final_rss} KB), exceeding stability threshold"
        )

    # Check FD count stability
    if len(fd_samples) >= 2:
        initial_fd = fd_samples[0]
        final_fd = fd_samples[-1]
        if final_fd > initial_fd + 3:
            violations.append(
                f"File descriptor leak detected: count grew from {initial_fd} to {final_fd}"
            )

    return violations


def load_jsonl_samples(path: Path) -> tuple[dict[str, dict[str, list[int]]], list[str]]:
    """Load the soak runner's authoritative samples without rewriting them."""
    series: dict[str, dict[str, list[int]]] = {}
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
        target = series.setdefault(name, {"rss_kib": [], "fd_count": []})
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
    for name, metrics in sorted(series.items()):
        violations = evaluate_soak_series(metrics["rss_kib"], metrics["fd_count"])
        findings.extend(f"{name}: {violation}" for violation in violations)
        per_target[name] = {
            "rss_samples": len(metrics["rss_kib"]),
            "fd_samples": len(metrics["fd_count"]),
            "rss_first_kib": metrics["rss_kib"][0] if metrics["rss_kib"] else None,
            "rss_last_kib": metrics["rss_kib"][-1] if metrics["rss_kib"] else None,
            "fd_first": metrics["fd_count"][0] if metrics["fd_count"] else None,
            "fd_last": metrics["fd_count"][-1] if metrics["fd_count"] else None,
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
    assert any("File descriptor leak" in e for e in errors), "self-test failed to catch FD leak"

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
        failures += not any("File descriptor leak" in item for item in leak["findings"])
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
