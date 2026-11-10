#!/usr/bin/env python3
"""One address for a per-shape layout band: the registry, not the vocabulary.

`taskmanager-ui-contract` carries two layout-band modules with different
contracts. `presentation::vocabulary` owns the bands **every** shape spends
("identical by construction, so they are stated exactly once here").
`presentation::thresholds` owns the per-shape bands: it is the census where a
shape that spends a different number records `Own`, and a shape that spends
none records `Absent` -- with the reason that names the mechanism.

A band whose *value* is shared but whose *spending* is not (the GPUI and Iced
windows clamp to a 720x480 floor; the terminal owns its viewport; the Bevy
window is whatever the compositor gives it) therefore has exactly one lawful
address: the threshold registry, whose `Optional`/`Absent` machinery is the
only place that can say who spends it. R136 found the mirror in `vocabulary`
(`MIN_WINDOW_WIDTH` / `MIN_WINDOW_HEIGHT`) while the registry's own copy was
dead -- nothing read it -- and the Bevy declaration claimed `Shared` for a
floor Bevy never sets. This guard fails the mirror shape, so the fact cannot
grow a second address again:

* every registry axis whose spending differs across shapes (`is_optional`)
  must not be restated by a `pub const` in `vocabulary.rs`;
* the mirror must be found whatever the constant is named, so the check is on
  the *value* (an axis baseline appearing as a vocabulary band), not on a name.

Deliberately not flagged: a value that coincides with a *required* axis
baseline (every shape spends it, so vocabulary is its lawful home), or a
value that appears in a non-band context (a test, a comment, a capture size).

Usage::

    python3 scripts/quality/test_threshold_band_mirror.py
"""

from __future__ import annotations

import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
VOCABULARY = ROOT / "crates/taskmanager-ui-contract/src/presentation/vocabulary.rs"
THRESHOLDS = ROOT / "crates/taskmanager-ui-contract/src/presentation/thresholds.rs"

# `pub const NAME: f32 = value;` -- the only band shape a module can own.
BAND_CONST = re.compile(r"pub\s+const\s+(\w+)\s*:\s*f32\s*=\s*([0-9.]+)\s*;")
# `LayoutAxis::Name => Some(value),` -- the registry's shared baselines.
AXIS_BASELINE = re.compile(r"Self::(\w+)\s*=>\s*Some\(([0-9.]+)\)")
# `Self::Name` inside `is_optional()` -- axes some shapes do not spend.
OPTIONAL_AXIS = re.compile(r"Self::(\w+)")


def axis_baselines(text: str) -> dict[str, float]:
    """`LayoutAxis` name -> its shared baseline, from the registry's own table."""
    return {name: float(value) for name, value in AXIS_BASELINE.findall(text)}


def optional_axes(text: str) -> set[str]:
    """The axes some shape does not spend: the registry's `is_optional` set."""
    is_optional = text[text.index("fn is_optional") : text.index("pub enum AxisDecision")]
    return set(OPTIONAL_AXIS.findall(is_optional))


def vocabulary_bands(text: str) -> dict[str, float]:
    """`vocabulary` constant -> value."""
    return {name: float(value) for name, value in BAND_CONST.findall(text)}


def mirror_findings(vocabulary_text: str, thresholds_text: str) -> list[str]:
    """Per-shape bands that `vocabulary` restates, as `name (axis = value)`."""
    optional = optional_axes(thresholds_text)
    per_shape = {
        axis: value
        for axis, value in axis_baselines(thresholds_text).items()
        if axis in optional
    }
    findings = []
    for name, value in vocabulary_bands(vocabulary_text).items():
        for axis, axis_value in per_shape.items():
            if value == axis_value:
                findings.append(f"{name} ({axis} = {axis_value:g})")
    return findings


class TheRegistryOwnsPerShapeBands(unittest.TestCase):
    def test_vocabulary_declares_no_per_shape_band(self):
        findings = mirror_findings(
            VOCABULARY.read_text(encoding="utf-8"), THRESHOLDS.read_text(encoding="utf-8")
        )
        self.assertEqual(
            findings,
            [],
            "vocabulary owns the bands every shape spends; a per-shape band "
            "(an optional registry axis) has one address -- the registry -- and a "
            "vocabulary copy is a second authority: " + ", ".join(findings),
        )

    def test_the_window_floor_is_a_registry_axis_not_a_vocabulary_band(self):
        text = VOCABULARY.read_text(encoding="utf-8")
        self.assertNotIn("MIN_WINDOW_WIDTH", text)
        self.assertNotIn("MIN_WINDOW_HEIGHT", text)
        thresholds = THRESHOLDS.read_text(encoding="utf-8")
        baselines = axis_baselines(thresholds)
        self.assertEqual(baselines.get("MinimumWindowWidth"), 720.0)
        self.assertEqual(baselines.get("MinimumWindowHeight"), 480.0)
        self.assertIn("MinimumWindowWidth", optional_axes(thresholds))
        self.assertIn("MinimumWindowHeight", optional_axes(thresholds))


class TheGuardBites(unittest.TestCase):
    def test_a_restated_optional_baseline_is_reported_whatever_it_is_named(self):
        vocabulary = VOCABULARY.read_text(encoding="utf-8")
        thresholds = THRESHOLDS.read_text(encoding="utf-8")
        for name in ("MIN_WINDOW_WIDTH", "WINDOW_FLOOR", "SMALLEST_WINDOW"):
            sample = vocabulary.replace(
                "pub const MAX_WINDOW_WIDTH: f32 = 3840.0;",
                f"pub const {name}: f32 = 720.0;\npub const MAX_WINDOW_WIDTH: f32 = 3840.0;",
            )
            with self.subTest(name=name):
                self.assertIn(
                    f"{name} (MinimumWindowWidth = 720)",
                    mirror_findings(sample, thresholds),
                )

    def test_a_required_axis_baseline_is_not_a_mirror(self):
        # `PerformanceStatsMinWidth` (236.0) is spent by every shape that has a
        # stats rail, so it is NOT in the optional set: a shared band with the
        # same value would be a different fact, not a mirror.
        thresholds = THRESHOLDS.read_text(encoding="utf-8")
        sample = VOCABULARY.read_text(encoding="utf-8").replace(
            "pub const MAX_WINDOW_WIDTH: f32 = 3840.0;",
            "pub const STAT_FLOOR: f32 = 236.0;\npub const MAX_WINDOW_WIDTH: f32 = 3840.0;",
        )
        self.assertEqual(mirror_findings(sample, thresholds), [])


if __name__ == "__main__":
    suite = unittest.TestLoader().loadTestsFromModule(__import__("__main__"))
    result = unittest.TextTestRunner(verbosity=1).run(suite)
    raise SystemExit(0 if result.wasSuccessful() else 1)
