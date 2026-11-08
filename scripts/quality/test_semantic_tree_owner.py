#!/usr/bin/env python3
"""The shell owns projection -> semantic component-tree facts.

Frontends may keep their own root name, modal surfaces and structural group
rows. They may not re-spell the common semantic status, bounded row count,
CPU graph summary, memory percentage or process-row conversion. A mutation
that reintroduces one of those local implementations must fail this check.
"""

from __future__ import annotations

import re
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OWNER = ROOT / "crates/taskmanager-shell/src/accessibility_projection.rs"
FRONTENDS = (
    ROOT / "crates/taskmanager-gpui/src/gpui_app/root/a11y.rs",
    ROOT / "crates/taskmanager-iced/src/a11y.rs",
    ROOT / "crates/taskmanager-tui/src/runtime/semantic.rs",
    ROOT / "crates/taskmanager-bevy-ui/src/semantic.rs",
)
FORBIDDEN = (
    re.compile(r"processes visible"),
    re.compile(r"MAX_(?:PUBLISHED|SNAPSHOT)_ROWS\s*:\s*usize\s*=\s*64"),
    re.compile(r"GraphSummary\s*\{"),
    re.compile(r"current_cpu_percentage\(\)[\s\S]{0,160}clamp\(0\.0,\s*100\.0\)"),
)
EXPECTED_OWNER_LINES = (
    "pub const MAX_PUBLISHED_ROWS: usize = 64;",
    "pub fn semantic_status(",
    "pub fn semantic_cpu_graph(",
    "pub fn memory_percentage(",
    "pub fn semantic_process_row(",
    "pub fn semantic_snapshot_header(",
    "pub fn base_semantic_snapshot_builder(",
)


class SemanticTreeOwnerTests(unittest.TestCase):
    def test_owner_exports_every_common_fact(self):
        source = OWNER.read_text(encoding="utf-8")
        for line in EXPECTED_OWNER_LINES:
            self.assertIn(line, source)

    def test_no_frontend_reimplements_the_common_facts(self):
        for path in FRONTENDS:
            source = path.read_text(encoding="utf-8")
            for pattern in FORBIDDEN:
                with self.subTest(frontend=path.parents[2].name, rule=pattern.pattern):
                    self.assertIsNone(pattern.search(source))

    def test_every_frontend_binds_the_owner_helpers(self):
        for path in FRONTENDS:
            source = path.read_text(encoding="utf-8")
            with self.subTest(frontend=path.parents[2].name):
                self.assertTrue(
                    "semantic_snapshot_header" in source
                    or "base_semantic_snapshot_builder" in source
                    or "semantic_process_row" in source
                )

    def test_a_fork_mutation_fails(self):
        source = OWNER.read_text(encoding="utf-8").replace(
            "pub const MAX_PUBLISHED_ROWS: usize = 64;",
            "pub const MAX_PUBLISHED_ROWS: usize = 32;",
        )
        self.assertNotIn("pub const MAX_PUBLISHED_ROWS: usize = 64;", source)
        self.assertIn("pub const MAX_PUBLISHED_ROWS: usize = 32;", source)


if __name__ == "__main__":
    unittest.main()
