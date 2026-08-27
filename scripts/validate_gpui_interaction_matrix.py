#!/usr/bin/env python3
"""Validate the GPUI interaction matrix against nextest discovery and receipts."""

from __future__ import annotations

import argparse
import csv
import json
import sys
from pathlib import Path


MATRIX_FIELDS = (
    "case_id",
    "p0_id",
    "target",
    "test_name",
    "paths",
    "capture_scenarios",
)
ALLOWED_TARGETS = {"gui", "lib"}
ALLOWED_PATHS = {
    "cancel",
    "evidence",
    "failure",
    "focus",
    "isolation",
    "keyboard",
    "lifecycle",
    "pointer",
    "provider-gap",
    "recovery",
    "responsive",
    "success",
    "toggle",
}


class MatrixError(RuntimeError):
    """Raised when the acceptance contract is malformed or incomplete."""


def read_tsv(path: Path, fields: tuple[str, ...]) -> list[dict[str, str]]:
    with path.open("r", encoding="utf-8", newline="") as handle:
        reader = csv.DictReader(handle, delimiter="\t")
        if tuple(reader.fieldnames or ()) != fields:
            raise MatrixError(f"{path}: expected fields {fields}, got {reader.fieldnames}")
        rows = list(reader)
    if not rows:
        raise MatrixError(f"{path}: matrix is empty")
    if any(any(value is None for value in row.values()) for row in rows):
        raise MatrixError(f"{path}: malformed row")
    return rows


def requirement_ids(path: Path) -> set[str]:
    lines = [
        line.strip()
        for line in path.read_text(encoding="utf-8").splitlines()
        if line.strip() and not line.lstrip().startswith("#")
    ]
    if not lines or lines[0] != "requirement_id":
        raise MatrixError(f"{path}: expected requirement_id header")
    ids: set[str] = set()
    for line in lines[1:]:
        if "\t" in line or line in ids:
            raise MatrixError(f"{path}: malformed or duplicate requirement row")
        ids.add(line)
    if not ids:
        raise MatrixError(f"{path}: no requirement IDs")
    return ids


def capture_ids(path: Path) -> set[str]:
    rows = read_tsv(path, (
        "name",
        "skin",
        "page",
        "device",
        "settings",
        "scenario",
        "window_size",
        "capture_size",
    ))
    return {row["name"] for row in rows}


def validate_rows(
    rows: list[dict[str, str]], requirements: set[str], captures: set[str]
) -> dict[str, object]:
    case_ids: set[str] = set()
    matrix_ids: set[str] = set()
    tests_by_target: dict[str, set[str]] = {target: set() for target in ALLOWED_TARGETS}
    path_coverage: dict[str, set[str]] = {p0_id: set() for p0_id in requirements}
    for row in rows:
        case_id = row["case_id"]
        p0_id = row["p0_id"]
        target = row["target"]
        test_name = row["test_name"]
        if not case_id or case_id in case_ids:
            raise MatrixError(f"duplicate or empty case_id: {case_id!r}")
        if p0_id not in requirements:
            raise MatrixError(f"{case_id}: unknown requirement ID {p0_id!r}")
        if target not in ALLOWED_TARGETS:
            raise MatrixError(f"{case_id}: invalid target {target!r}")
        if not test_name or test_name in tests_by_target[target]:
            raise MatrixError(f"{case_id}: duplicate or empty test_name for {target}")
        paths = {item for item in row["paths"].split("|") if item}
        if not paths or not paths.issubset(ALLOWED_PATHS):
            raise MatrixError(f"{case_id}: invalid paths {sorted(paths)}")
        scenario_names = {
            item for item in row["capture_scenarios"].split("|") if item and item != "-"
        }
        missing_scenarios = scenario_names - captures
        if missing_scenarios:
            raise MatrixError(
                f"{case_id}: unknown capture scenarios {sorted(missing_scenarios)}"
            )
        case_ids.add(case_id)
        matrix_ids.add(p0_id)
        tests_by_target[target].add(test_name)
        path_coverage[p0_id].update(paths)

    if matrix_ids != requirements:
        raise MatrixError(
            "matrix/requirement IDs differ: "
            f"missing={sorted(requirements - matrix_ids)}, extra={sorted(matrix_ids - requirements)}"
        )
    missing_success = sorted(p0_id for p0_id, paths in path_coverage.items() if "success" not in paths)
    if missing_success:
        raise MatrixError(f"P0 rows without a success path: {missing_success}")
    return {
        "case_count": len(rows),
        "p0_count": len(matrix_ids),
        "tests_by_target": {target: sorted(names) for target, names in tests_by_target.items()},
        "path_coverage": {p0_id: sorted(paths) for p0_id, paths in sorted(path_coverage.items())},
    }


