#!/usr/bin/env python3
"""Per-crate line-coverage floors over an llvm-cov LCOV report.

The workspace-wide ``--fail-under-lines 71`` floor is a single global number:
hot crates can silently sink while coverage "moves" to well-tested ones. This
gate re-reads the SAME lcov.info and enforces a per-crate floor from
``docs/quality/coverage-floors.toml`` at zero extra compile cost.

Modes
  --check                 enforce floors against the given lcov file (default)
  --baseline [--write]    print measured values; with --write, seed
                          coverage-floors.toml at measured - 1.0 tolerance
  --self-test             run the internal correctness fixtures

Floors apply to crates listed in the TOML. Unlisted crates are reported
(``--report``) but never enforced — a crate joins the enforced set only via
a deliberate --baseline write. Floors only ever move up.

Exit codes: 0 pass / skip-no-data, 1 violation, 2 usage/config error.
"""

from __future__ import annotations

import argparse
import re
import sys
import tomllib
import tempfile
from pathlib import Path

FLOORS_DEFAULT = Path(__file__).resolve().parent.parent.parent / "docs" / "quality" / "coverage-floors.toml"
REPO = Path(__file__).resolve().parent.parent.parent

SF_RE = re.compile(r"^SF:(.+)$")
DA_RE = re.compile(r"^DA:(\d+),(\d+)$")


def crate_of_path(path: Path) -> str | None:
    relative = path.resolve()
    try:
        relative = relative.relative_to(REPO.resolve())
    except ValueError:
        return None
    parts = relative.parts
    if len(parts) >= 3 and parts[0] == "crates":
        return parts[1]
    if len(parts) >= 1 and parts[0] == "src":
        return "taskmanager"
    return None


def parse_lcov(text: str) -> dict[str, tuple[int, int]]:
    covered: dict[str, list[int]] = {}
    total: dict[str, list[int]] = {}
    # Split on the LCOV record terminator first: llvm-cov may omit the final
    # newline, so concatenated reports glue `end_of_record` to the next `SF:`
    # header. Record-scoped parsing attributes every DA line to the right file.
    for record in text.split("end_of_record"):
        current_file: str | None = None
        for line in record.splitlines():
            match = SF_RE.match(line)
            if match:
                current_file = match.group(1)
                continue
            match = DA_RE.match(line)
            if match and current_file is not None:
                crate = crate_of_path(Path(current_file))
                if crate is None:
                    continue
                covered.setdefault(crate, []).append(int(match.group(2)) > 0)
                total.setdefault(crate, []).append(True)
    result: dict[str, tuple[int, int]] = {}
    for crate in set(covered) | set(total):
        hits = sum(1 for hit in covered.get(crate, []) if hit)
        result[crate] = (hits, len(total.get(crate, [])))
    return result


def read_floors(path: Path) -> dict[str, float]:
    if not path.exists():
        raise ConfigError(f"floors file missing: {path} (run --baseline --write first)")
    with path.open("rb") as handle:
        data = tomllib.load(handle)
    floors = data.get("floors")
    if not isinstance(floors, dict):
        raise ConfigError(f"{path}: expected a [floors] table")
    result: dict[str, float] = {}
    for crate, value in floors.items():
        if not isinstance(value, (int, float)):
            raise ConfigError(f"{path}: floor for '{crate}' is not a number")
        result[crate] = float(value)
    return result


class ConfigError(Exception):
    pass


def format_report(measured: dict[str, tuple[int, int]], floors: dict[str, float]) -> list[str]:
    lines: list[str] = []
    width = max(len(name) for name in measured) if measured else 1
    for crate in sorted(measured):
        hits, total = measured[crate]
        pct = 100.0 * hits / total if total else 0.0
        floor = floors.get(crate)
        status = "ENFORCED" if floor is not None else "reported"
        verdict = "ok" if floor is None or pct >= floor else "BELOW"
        lines.append(f"{crate:<{width}} {pct:6.2f}%  floor={floor if floor is not None else '-':>6}  {status:8} {verdict}")
    return lines


