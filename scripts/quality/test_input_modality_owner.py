#!/usr/bin/env python3
"""One input modality vocabulary, one focus-visible rule (GPUI / Iced / Bevy).

No toolkit attaches an input origin to focus state, so every graphical
frontend synthesizes focus-visible from its own small tracker. Three trackers
carried three copies of the same three-state vocabulary and three copies of
the same rule ("only keyboard modality paints the ring"), each copy
self-described as mirroring the others — parity maintained by hand.

The owner is `taskmanager_ui_contract::InputModality` (toolkit-neutral
focus policy). This check fails the shapes that made the drift possible:

* a crate that declares its own `InputModality` enum;
* a surface that re-derives the focus-ring decision instead of delegating to
  the owner (a `shows_focus_ring` body that names the `Keyboard` variant);
* a surface that re-exports the owner type (a forwarding facade);
* a graphical surface that stops binding the owner.

Usage::

    python3 scripts/quality/test_input_modality_owner.py
"""

from __future__ import annotations

import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CRATES = ROOT / "crates"

# The owner: the input-origin vocabulary and the focus-visible rule.
OWNER_CRATE = "taskmanager-ui-contract"
OWNER = CRATES / OWNER_CRATE / "src/focus.rs"
OWNER_LIB = CRATES / OWNER_CRATE / "src/lib.rs"
OWNER_SYMBOLS = (
    "pub enum InputModality",
    "pub const fn shows_focus_ring(",
    "pub fn transition(",
)

# The graphical frontends that track an input modality.
SURFACES = (
    "taskmanager-gpui",
    "taskmanager-iced",
    "taskmanager-bevy-ui",
)

# A locally declared modality vocabulary: a second owner.
LOCAL_MODALITY_ENUM = re.compile(r"\benum\s+InputModality\b")

# The focus-visible decision spelled out in a surface: a second rule.
LOCAL_FOCUS_RING_DECISION = re.compile(
    r"fn\s+shows_focus_ring\b[^{]*\{[^{}]*\bKeyboard\b", re.DOTALL
)

# A forwarding facade for the owner type.
OWNER_REEXPORT = re.compile(r"pub\s+use\s+taskmanager_ui_contract::[^;]*InputModality")

OWNER_BINDING = re.compile(r"taskmanager_ui_contract::(?:\{[^}]*\b)?InputModality\b")


def crate_sources(crate: str) -> list[Path]:
    """Every production source file of one crate."""
    src = CRATES / crate / "src"
    if not src.is_dir():
        return []
    return sorted(src.rglob("*.rs"))


def all_non_owner_sources() -> list[Path]:
    """Every production source file outside the owner crate."""
    files: list[Path] = []
    for crate_dir in sorted(CRATES.iterdir()):
        if not crate_dir.is_dir() or crate_dir.name == OWNER_CRATE:
            continue
        files.extend(crate_sources(crate_dir.name))
    return files


class InputModalityOwnerTests(unittest.TestCase):
    def test_the_owner_publishes_one_vocabulary(self):
        self.assertTrue(OWNER.is_file(), OWNER)
        source = OWNER.read_text(encoding="utf-8")
        for symbol in OWNER_SYMBOLS:
            with self.subTest(symbol=symbol):
                self.assertIn(symbol, source)
        self.assertIn("InputModality", OWNER_LIB.read_text(encoding="utf-8"))

    def test_no_crate_declares_its_own_modality_enum(self):
        offenders: list[str] = []
        for path in all_non_owner_sources():
            source = path.read_text(encoding="utf-8")
            for number, line in enumerate(source.splitlines(), start=1):
                if LOCAL_MODALITY_ENUM.search(line):
                    offenders.append(
                        f"{path.relative_to(CRATES).as_posix()}:{number}: {line.strip()}"
                    )
        self.assertEqual(
            offenders,
            [],
            "the input modality vocabulary belongs to the owner:\n"
            + "\n".join(offenders),
        )

    def test_no_surface_re_derives_the_focus_ring_rule(self):
        offenders: list[str] = []
        for path in all_non_owner_sources():
            source = path.read_text(encoding="utf-8")
            if LOCAL_FOCUS_RING_DECISION.search(source):
                offenders.append(path.relative_to(CRATES).as_posix())
        self.assertEqual(
            offenders,
            [],
            "a surface may delegate the focus-ring decision, never re-derive it:\n"
            + "\n".join(offenders),
        )

    def test_no_surface_reexports_the_owner_type(self):
        offenders: list[str] = []
        for path in all_non_owner_sources():
            source = path.read_text(encoding="utf-8")
            if OWNER_REEXPORT.search(source):
                offenders.append(path.relative_to(CRATES).as_posix())
        self.assertEqual(
            offenders,
            [],
            "consumers import the owner module; no forwarding facade:\n"
            + "\n".join(offenders),
        )

    def test_every_graphical_surface_binds_the_owner(self):
        missing: list[str] = []
        for crate in SURFACES:
            sources = crate_sources(crate)
            if not sources:
                missing.append(crate)
                continue
            bound = any(
                OWNER_BINDING.search(path.read_text(encoding="utf-8"))
                for path in sources
            )
            if not bound:
                missing.append(crate)
        self.assertEqual(
            missing,
            [],
            "every graphical surface must bind the shared modality vocabulary:\n"
            + "\n".join(missing),
        )

    def test_a_local_copy_is_caught(self):
        """The rules must bite on the shapes they forbid, not just on names."""
        local_enum = """
pub(crate) enum InputModality {
    Programmatic,
    Keyboard,
    Pointer,
}
"""
        local_rule = """
impl InputModality {
    pub(crate) const fn shows_focus_ring(self) -> bool {
        matches!(self, Self::Keyboard)
    }
}
"""
        facade = "pub use taskmanager_ui_contract::InputModality;\n"

        self.assertIsNotNone(LOCAL_MODALITY_ENUM.search(local_enum))
        self.assertIsNotNone(LOCAL_FOCUS_RING_DECISION.search(local_rule))
        self.assertIsNotNone(OWNER_REEXPORT.search(facade))


if __name__ == "__main__":
    unittest.main(verbosity=2)
