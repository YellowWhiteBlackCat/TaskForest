#!/usr/bin/env python3
"""Verify the GPUI per-target interaction receipt against nextest discovery.

The structural authority for the interaction contract is no longer here:

* the unified matrix (``scripts/parity/cross_frontend_matrix.tsv``) owns the row
  declarations and the ``contract_tag == first paths token`` rule;
* the Rust ``ContractTag`` conformance owns the token vocabulary;
* ``tests/logic/gpui_interaction_matrix_test.rs`` owns the GPUI projection
  (pinned row count, interaction path vocabulary, case-prefix discipline,
  requirement and capture-scenario coverage);
* the unified resolver (``resolve_frontend_evidence.py``) resolves every
  declared anchor against discovery, including the GPUI stable case-prefix
  channel.

What remains in this module is the one guarantee no other check owns: every
discovered GPUI interaction anchor ran to ``ok`` in its own nextest target
(``gui`` or ``lib``).  A flat discovery set cannot prove that, so the two target
artifacts and the two run logs are checked separately.
"""

from __future__ import annotations

import argparse
import csv
import json
import re
import sys
import tempfile
from pathlib import Path


ALLOWED_TARGETS = ("gui", "lib")
REQUIRED_COLUMNS = ("case_id", "target")


class MatrixError(RuntimeError):
    """Raised when the acceptance receipt cannot be resolved or is incomplete."""


def read_rows(path: Path) -> list[dict[str, str]]:
    """Read the interaction rows that seed the receipt.

    Only the columns the receipt needs are required; the row schema and its
    semantics are owned by the unified matrix and its Rust gate, so this reader
    does not re-declare them.
    """
    if not path.is_file():
        raise MatrixError(f"{path}: matrix not found")
    with path.open("r", encoding="utf-8", newline="") as handle:
        data_lines = [
            line
            for line in handle
            if line.strip() and not line.lstrip().startswith("#")
        ]
    reader = csv.DictReader(data_lines, delimiter="\t")
    fieldnames = tuple(reader.fieldnames or ())
    missing = [column for column in REQUIRED_COLUMNS if column not in fieldnames]
    if missing:
        raise MatrixError(
            f"{path}: missing required column(s) {missing} (got {fieldnames})"
        )
    rows: list[dict[str, str]] = []
    for row in reader:
        if None in row:
            raise MatrixError(f"{path}: malformed row (too many fields): {row[None]!r}")
        if not (row.get("case_id") or "").strip() or not (row.get("target") or "").strip():
            raise MatrixError(f"{path}: malformed row (missing case_id or target)")
        rows.append(row)
    if not rows:
        raise MatrixError(f"{path}: matrix has no data rows")
    return rows


def case_token(case_id: str) -> str:
    """Map a stable kebab-case matrix ID to its Rust test-name prefix."""
    return case_id.replace("-", "_")


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


def expected_by_target(
    rows: list[dict[str, str]], gui: set[str], lib: set[str]
) -> dict[str, set[str]]:
    """Resolve every declared case to the tests its own target must have run.

    GPUI declares its anchors through the stable case-prefix channel
    (``<case_id>_case_*``); the channel itself is owned by the unified matrix and
    the resolver's ``CASE_PREFIX_CHANNELS``.  Here it only seeds the per-target
    receipt, and the scan also rejects an acceptance-tagged test that has no
    matrix contract at all (a completeness rule the resolver does not own).
    """
    available = {"gui": gui, "lib": lib}
    discovered: dict[str, set[str]] = {target: set() for target in ALLOWED_TARGETS}
    tokens: dict[str, str] = {}
    for row in rows:
        case_id = row["case_id"].strip()
        target = (row.get("target") or "").strip()
        if target not in available:
            raise MatrixError(f"{case_id}: unknown target {target!r}")
        token = case_token(case_id)
        if token in tokens:
            raise MatrixError(f"case IDs collide after Rust normalization: {case_id!r}")
        tokens[token] = case_id
        prefix = f"{token}_case_"
        matches = sorted(
            name
            for name in available[target]
            if name.rsplit("::", 1)[-1].startswith(prefix)
        )
        if not matches:
            raise MatrixError(
                f"{target}: no nextest test advertises stable case prefix {prefix!r}"
            )
        discovered[target].update(matches)

    unregistered = []
    for target in ALLOWED_TARGETS:
        for name in available[target]:
            leaf = name.rsplit("::", 1)[-1]
            tag, separator, _ = leaf.partition("_case_")
            if separator and re.fullmatch(r"mc[0-9]{2}(?:_[a-z0-9]+)+", tag):
                if tag not in tokens:
                    unregistered.append(name)
    if unregistered:
        raise MatrixError(
            f"acceptance-tagged tests have no matrix contract: {sorted(unregistered)}"
        )
    return discovered


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


