#!/usr/bin/env python3
"""One Iced scroll direction/material owner, one gate.

`crate::ui::components::primitives` owns the Iced scrollbar geometry and
glass rail style. Plain pages consume `scrollable(...)`; a kit
`virtual_table` cannot consume that widget, so it consumes
`virtual_table_direction(...)`, which resolves the SAME
`ScrollScale::Standard/Compact` scrollbar. Four pages used to construct
`Direction::Vertical(theme::scrollbar())` (and one `Direction::Both`) in
place, re-opening a second rail-material authority.

This gate refuses a page that constructs a scroll direction itself or calls
`theme::scrollbar*` outside the component owner. The message/app types may
still name `iced::widget::scrollable::Viewport`; that is a typed event, not
a direction.

Usage::

    python3 scripts/quality/test_iced_scroll_owner.py
"""

from __future__ import annotations

import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CRATES = ROOT / "crates"
LOCAL_GATES = ROOT / "scripts/quality/local-gates.sh"

OWNER = CRATES / "taskmanager-iced/src/ui/components/primitives.rs"
OWNER_SYMBOLS = (
    "pub(crate) fn scrollable<'a, Message: 'a>(",
    "pub(crate) fn virtual_table_direction(",
    "pub(crate) enum ScrollScale",
    "pub(crate) enum VirtualTableScrollAxis",
)

PAGES = CRATES / "taskmanager-iced/src/ui"
EXCLUDED = {OWNER}

LOCAL_DIRECTION = re.compile(
    r"(?:iced::widget::scrollable::)?Direction::\s*(?:Vertical|Horizontal|Both)\b"
)
DIRECT_BAR = re.compile(r"\b(?:crate::)?theme::(?:compact_)?scrollbar\s*\(\s*\)")

BINDING_FILES = (
    "startup_table.rs",
    "tables.rs",
    "users.rs",
    "applications/page.rs",
)

BLOCK_COMMENT = re.compile(r"/\*.*?\*/", re.DOTALL)
LINE_COMMENT = re.compile(r"//[^\n]*")
STRING_LITERAL = re.compile(r'"((?:[^"\\]|\\.)*)"')


def code_only(source: str) -> str:
    text = BLOCK_COMMENT.sub(lambda m: re.sub(r"[^\n]", " ", m.group(0)), source)
    text = LINE_COMMENT.sub(lambda m: " " * len(m.group(0)), text)
    return STRING_LITERAL.sub(lambda m: re.sub(r"[^\n]", " ", m.group(0)), text)


class IcedScrollOwnerTests(unittest.TestCase):
    def test_the_owner_publishes_its_surface(self):
        self.assertTrue(OWNER.is_file(), OWNER)
        source = OWNER.read_text(encoding="utf-8")
        for symbol in OWNER_SYMBOLS:
            with self.subTest(symbol=symbol):
                self.assertIn(symbol, source)

    def test_the_quick_gate_runs_this_check(self):
        source = LOCAL_GATES.read_text(encoding="utf-8")
        self.assertIn(
            "scripts/quality/test_iced_scroll_owner.py",
            source,
            "the Iced scroll owner test must be wired into quick",
        )

    def test_no_page_constructs_a_direction_or_a_bar_itself(self):
        offenders: list[str] = []
        for path in sorted(PAGES.rglob("*.rs")):
            if path in EXCLUDED:
                continue
            masked = code_only(path.read_text(encoding="utf-8"))
            for number, line in enumerate(masked.splitlines(), start=1):
                if LOCAL_DIRECTION.search(line) or DIRECT_BAR.search(line):
                    offenders.append(
                        f"{path.relative_to(CRATES).as_posix()}:{number}: {line.strip()}"
                    )
        self.assertEqual(
            offenders,
            [],
            "Iced pages spend the component owner's direction and bar:\n"
            + "\n".join(offenders),
        )

    def test_the_kit_tables_bind_the_direction_owner(self):
        for relative in BINDING_FILES:
            path = PAGES / relative
            source = path.read_text(encoding="utf-8")
            with self.subTest(page=relative):
                self.assertIn("virtual_table_direction(", source)

    def test_the_detector_bites_a_fork_and_spares_a_binding(self):
        self.assertTrue(
            LOCAL_DIRECTION.search(
                "Direction::Vertical(crate::theme::scrollbar())"
            )
        )
        self.assertTrue(DIRECT_BAR.search("theme::scrollbar()"))
        binding = "virtual_table_direction(VirtualTableScrollAxis::Both, ScrollScale::Standard)"
        self.assertFalse(LOCAL_DIRECTION.search(binding))
        self.assertFalse(DIRECT_BAR.search(binding))


if __name__ == "__main__":
    unittest.main(verbosity=2)
