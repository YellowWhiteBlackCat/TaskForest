#!/usr/bin/env python3
"""One bounded card-row width fold, one owner, one gate.

`taskmanager_ui_contract::presentation::card_width` owns the WHOLE-column
verdict, the measured card/content caps, the bounded lower content width
and the residual lane space. It also decides whether the group header's
label/overflow fact group fits inside that bounded group before any card
is spent. Bevy used to declare its own `GroupWidthAdmission`, its own
`card_columns` ladder and its own `admit_group_width` arithmetic — and
silently discarded the `has_overflow` fact with `let _ = has_overflow`.

This gate refuses a frontend that forks the verdict type, redefines the
admission function, or re-derives the width arithmetic. It also proves the
owner is actually bound by the shape that paints atomic cards.

Usage::

    python3 scripts/quality/test_card_width_owner.py
"""

from __future__ import annotations

import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CRATES = ROOT / "crates"
LOCAL_GATES = ROOT / "scripts/quality/local-gates.sh"

OWNER = CRATES / "taskmanager-ui-contract/src/presentation/card_width.rs"
OWNER_SYMBOLS = (
    "pub struct CardRowWidthBudget",
    "pub struct CardRowWidthSpec",
    "pub enum CardRowWidthAdmission",
    "pub const fn from_measured_card_cap(",
    "pub fn row_width_cap(",
    "pub fn admit_card_row_width(",
    "pub fn card_row_columns(",
    "pub fn admit_card_row_width_budget(",
    "pub const fn header_width(",
    "pub const fn row_width(",
)

FRONTEND_CRATES = (
    "taskmanager-gpui",
    "taskmanager-iced",
    "taskmanager-bevy-ui",
    "taskmanager-tui",
    "taskmanager-ui",
)

LOCAL_VERDICT = re.compile(
    r"\b(?:enum|struct|type)\s+"
    r"(?:GroupWidthAdmission|CardRowWidthAdmission|\w*CardWidth\w*Admission)\b"
)
LOCAL_ADMISSION = re.compile(
    r"\bfn\s+(?:admit_group_width|group_width_admission|card_row_columns|"
    r"admit_card_row_width)\s*\("
)
LOCAL_COLUMN_REQUEST = re.compile(r"\bfn\s+card_columns\s*\(")
LOCAL_ROW_ARITHMETIC = re.compile(
    r"card_min_width_px\s*\*\s*\w+\s+[\s\S]{0,120}?group_gap_px\s*\*"
)
LOCAL_WIDTH_CAP = re.compile(
    r"\b(?:card|content)_(?:max|cap)(?:_width)?(?:_px|_rem)?\b\s*(?::|=)\s*\d"
    r"|\bfn\s+(?:card_row_width|content_width_cap|card_max_width)\w*\s*\("
)
DISCARDED_OVERFLOW = re.compile(r"\blet\s+_\s*=\s*has_overflow\s*;")

BINDINGS = (
    (
        CRATES / "taskmanager-bevy-ui/src/pages/processes/card_lane.rs",
        (
            "CardRowWidthSpec",
            "CardRowWidthBudget",
            "from_measured_card_cap",
            "admit_card_row_width_budget",
            "card_row_columns",
            "has_overflow",
            "card_max_width_px",
            "content_width_cap",
        ),
    ),
    # A PLAIN grid (the dashboard metric row and the GPU-insight row) binds
    # the shared grid shape. A page that re-derived its own share formula
    # would own a second address for the same width arithmetic.
    (
        CRATES / "taskmanager-gpui/src/gpui_app/root/responsive/dashboard.rs",
        ("CardGridWidthSpec", "admit_card_grid_width"),
    ),
    (
        CRATES / "taskmanager-gpui/src/gpui_app/process_insights/view/facets.rs",
        ("CardGridWidthSpec", "admit_card_grid_width"),
    ),
)

BLOCK_COMMENT = re.compile(r"/\*.*?\*/", re.DOTALL)
LINE_COMMENT = re.compile(r"//[^\n]*")
STRING_LITERAL = re.compile(r'"((?:[^"\\]|\\.)*)"')


def code_only(source: str) -> str:
    """Comments and string literals removed, line and column offsets kept."""
    text = BLOCK_COMMENT.sub(lambda m: re.sub(r"[^\n]", " ", m.group(0)), source)
    text = LINE_COMMENT.sub(lambda m: " " * len(m.group(0)), text)
    return STRING_LITERAL.sub(lambda m: re.sub(r"[^\n]", " ", m.group(0)), text)


def frontend_sources() -> list[Path]:
    files: list[Path] = []
    for crate in FRONTEND_CRATES:
        src = CRATES / crate / "src"
        if src.is_dir():
            files.extend(sorted(src.rglob("*.rs")))
    return files


