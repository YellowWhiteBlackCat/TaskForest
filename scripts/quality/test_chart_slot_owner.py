#!/usr/bin/env python3
"""One sample-window → pixel-slot grid, one owner (ADR-020).

Every chart in the product maps a window of samples onto pixels, and the
drawn sample and the hover readout must resolve through the *same* grid —
otherwise the crosshair points beside the value it reports. GPUI
(`sample_x` / `sample_index_at_cursor_x`), Iced (`WindowSlots`) and Bevy
(`ChartSlots`) each carried their own copy of that mapping, and the copies
had already drifted: one answer for a single-sample window, one answer for a
cursor on the right edge, and two rounding orders for the same slot.

The owner is `taskmanager_shell::presentation::ChartSlots` (toolkit-neutral,
shared by all four frontends). This check fails the shapes that made the
drift possible in the first place:

* a frontend that declares its own slot-grid type (`WindowSlots`,
  `ChartSlots`) or its own `denominator` helper;
* a frontend that re-implements the denominator law inline
  (`saturating_sub(1).max(... saturating_sub(1)).max(1)`);
* a surface that stops importing the owner.

Usage::

    python3 scripts/quality/test_chart_slot_owner.py
    python3 scripts/quality/test_chart_slot_owner.py --self-test
"""

from __future__ import annotations

import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CRATES = ROOT / "crates"

# The owner: one sample window → one pixel-slot grid.
OWNER = CRATES / "taskmanager-shell/src/presentation/chart_slots.rs"
OWNER_SYMBOLS = (
    "pub struct ChartSlots",
    "pub fn new(",
    "pub fn spread(",
    "pub fn denominator(",
    "pub fn x(",
    "pub fn index_at(",
)

# The frontends that render charts. Each of them must bind the owner.
SURFACES = (
    "taskmanager-gpui",
    "taskmanager-iced",
    "taskmanager-bevy-ui",
)

# A surface-local copy of the grid type, or of its denominator helper.
LOCAL_GRID_TYPES = re.compile(r"\bstruct\s+(WindowSlots|ChartSlots)\b")
LOCAL_DENOMINATOR = re.compile(r"\bfn\s+denominator\b")

# The denominator law itself, spelled out: the sample span, the capacity span,
# then one. A surface that writes this expression has a second authority.
DENOMINATOR_LAW = re.compile(
    r"saturating_sub\(\s*1\s*\)"
    r"\s*\.max\([^)]*saturating_sub\(\s*1\s*\)\s*\)"
    r"\s*\.max\(\s*1\s*\)"
)

OWNER_IMPORT = re.compile(r"taskmanager_shell::presentation::(?:ChartSlots|chart_slots)")


def surface_sources() -> list[Path]:
    """Every production source file of the chart surfaces."""
    files: list[Path] = []
    for crate in SURFACES:
        src = CRATES / crate / "src"
        if src.is_dir():
            files.extend(sorted(src.rglob("*.rs")))
    return files


class ChartSlotOwnerTests(unittest.TestCase):
    def test_the_owner_publishes_one_grid(self):
        self.assertTrue(OWNER.is_file(), OWNER)
        source = OWNER.read_text(encoding="utf-8")
        for symbol in OWNER_SYMBOLS:
            with self.subTest(symbol=symbol):
                self.assertIn(symbol, source)

    def test_no_surface_declares_its_own_slot_grid(self):
        offenders: list[str] = []
        for path in surface_sources():
            source = path.read_text(encoding="utf-8")
            for number, line in enumerate(source.splitlines(), start=1):
                if LOCAL_GRID_TYPES.search(line) or LOCAL_DENOMINATOR.search(line):
                    offenders.append(
                        f"{path.relative_to(CRATES).as_posix()}:{number}: {line.strip()}"
                    )
        self.assertEqual(
            offenders,
            [],
            "a surface may not declare its own sample-window grid:\n"
            + "\n".join(offenders),
        )

    def test_no_surface_re_implements_the_denominator_law(self):
        offenders: list[str] = []
        for path in surface_sources():
            source = path.read_text(encoding="utf-8")
            for number, line in enumerate(source.splitlines(), start=1):
                if DENOMINATOR_LAW.search(line):
                    offenders.append(
                        f"{path.relative_to(CRATES).as_posix()}:{number}: {line.strip()}"
                    )
        self.assertEqual(
            offenders,
            [],
            "the denominator law belongs to the owner:\n" + "\n".join(offenders),
        )

    def test_every_surface_binds_the_owner(self):
        missing: list[str] = []
        for crate in SURFACES:
            src = CRATES / crate / "src"
            if not src.is_dir():
                continue
            bound = any(
                OWNER_IMPORT.search(path.read_text(encoding="utf-8"))
                for path in src.rglob("*.rs")
            )
            if not bound:
                missing.append(crate)
        self.assertEqual(
            missing,
            [],
            "every chart surface must bind the shared slot grid:\n" + "\n".join(missing),
        )

    def test_a_local_copy_is_caught(self):
        """The rule must bite on the shape it forbids, not just on names."""
        grid = """
struct ChartSlots {
    sample_count: usize,
}

impl ChartSlots {
    fn denominator(&self) -> f32 {
        self.sample_count
            .saturating_sub(1)
            .max(self.capacity.saturating_sub(1))
            .max(1) as f32
    }
}
"""
        self.assertIsNotNone(LOCAL_GRID_TYPES.search(grid))
        self.assertIsNotNone(LOCAL_DENOMINATOR.search(grid))
        self.assertIsNotNone(DENOMINATOR_LAW.search(grid))


if __name__ == "__main__":
    unittest.main(verbosity=2)
