#!/usr/bin/env python3
"""One command-launch draft owner, one gate.

`CommandLaunchRequest::from_draft` owns CRLF/CR normalization and the trim that
turns a frontend editor draft into the typed platform request. A frontend that
re-implements `replace("\\r\\n", "\\n")` or constructs a launch request with a
manual `trim()` owns a second command fact.
"""

from __future__ import annotations

import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CRATES = ROOT / "crates"
OWNER = CRATES / "taskmanager-application/src/platform/facets/integration.rs"
FRONTENDS = (
    CRATES / "taskmanager-gpui/src",
    CRATES / "taskmanager-iced/src",
    CRATES / "taskmanager-bevy-ui/src",
    CRATES / "taskmanager-tui/src",
)
LOCAL_GATES = ROOT / "scripts/quality/local-gates.sh"

NORMALIZATION = re.compile(r"replace\s*\(\s*[fr]?[\"']\\r(?:\\n)?[\"']")
MANUAL_REQUEST = re.compile(
    r"CommandLaunchRequest\s*\{[^}]*command\s*:[^}]*\.trim\s*\(\s*\)",
    re.DOTALL,
)
FROM_DRAFT = "CommandLaunchRequest::from_draft"


class CommandLaunchOwnerTests(unittest.TestCase):
    def test_the_application_owner_publishes_the_fold(self):
        source = OWNER.read_text(encoding="utf-8")
        self.assertIn("pub fn from_draft(draft: &str) -> Option<Self>", source)
        self.assertIn('draft.replace("\\r\\n", "\\n")', source)
        self.assertIn("normalized.trim()", source)

    def test_the_quick_gate_runs_this_check(self):
        source = LOCAL_GATES.read_text(encoding="utf-8")
        self.assertIn("scripts/quality/test_command_launch_owner.py", source)

    def test_frontends_bind_the_owner_instead_of_forking_it(self):
        bindings: list[str] = []
        offenders: list[str] = []
        for root in FRONTENDS:
            for path in sorted(root.rglob("*.rs")):
                source = path.read_text(encoding="utf-8")
                if NORMALIZATION.search(source):
                    offenders.append(
                        f"{path.relative_to(ROOT).as_posix()}: local newline normalization"
                    )
                if MANUAL_REQUEST.search(source):
                    offenders.append(
                        f"{path.relative_to(ROOT).as_posix()}: manually trimmed launch request"
                    )
                if FROM_DRAFT in source:
                    bindings.append(path.relative_to(ROOT).as_posix())
        self.assertEqual(offenders, [])
        self.assertTrue(
            any("taskmanager-gpui" in path for path in bindings)
            and any("taskmanager-iced" in path for path in bindings),
            "both command-launch frontends must bind the shared fold",
        )

    def test_the_detector_bites_a_fork(self):
        self.assertTrue(NORMALIZATION.search('draft.replace("\\r\\n", "\\n")'))
        self.assertTrue(
            MANUAL_REQUEST.search(
                "CommandLaunchRequest { command: draft.trim().to_owned() }"
            )
        )
        self.assertFalse(MANUAL_REQUEST.search("CommandLaunchRequest::from_draft(draft)"))


if __name__ == "__main__":
    unittest.main(verbosity=2)
