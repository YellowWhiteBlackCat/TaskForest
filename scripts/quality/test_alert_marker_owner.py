#!/usr/bin/env python3
"""One address for the alert marker and the SMART readouts that wear it.

ADR-063 gives one authority to every visible resource: the icon registry owns
an alert's SEMANTIC identity (`IconId::Alert`), the catalogs own words, and the
TEXT form of the marker a surface that paints text wears was, at the last
committed baseline, hand-written in nineteen live sites across three frontends
(GPUI eight, Iced six, TUI five; Bevy carries none, its tofu law painting an
icon plate instead). Nineteen copies of one codepoint are nineteen chances to
drift, and the sites also re-composed the same SMART readouts around it — one
through a second percent helper, two spelling the English word "warning" inside
source instead of the catalog, and one frontend writing the English label
"Temperature" where the other three read the catalog.

Now one owner: `taskmanager-shell::presentation::constants::ALERT_MARKER` for
the spelling, and `taskmanager-shell::presentation::smart` for the three
compositions (`endurance_used_readout`, `spare_pool_readout`,
`smart_temperature_label`). This guard fails the shapes that grew back:

* no production file outside the owner writes the codepoint in CODE, in any
  of its spellings (raw, `\\u{26a0}`, or `\\u26a0`);
* the catalogs carry no copy of it — a marker inside a string table would be a
  second address for the same token;
* the owner holds exactly one spelling and exports it once;
* the three compositions are declared once, in the owner, so no frontend can
  re-implement one;
* the marker is NOT the icon registry's monochrome fallback (`IconId::Alert`
  falls back to a different codepoint on purpose); binding them would repaint
  every text surface to promote a fallback into the product's marker.

Usage::

    python3 scripts/quality/test_alert_marker_owner.py
"""

from __future__ import annotations

import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CRATES = ROOT / "crates"
LOCALES = ROOT / "locales"
OWNER = CRATES / "taskmanager-shell/src/presentation/constants.rs"
SMART = CRATES / "taskmanager-shell/src/presentation/smart.rs"
PRESENTATION = CRATES / "taskmanager-shell/src/presentation.rs"
HELP = CRATES / "taskmanager-shell/src/presentation/help.rs"

FRONTEND_CRATES = (
    "taskmanager-gpui",
    "taskmanager-iced",
    "taskmanager-tui",
    "taskmanager-bevy-ui",
)

# The codepoint in every spelling a Rust source file can carry it in, plus the
# raw character (an escape-free literal is the commonest way to write it twice).
MARKER_SPELLINGS = ("\\u{26a0}", "\\u26a0", "\u26a0")

# The compositions that were written per frontend and now live in one owner.
OWNED_COMPOSITIONS = (
    "endurance_used_readout",
    "spare_pool_readout",
    "smart_temperature_label",
)


def production_sources() -> list[Path]:
    """Every `src/` Rust file of every crate, owner included."""
    return sorted(
        path
        for path in CRATES.glob("*/src/**/*.rs")
        if "target" not in path.parts and path.is_file()
    )


def marker_sites(text: str) -> list[int]:
    """The 1-based CODE lines carrying the alert codepoint in any spelling.

    Comments are not scanned: a comment cannot paint a marker, and a guard
    that reddens on prose is a false-FAIL generator that gets ignored.
    """
    sites = []
    for number, line in enumerate(text.splitlines(), 1):
        stripped = line.strip()
        if stripped.startswith(("///", "//!", "//")):
            continue
        if any(spelling in line for spelling in MARKER_SPELLINGS):
            sites.append(number)
    return sites


def owner_spelling(text: str) -> str:
    """The one string literal `ALERT_MARKER` is defined as."""
    match = re.search(r'pub const ALERT_MARKER: &str = "([^"]*)";', text)
    if match is None:
        raise SystemExit("the alert marker owner no longer defines ALERT_MARKER")
    return match.group(1)


def icon_fallback_glyph(text: str) -> str:
    """The monochrome fallback the icon registry gives `IconId::Alert`."""
    table = text[text.index("pub const fn icon_glyph(") :]
    match = re.search(r"IconId::Alert\s*=>\s*\"([^\"]*)\"", table)
    if match is None:
        raise SystemExit("icon_glyph no longer answers for IconId::Alert")
    return match.group(1)