def check(measured: dict[str, tuple[int, int]], floors: dict[str, float]) -> int:
    violations: list[str] = []
    for crate, floor in floors.items():
        if crate not in measured:
            violations.append(f"{crate}: no lines measured (typo in floors file?)")
            continue
        hits, total = measured[crate]
        pct = 100.0 * hits / total if total else 0.0
        if pct < floor:
            violations.append(f"{crate}: {pct:.2f}% < floor {floor}%")
    if violations:
        for line in violations:
            print(f"coverage floor violation: {line}")
        return 1
    return 0


def self_test() -> int:
    failures: list[str] = []
    sample = """\
SF:{repo}/crates/taskmanager-core/src/lib.rs
DA:1,1
DA:2,0
DA:3,1
SF:{repo}/crates/taskmanager-core/src/other.rs
DA:1,1
SF:{repo}/src/main.rs
DA:1,1
end_of_record
""".format(repo=REPO)
    measured = parse_lcov(sample)
    if measured != {"taskmanager-core": (3, 4), "taskmanager": (1, 1)}:
        failures.append(f"parse_lcov mapping wrong: {measured}")
    if crate_of_path(Path("/elsewhere/x.rs")) is not None:
        failures.append("crate_of_path must ignore outside paths")
    with tempfile.TemporaryDirectory() as scratch:
        floors_path = Path(scratch) / "floors.toml"
        floors_path.write_text("[floors]\ntaskmanager-core = 74.0\n", encoding="utf-8")
        floors = read_floors(floors_path)
        if floors != {"taskmanager-core": 74.0}:
            failures.append(f"read_floors wrong: {floors}")
        if check(measured, floors) != 0:
            failures.append("check must pass 75% vs 74% floor")
        if check(measured, {"taskmanager-core": 76.0}) != 1:
            failures.append("check must flag 75% vs 76% floor")
        floors_path.write_text("[floors]\nmissing-crate = 80.0\n", encoding="utf-8")
        if check(measured, read_floors(floors_path)) != 1:
            failures.append("check must flag a crate absent from the lcov")
    if failures:
        for failure in failures:
            print(f"self-test FAIL: {failure}")
        return 1
    print("per_crate_coverage_gate self-test: ok")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--lcov", default="target/lcov.info", help="path to the lcov report")
    parser.add_argument("--floors", default=str(FLOORS_DEFAULT), help="floors TOML path")
    parser.add_argument("--check", action="store_true", help="enforce floors (default)")
    parser.add_argument("--baseline", action="store_true", help="print measured values")
    parser.add_argument("--write", action="store_true", help="with --baseline: seed the floors file")
    parser.add_argument("--self-test", action="store_true", help="run internal fixtures")
    args = parser.parse_args()

    if args.self_test:
        return self_test()

    lcov_path = Path(args.lcov)
    if not lcov_path.exists():
        print(f"coverage-gate: SKIP (no lcov report at {lcov_path}; run the extended coverage stage first)")
        return 0

    measured = parse_lcov(lcov_path.read_text(encoding="utf-8"))
    if not measured:
        print("coverage-gate: no measurable lines found in lcov report")
        return 0

    if args.baseline:
        floors = {}
        try:
            floors = read_floors(Path(args.floors))
        except ConfigError:
            pass
        for line in format_report(measured, floors):
            print(line)
        if not args.write:
            return 0
        entries = {crate: round(100.0 * hits / total - 1.0, 1) for crate, (hits, total) in measured.items()}
        content = "# Per-crate line-coverage floors; bootstrapped by\n# scripts/quality/per_crate_coverage_gate.py --baseline --write.\n# Floors only move up; tolerance baked in at seed time is 1.0%.\n[floors]\n"
        for crate in sorted(entries):
            content += f"{crate} = {entries[crate]}\n"
        Path(args.floors).parent.mkdir(parents=True, exist_ok=True)
        Path(args.floors).write_text(content, encoding="utf-8")
        print(f"wrote {args.floors} with {len(entries)} crates (measured - 1.0 tolerance)")
        return 0

    try:
        floors = read_floors(Path(args.floors))
    except ConfigError as error:
        print(f"coverage-gate: config error: {error}")
        return 2
    for line in format_report(measured, floors):
        print(line)
    return check(measured, floors)


if __name__ == "__main__":
    sys.exit(main())