class CardWidthOwnerTests(unittest.TestCase):
    def test_the_owner_publishes_its_surface(self):
        self.assertTrue(OWNER.is_file(), OWNER)
        source = OWNER.read_text(encoding="utf-8")
        for symbol in OWNER_SYMBOLS:
            with self.subTest(symbol=symbol):
                self.assertIn(symbol, source)

    def test_the_quick_gate_runs_this_check(self):
        source = LOCAL_GATES.read_text(encoding="utf-8")
        self.assertIn(
            "scripts/quality/test_card_width_owner.py",
            source,
            "the card-width owner test must be wired into quick",
        )

    def test_no_frontend_forks_the_width_verdict(self):
        offenders: list[str] = []
        for path in frontend_sources():
            masked = code_only(path.read_text(encoding="utf-8"))
            for number, line in enumerate(masked.splitlines(), start=1):
                if LOCAL_VERDICT.search(line):
                    offenders.append(
                        f"{path.relative_to(CRATES).as_posix()}:{number}: {line.strip()}"
                    )
        self.assertEqual(
            offenders,
            [],
            "the card-row width verdict has one owner:\n" + "\n".join(offenders),
        )

    def test_no_frontend_reimplements_the_admission_or_column_request(self):
        offenders: list[str] = []
        for path in frontend_sources():
            masked = code_only(path.read_text(encoding="utf-8"))
            for number, line in enumerate(masked.splitlines(), start=1):
                if LOCAL_ADMISSION.search(line) or LOCAL_COLUMN_REQUEST.search(line):
                    offenders.append(
                        f"{path.relative_to(CRATES).as_posix()}:{number}: {line.strip()}"
                    )
        self.assertEqual(
            offenders,
            [],
            "no renderer declares a second width-admission function:\n"
            + "\n".join(offenders),
        )

    def test_no_frontend_rederives_the_row_width_or_discards_overflow(self):
        offenders: list[str] = []
        for path in frontend_sources():
            masked = code_only(path.read_text(encoding="utf-8"))
            if LOCAL_ROW_ARITHMETIC.search(masked):
                offenders.append(
                    f"{path.relative_to(CRATES).as_posix()}: re-derives the card row width"
                )
            if DISCARDED_OVERFLOW.search(masked):
                offenders.append(
                    f"{path.relative_to(CRATES).as_posix()}: discards the header overflow fact"
                )
        self.assertEqual(
            offenders,
            [],
            "the row-width arithmetic and the overflow fact stay with the owner:\n"
            + "\n".join(offenders),
        )

    def test_the_card_lane_binds_the_owner(self):
        for path, symbols in BINDINGS:
            source = path.read_text(encoding="utf-8")
            for symbol in symbols:
                with self.subTest(path=path.name, symbol=symbol):
                    self.assertIn(symbol, source)

    def test_the_detector_bites_a_fork_and_spares_a_binding(self):
        self.assertTrue(
            LOCAL_VERDICT.search("enum GroupWidthAdmission { SingleColumn }")
        )
        self.assertTrue(
            LOCAL_ADMISSION.search("fn admit_group_width(slot: f32) -> usize {")
        )
        self.assertFalse(
            LOCAL_VERDICT.search("use taskmanager_ui_contract::presentation::card_width::CardRowWidthAdmission;")
        )

    def test_no_frontend_forks_the_bounded_width_cap(self):
        offenders: list[str] = []
        for path in frontend_sources():
            masked = code_only(path.read_text(encoding="utf-8"))
            for number, line in enumerate(masked.splitlines(), start=1):
                if LOCAL_WIDTH_CAP.search(line):
                    offenders.append(
                        f"{path.relative_to(CRATES).as_posix()}:{number}: {line.strip()}"
                    )
        self.assertEqual(
            offenders,
            [],
            "measured card/content caps live in the shared width fold:\n"
            + "\n".join(offenders),
        )

    def test_the_bevy_scene_spends_the_shared_row_width(self):
        source = (
            CRATES / "taskmanager-bevy-ui/src/pages/processes/card_lane/scenes.rs"
        ).read_text(encoding="utf-8")
        for symbol in (
            "CardRowWidthMeasurements",
            "row_width.content_width_rem",
            "row_width.card_width_rem",
            "max_width: percent(100)",
        ):
            with self.subTest(symbol=symbol):
                self.assertIn(symbol, source)
        atomic = code_only(source.split("pub(crate) fn atomic_card_scene", 1)[1])
        for symbol in (
            "flex_grow: 0.0",
            "flex_shrink: 0.0",
            "flex_basis: card_basis",
            "max_width: rem(row_width.card_width_rem)",
            "let card_basis = if compact_focus",
            "px(card_height_px)",
            "rem(row_width.card_width_rem)",
        ):
            with self.subTest(symbol=symbol):
                self.assertIn(
                    symbol,
                    atomic,
                    "an atomic card must not grow past the shared bounded card width",
                )

    def test_bevy_declares_only_the_measured_constants_the_owner_requires(self):
        source = (
            CRATES / "taskmanager-bevy-ui/src/pages/processes/card_lane.rs"
        ).read_text(encoding="utf-8")
        self.assertIn("let card_max_width_px = control_height_px * 12.0;", source)
        self.assertIn("max_columns: 4", source)
        self.assertNotIn("content_width_cap:", source)


if __name__ == "__main__":
    unittest.main(verbosity=2)