class TheMarkerHasOneAddress(unittest.TestCase):
    def test_only_the_owner_writes_the_codepoint(self):
        offenders = {}
        for path in production_sources():
            if path == OWNER:
                continue
            sites = marker_sites(path.read_text(encoding="utf-8"))
            if sites:
                offenders[str(path.relative_to(ROOT))] = sites
        self.assertEqual(
            offenders,
            {},
            "the alert marker has one owner; a frontend that writes the "
            "codepoint owns a spelling that can drift: "
            + ", ".join(f"{name}:{line}" for name, lines in offenders.items() for line in lines),
        )

    def test_no_catalog_holds_a_copy_of_the_marker(self):
        offenders = {}
        for path in sorted(LOCALES.glob("*.json")):
            sites = marker_sites(path.read_text(encoding="utf-8"))
            if sites:
                offenders[str(path.relative_to(ROOT))] = sites
        self.assertEqual(
            offenders,
            {},
            "the catalogs own words, not the marker; a glyph inside a string "
            "table is a second address for one token: "
            + ", ".join(f"{name}:{line}" for name, lines in offenders.items() for line in lines),
        )

    def test_the_owner_spells_it_exactly_once(self):
        text = OWNER.read_text(encoding="utf-8")
        declarations = re.findall(
            r"pub const ALERT_MARKER: &str = \"[^\"]*\";", text
        )
        self.assertEqual(
            len(declarations),
            1,
            f"ALERT_MARKER must be declared once, found {len(declarations)}",
        )
        self.assertEqual(
            len(marker_sites(text)),
            1,
            "the owner carries the codepoint on exactly one line: the "
            "declaration itself. A second line in the owner is a marker with "
            "two spellings inside one file.",
        )

    def test_the_owner_exports_the_marker(self):
        exports = re.findall(
            r"pub use constants::\{([^}]*)\}", PRESENTATION.read_text(encoding="utf-8")
        )
        self.assertEqual(
            len(exports),
            1,
            "presentation must re-export the shared constants from one place",
        )
        self.assertIn(
            "ALERT_MARKER",
            exports[0],
            "the marker is reachable as presentation::ALERT_MARKER or every "
            "frontend will reach for the literal again",
        )

    def test_the_marker_is_not_the_icon_fallback(self):
        # `icon_glyph` answers for a surface that may not carry a decoration
        # codepoint and therefore uses a monochrome fallback. The text marker
        # and that fallback are different facts; folding one into the other
        # would silently repaint every text surface.
        marker = owner_spelling(OWNER.read_text(encoding="utf-8"))
        fallback = icon_fallback_glyph(HELP.read_text(encoding="utf-8"))
        self.assertNotEqual(
            marker,
            fallback,
            "ALERT_MARKER must not be defined as icon_glyph(IconId::Alert): "
            f"both are {marker!r}, and the fallback is a different fact",
        )
        self.assertNotIn("icon_glyph", OWNER.read_text(encoding="utf-8"))


class TheCompositionsAreOwned(unittest.TestCase):
    def test_each_composition_is_declared_once_in_the_owner(self):
        owner_text = SMART.read_text(encoding="utf-8")
        for name in OWNED_COMPOSITIONS:
            declarations = [
                str(path.relative_to(ROOT))
                for path in production_sources()
                if f"pub fn {name}(" in path.read_text(encoding="utf-8")
            ]
            self.assertEqual(
                [str(SMART.relative_to(ROOT))],
                declarations,
                f"{name} is composed in {declarations}; a second declaration "
                "is a second authority for one readout",
            )
            self.assertGreaterEqual(
                owner_text.count(f"pub fn {name}("),
                1,
                f"{name} vanished from the owner",
            )

    def test_the_text_surfaces_bind_the_owner(self):
        # A frontend reaches a readout only by calling the owner, so a file
        # that names one must also import the owner module.
        for crate in FRONTEND_CRATES:
            for path in sorted(CRATES.glob(f"{crate}/src/**/*.rs")):
                text = path.read_text(encoding="utf-8")
                compact = "".join(text.split())
                for name in OWNED_COMPOSITIONS:
                    if not re.search(rf"\b{re.escape(name)}\(", text):
                        continue
                    self.assertIn(
                        "presentation::smart",
                        compact,
                        f"{path.relative_to(ROOT)} names {name} without "
                        "importing presentation::smart",
                    )

class TheGuardBites(unittest.TestCase):
    def test_a_frontend_literal_is_reported_in_every_spelling(self):
        for sample, expected in (
            ('let label = format!("{} \\u{26a0}", name);', [1]),
            ('format!("{} \\u26a0", name)', [1]),
            ('format!("{} \u26a0", name)', [1]),
            ('"Temperature \u26a0"', [1]),
            ('text("no marker here")', []),
        ):
            with self.subTest(sample=sample):
                self.assertEqual(expected, marker_sites(sample))

    def test_a_second_owner_spelling_is_reported(self):
        text = OWNER.read_text(encoding="utf-8")
        mutant = text.replace(
            'pub const ALERT_MARKER: &str = "\u26a0";',
            'pub const ALERT_MARKER: &str = "\u26a0";\n'
            'pub const ALERT_MARKER_BOLD: &str = "\u26a0";',
        )
        self.assertEqual(2, len(marker_sites(mutant)))
        self.assertEqual(1, len(marker_sites(text)))

    def test_a_readout_fork_is_reported_as_a_second_declaration(self):
        # The shape the census check hunts: a frontend growing its own copy.
        names = [
            str(path.relative_to(ROOT))
            for path in production_sources()
            if "pub fn spare_pool_readout(" in path.read_text(encoding="utf-8")
        ]
        self.assertEqual(
            [str(SMART.relative_to(ROOT))],
            names,
            "the owner list must be exactly one file, so a fork is a finding",
        )


if __name__ == "__main__":
    suite = unittest.TestLoader().loadTestsFromModule(__import__("__main__"))
    result = unittest.TextTestRunner(verbosity=1).run(suite)
    raise SystemExit(0 if result.wasSuccessful() else 1)
