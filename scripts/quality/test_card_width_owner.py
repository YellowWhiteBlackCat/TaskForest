#!/usr/bin/env python3
"""One card-row width admission, one owner, one gate.

`taskmanager_ui_contract::presentation::card_width` owns the WHOLE-column
verdict: how many complete cards fit a measured slot and whether the group
header's label/overflow fact group fits before any card is spent. Bevy used
to declare its own `GroupWidthAdmission`, its own `card_columns` ladder and
its own `admit_group_width` arithmetic — and silently discarded the
`has_overflow` fact with `let _ = has_overflow`.

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
    "pub struct CardRowWidthSpec",
    "pub enum CardRowWidthAdmission",
    "pub fn admit_card_row_width(",
    "pub fn card_row_columns(",
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
DISCARDED_OVERFLOW = re.compile(r"\blet\s+_\s*=\s*has_overflow\s*;")

BINDINGS = (
    (
        CRATES / "taskmanager-bevy-ui/src/pages/processes/card_lane.rs",
        (
            "CardRowWidthSpec",
            "admit_card_row_width",
            "card_row_columns",
            "has_overflow",
        ),
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


if __name__ == "__main__":
    unittest.main(verbosity=2)