def nextest_tests(path: Path, target: str) -> set[str]:
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise MatrixError(f"{path}: invalid nextest list JSON") from error
    suites = payload.get("rust-suites")
    if not isinstance(suites, dict):
        raise MatrixError(f"{path}: missing rust-suites")
    found: set[str] = set()
    for suite in suites.values():
        if not isinstance(suite, dict) or suite.get("kind") != ("test" if target == "gui" else "lib"):
            continue
        if target == "gui" and suite.get("binary-name") != "gui":
            continue
        cases = suite.get("testcases")
        if isinstance(cases, dict):
            found.update(cases)
    if not found:
        raise MatrixError(f"{path}: no {target} test cases discovered")
    return found


def validate_discovery(summary: dict[str, object], gui: set[str], lib: set[str]) -> None:
    tests_by_target = summary["tests_by_target"]
    assert isinstance(tests_by_target, dict)
    for target, available in (("gui", gui), ("lib", lib)):
        expected = set(tests_by_target[target])
        missing = sorted(expected - available)
        if missing:
            raise MatrixError(f"{target}: matrix tests were not discovered: {missing}")


def run_tests_from_logs(paths: list[Path], expected: dict[str, set[str]]) -> dict[str, object]:
    started: dict[str, set[str]] = {target: set() for target in expected}
    passed: dict[str, set[str]] = {target: set() for target in expected}
    failed: list[str] = []
    for path in paths:
        for raw_line in path.read_text(encoding="utf-8").splitlines():
            if not raw_line.startswith("{"):
                continue
            try:
                event = json.loads(raw_line)
            except json.JSONDecodeError:
                continue
            name = event.get("name")
            if not isinstance(name, str) or "$" not in name:
                continue
            suite, test_name = name.split("$", 1)
            # `gui` rows run in the root package's gui integration binary
            # (`taskmanager::gui`); the `lib` rows live in the root package lib
            # and (since the GPUI crate split) taskmanager-gpui's lib binary.
            # The suite suffix is the TEST BINARY name, which normalizes the
            # crate name to underscores: `taskmanager::taskmanager` and
            # `taskmanager-gpui::taskmanager_gpui`.
            target = (
                "gui"
                if suite.endswith("::gui")
                else "lib"
                if suite.endswith("::taskmanager") or suite.endswith("::taskmanager_gpui")
                else ""
            )
            if target not in expected or test_name not in expected[target]:
                continue
            if event.get("event") == "started":
                started[target].add(test_name)
            elif event.get("event") == "ok":
                passed[target].add(test_name)
            elif event.get("event") == "failed":
                failed.append(test_name)
    missing = {
        target: sorted(names - passed[target])
        for target, names in expected.items()
        if names - passed[target]
    }
    if missing or failed:
        raise MatrixError(f"interaction receipt incomplete: missing={missing}, failed={sorted(failed)}")
    return {
        "passed_by_target": {target: len(names) for target, names in passed.items()},
        "started_by_target": {target: len(names) for target, names in started.items()},
        "failed": sorted(failed),
    }


def write_receipt(path: Path | None, payload: dict[str, object]) -> None:
    if path is not None:
        path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def self_test() -> None:
    rows = [
        {
            "case_id": "case",
            "p0_id": "P0",
            "target": "gui",
            "test_name": "one",
            "paths": "success|failure",
            "capture_scenarios": "-",
        }
    ]
    summary = validate_rows(rows, {"P0"}, set())
    assert summary["case_count"] == 1
    try:
        validate_rows(rows + [rows[0]], {"P0"}, set())
    except MatrixError:
        pass
    else:
        raise AssertionError("duplicate cases must be rejected")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--matrix", type=Path)
    parser.add_argument("--requirements", type=Path)
    parser.add_argument("--capture-matrix", type=Path)
    parser.add_argument("--gui-list", type=Path)
    parser.add_argument("--lib-list", type=Path)
    parser.add_argument("--run-log", type=Path, action="append", default=[])
    parser.add_argument("--receipt", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("GPUI interaction matrix validator self-test: PASS")
        return 0
    required = (args.matrix, args.requirements, args.capture_matrix, args.gui_list, args.lib_list)
    if any(path is None for path in required):
        parser.error("--matrix, --requirements, --capture-matrix, --gui-list and --lib-list are required")
    assert args.matrix and args.requirements and args.capture_matrix and args.gui_list and args.lib_list
    try:
        summary = validate_rows(
            read_tsv(args.matrix, MATRIX_FIELDS),
            requirement_ids(args.requirements),
            capture_ids(args.capture_matrix),
        )
        gui = nextest_tests(args.gui_list, "gui")
        lib = nextest_tests(args.lib_list, "lib")
        validate_discovery(summary, gui, lib)
        receipt: dict[str, object] = {
            "status": "pass",
            "matrix": summary,
            "discovered": {"gui": len(gui), "lib": len(lib)},
        }
        if args.run_log:
            receipt["run"] = run_tests_from_logs(
                args.run_log,
                {target: set(names) for target, names in summary["tests_by_target"].items()},
            )
        write_receipt(args.receipt, receipt)
    except MatrixError as error:
        print(f"GPUI interaction matrix: FAIL: {error}", file=sys.stderr)
        return 1
    print("GPUI interaction matrix: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
