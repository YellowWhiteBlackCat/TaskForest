#!/usr/bin/env python3
"""One Performance quantity-family vocabulary, one owner, one gate.

`taskforest_core::core::units::QuantityFamily` owns the three Performance
quantity families (Memory / Drive / Network), the order in which a surface
offers them (`QuantityFamily::ALL`), and the six-slot editor dispatch
(`UnitPreferences::set_setting` with `QuantitySetting`). GPUI used to declare
its own `pub enum UnitFamily { Memory, Drive, Network }` and hand-copy the
`(use_bytes, use_base2)` match into `unit_choices`, `set_quantity_notation`
and `set_magnitude_base` — a second vocabulary and a second dispatch that
can drift from the core formatter.

This gate fails the shapes that made the drift possible:

* a frontend that declares its own quantity-family enum or type alias;
* a frontend that restates the three-family ladder;
* a frontend that hand-writes the six mutable bool slots;
* a renderer whose unit-preference editor no longer binds the core owner.

A toolkit binding that carries the core family as a payload (a settings
widget's private `Bytes/Bits` or `Binary/Decimal` choice) declares no family
of its own and is allowed.

Usage::

    python3 scripts/quality/test_quantity_family_owner.py
"""

from __future__ import annotations

import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CRATES = ROOT / "crates"
LOCAL_GATES = ROOT / "scripts/quality/local-gates.sh"

OWNER = CRATES / "taskforest-core/src/core/units.rs"
OWNER_SYMBOLS = (
    "pub enum QuantityFamily",
    "pub const ALL: [Self; 3]",
    "pub enum QuantitySetting",
    "pub const fn set_setting(",
)

FRONTEND_CRATES = (
    "taskmanager-gpui",
    "taskmanager-iced",
    "taskmanager-bevy-ui",
    "taskmanager-tui",
    "taskmanager-ui",
)

# A second family vocabulary: a type named like the owner, or a
# quantity-family enum that declares the three variants itself.
LOCAL_FAMILY_TYPE = re.compile(
    r"\b(?:enum|struct|type)\s+(?:QuantityFamily|UnitFamily|Quantity\w*Family|Unit\w*Family)\b"
)
LOCAL_FAMILY_VARIANTS = re.compile(
    r"\benum\s+\w+\b\s*(?::[^{;]*)?\{(?P<body>[^{}]*)\}", re.DOTALL
)
FAMILY_VARIANT = re.compile(r"(?<![:\w])(?:Memory|Drive|Network)\b")

# A frontend that restates the ladder instead of spending QuantityFamily::ALL.
LOCAL_FAMILY_LADDER = re.compile(
    r"\[\s*QuantityFamily::Memory\s*,\s*QuantityFamily::Drive\s*,\s*QuantityFamily::Network\s*\]"
)

# A frontend that writes the six preference slots itself instead of calling
# `set_setting`: a match arm binding a mutable field (never a read-only fold).
LOCAL_SETTING_MUTATION = re.compile(
    r"&mut\s+self\.\w*units\.(?:memory|drive|network)_use_(?:bytes|base2)"
)

OWNER_BINDINGS = ("QuantityFamily", "QuantitySetting", "set_setting")

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


class QuantityFamilyOwnerTests(unittest.TestCase):
    def test_the_owner_publishes_its_surface(self):
        self.assertTrue(OWNER.is_file(), OWNER)
        source = OWNER.read_text(encoding="utf-8")
        for symbol in OWNER_SYMBOLS:
            with self.subTest(symbol=symbol):
                self.assertIn(symbol, source)

    def test_the_quick_gate_runs_this_check(self):
        """An unwired owner test is a dead test, not evidence."""
        source = LOCAL_GATES.read_text(encoding="utf-8")
        self.assertIn(
            "scripts/quality/test_quantity_family_owner.py",
            source,
            "the quantity-family owner test must be wired into quick",
        )

    def test_no_frontend_declares_a_second_family_type(self):
        offenders: list[str] = []
        for path in frontend_sources():
            masked = code_only(path.read_text(encoding="utf-8"))
            for number, line in enumerate(masked.splitlines(), start=1):
                if LOCAL_FAMILY_TYPE.search(line):
                    offenders.append(
                        f"{path.relative_to(CRATES).as_posix()}:{number}: {line.strip()}"
                    )
        self.assertEqual(
            offenders,
            [],
            "the family vocabulary has one owner "
            "(taskforest_core::core::units::QuantityFamily):\n" + "\n".join(offenders),
        )

    def test_no_frontend_declares_the_three_family_variants(self):
        offenders: list[str] = []
        for path in frontend_sources():
            masked = code_only(path.read_text(encoding="utf-8"))
            for match in LOCAL_FAMILY_VARIANTS.finditer(masked):
                body = match.group("body")
                if len(FAMILY_VARIANT.findall(body)) == 3:
                    offenders.append(
                        f"{path.relative_to(CRATES).as_posix()}: {match.group(0)[:60]}"
                    )
        self.assertEqual(
            offenders,
            [],
            "no renderer declares its own Memory/Drive/Network ladder:\n"
            + "\n".join(offenders),
        )

    def test_no_frontend_restates_the_offered_ladder(self):
        offenders: list[str] = []
        for path in frontend_sources():
            masked = code_only(path.read_text(encoding="utf-8"))
            for number, line in enumerate(masked.splitlines(), start=1):
                if LOCAL_FAMILY_LADDER.search(line):
                    offenders.append(
                        f"{path.relative_to(CRATES).as_posix()}:{number}: {line.strip()}"
                    )
        self.assertEqual(
            offenders,
            [],
            "the offered set and order come from QuantityFamily::ALL:\n"
            + "\n".join(offenders),
        )

    def test_no_frontend_hand_writes_the_six_mutable_slots(self):
        offenders: list[str] = []
        for path in frontend_sources():
            masked = code_only(path.read_text(encoding="utf-8"))
            for number, line in enumerate(masked.splitlines(), start=1):
                if LOCAL_SETTING_MUTATION.search(line):
                    offenders.append(
                        f"{path.relative_to(CRATES).as_posix()}:{number}: {line.strip()}"
                    )
        self.assertEqual(
            offenders,
            [],
            "unit-preference edits spend UnitPreferences::set_setting:\n"
            + "\n".join(offenders),
        )

    def test_the_gpuI_editor_binds_the_owner(self):
        """An owner nobody spends is a dead abstraction, not a contract."""
        source = (
            CRATES / "taskmanager-gpui/src/gpui_app/root/presentation_preferences.rs"
        ).read_text(encoding="utf-8")
        for symbol in OWNER_BINDINGS:
            with self.subTest(symbol=symbol):
                self.assertIn(symbol, source)

    # ── The detector must bite a fork and spare a binding ──────────────────

    def test_the_detector_bites_a_forked_family_type(self):
        self.assertTrue(
            LOCAL_FAMILY_TYPE.search("pub enum UnitFamily { Memory, Drive, Network }")
        )

    def test_the_detector_leaves_toolkit_bindings_alone(self):
        for shape in (
            "enum QuantityNotation { Bytes, Bits }",
            "enum MagnitudeBase { Binary, Decimal }",
            "let (use_bytes, use_base2) = self.units.settings(family);",
            "self.units.set_setting(family, QuantitySetting::UseBytes, true)",
        ):
            with self.subTest(shape=shape):
                self.assertFalse(LOCAL_FAMILY_TYPE.search(shape))
                self.assertFalse(LOCAL_SETTING_MUTATION.search(shape))


if __name__ == "__main__":
    unittest.main(verbosity=2)
