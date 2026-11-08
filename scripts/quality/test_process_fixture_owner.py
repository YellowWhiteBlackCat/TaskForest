#!/usr/bin/env python3
"""The two process-page test modules bind one shared fixture owner."""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
PAGES = ROOT / "crates/taskmanager-bevy-ui/tests/headless/pages"
CONSUMERS = (
    PAGES / "processes.rs",
    PAGES / "processes_table_parity.rs",
)
OWNER = PAGES / "process_fixture_support.rs"
SOURCE = ROOT / "crates/taskmanager-bevy-ui/src/pages/processes.rs"
LOCAL_HELPER = re.compile(r"^fn\s+(?:process|shell_with)\s*\(", re.MULTILINE)
OWNER_IMPORT = "super::process_fixture_support"


def main() -> int:
    problems: list[str] = []
    if not OWNER.is_file():
        problems.append(f"missing shared fixture owner: {OWNER}")
    for path in CONSUMERS:
        source = path.read_text(encoding="utf-8")
        if LOCAL_HELPER.search(source):
            problems.append(f"{path.name}: local process fixture helper has reappeared")
        if OWNER_IMPORT not in source:
            problems.append(f"{path.name}: does not import the shared fixture owner")
    declaration = f'#[path = "../../tests/headless/pages/process_fixture_support.rs"]'
    if declaration not in SOURCE.read_text(encoding="utf-8"):
        problems.append("src/pages/processes.rs does not mount the shared fixture owner")
    for problem in problems:
        print(f"process fixture owner: FAIL {problem}", file=sys.stderr)
    if problems:
        return 1
    print("process fixture owner: one owner, two consumers — PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