def log_line(suite: str, test_name: str) -> str:
    return json.dumps({"type": "test", "event": "ok", "name": f"{suite}${test_name}"}) + "\n"


def self_test() -> None:
    """Prove the receipt is per target and fails closed on any gap."""
    gui_name = "module::mc00_case_case_behavior"
    lib_name = "libmod::mc01_case_case_behavior"
    rows = [
        {"case_id": "mc00-case", "target": "gui"},
        {"case_id": "mc01-case", "target": "lib"},
    ]
    expected = expected_by_target(rows, {gui_name}, {lib_name})
    assert expected == {"gui": {gui_name}, "lib": {lib_name}}, expected

    # The prefix channel is total: an acceptance-tagged test with no matrix
    # contract is rejected.
    try:
        expected_by_target(rows, {gui_name, "module::mc99_stray_case_behavior"}, {lib_name})
    except MatrixError:
        pass
    else:
        raise AssertionError("an unregistered acceptance-tagged test must be rejected")

    with tempfile.TemporaryDirectory(prefix="gpui-receipt-selftest-") as raw:
        root = Path(raw)
        matrix = root / "matrix.tsv"
        matrix.write_text("case_id\ttarget\nmc00-case\tgui\n", encoding="utf-8")
        assert [row["case_id"] for row in read_rows(matrix)] == ["mc00-case"]
        # A malformed row must fail, never be silently skipped out of the receipt.
        matrix.write_text("case_id\ttarget\nmc00-case\n", encoding="utf-8")
        try:
            read_rows(matrix)
        except MatrixError:
            pass
        else:
            raise AssertionError("a malformed row must be rejected, not skipped")

        gui_log = root / "gui-run.log"
        lib_log = root / "lib-run.log"
        gui_log.write_text(
            log_line("taskmanager-gpui::gui", gui_name), encoding="utf-8"
        )
        lib_log.write_text(
            log_line("taskmanager-gpui::taskmanager_gpui", lib_name), encoding="utf-8"
        )
        receipt = run_tests_from_logs([gui_log, lib_log], expected)
        assert receipt["passed_by_target"] == {"gui": 1, "lib": 1}, receipt

        # A `gui` anchor that only ran in the `lib` target is not covered: the
        # receipt is per target, not a flat discovery set.
        gui_log.write_text(
            log_line("taskmanager-gpui::taskmanager_gpui", gui_name), encoding="utf-8"
        )
        try:
            run_tests_from_logs([gui_log, lib_log], expected)
        except MatrixError:
            pass
        else:
            raise AssertionError("a cross-target ok must not satisfy the gui receipt")

        # A missing `ok` event fails closed.
        gui_log.write_text("", encoding="utf-8")
        try:
            run_tests_from_logs([gui_log, lib_log], expected)
        except MatrixError:
            pass
        else:
            raise AssertionError("an incomplete receipt must fail closed")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--matrix", type=Path)
    parser.add_argument("--gui-list", type=Path)
    parser.add_argument("--lib-list", type=Path)
    parser.add_argument("--run-log", type=Path, action="append", default=[])
    parser.add_argument("--receipt", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("GPUI interaction receipt validator self-test: PASS")
        return 0
    required = (args.matrix, args.gui_list, args.lib_list)
    if any(path is None for path in required):
        parser.error("--matrix, --gui-list and --lib-list are required")
    assert args.matrix and args.gui_list and args.lib_list
    try:
        rows = read_rows(args.matrix)
        gui = nextest_tests(args.gui_list, "gui")
        lib = nextest_tests(args.lib_list, "lib")
        expected = expected_by_target(rows, gui, lib)
        cases_by_target: dict[str, list[str]] = {}
        for row in rows:
            cases_by_target.setdefault(row["target"].strip(), []).append(
                row["case_id"].strip()
            )
        receipt: dict[str, object] = {
            "status": "pass",
            "matrix": {
                "case_count": len(rows),
                "cases_by_target": {
                    target: sorted(names)
                    for target, names in sorted(cases_by_target.items())
                },
            },
            "discovered": {"gui": len(gui), "lib": len(lib)},
            "tests_by_target": {
                target: sorted(names) for target, names in expected.items()
            },
        }
        if args.run_log:
            receipt["run"] = run_tests_from_logs(
                args.run_log,
                expected,
            )
        write_receipt(args.receipt, receipt)
    except MatrixError as error:
        print(f"GPUI interaction matrix: FAIL: {error}", file=sys.stderr)
        return 1
    print("GPUI interaction matrix: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
