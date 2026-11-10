#!/usr/bin/env python3
"""One device-visibility category set, one owner, one gate.

`taskforest_core::core::config::DeviceVisibilityKey` owns the ten
Performance device categories (CPU / Memory / Disks / Network + its five
sub-categories / GPUs), their Settings row order (`ALL`), each category's
locale key (`i18n_key`), each toggle's stable widget id (`stable_id`), and
the ten switches themselves (`DeviceVisibilityPreferences`, with the
`Config` fold).

GPUI declared a private `pub enum DevicePreference` with the same ten
variants plus its own ten-way visibility match, and Iced declared a
`pub enum DeviceKind` with the same ten variants plus another ten-way
match. Two frontends spelled one product set twice, so the set could drift
from the config schema, from the catalog and from the other shape.

This gate fails the shapes that made the drift possible:

* a frontend that declares its own category enum or type alias;
* a frontend that restates the ten variants or the ten-way ladder;
* a frontend that spells a category's locale key or widget id itself;
* a frontend that hand-writes the ten persisted `show_*` fields.

Usage::

    python3 scripts/quality/test_device_visibility_owner.py
"""

from __future__ import annotations

import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CRATES = ROOT / "crates"
LOCAL_GATES = ROOT / "scripts/quality/local-gates.sh"

OWNER = CRATES / "taskforest-core/src/core/config/device_visibility.rs"
OWNER_SYMBOLS = (
    "pub enum DeviceVisibilityKey",
    "pub const ALL: [Self; 10]",
    "pub const fn i18n_key(",
    "pub const fn stable_id(",
    "pub struct DeviceVisibilityPreferences",
    "pub const fn from_config(",
    "pub const fn apply_to(",
    "pub const fn set_visible(",
)

FRONTEND_CRATES = (
    "taskmanager-gpui",
    "taskmanager-iced",
    "taskmanager-bevy-ui",
    "taskmanager-tui",
    "taskmanager-ui",
)

# A second category vocabulary.
LOCAL_CATEGORY_TYPE = re.compile(
    r"\b(?:enum|struct|type)\s+"
    r"(?:DevicePreference|DeviceKind|DeviceVisibilityKey|"
    r"DeviceVisibility\w*Toggle|Device\w*Visibility\w*Key)\b"
)

# The ten variants spelled out in one enum body.
LOCAL_CATEGORY_VARIANTS = re.compile(
    r"\benum\s+\w+\b\s*(?::[^{;]*)?\{(?P<body>[^{}]*)\}", re.DOTALL
)
CATEGORY_VARIANTS = (
    "Cpu",
    "Memory",
    "Disks",
    "Network",
    "NetworkWired",
    "NetworkWireless",
    "NetworkVpn",
    "NetworkVirtual",
    "NetworkOther",
    "Gpus",
)

# A hand-written ten-way ladder over the category set.
LOCAL_CATEGORY_LADDER = re.compile(
    r"(?:DevicePreference|DeviceKind|DeviceVisibilityKey)::Cpu\b[\s\S]{0,400}?::Gpus\b"
)

# A frontend that spells a category's locale key or widget id itself.
LOCAL_CATEGORY_KEY = re.compile(
    r'"(?:settings\.show_(?:cpu|memory|disks|network|gpus)'
    r'|settings\.show_network_(?:wired|wireless|vpn|virtual|other)'
    r'|device-(?:cpu|memory|disks|network|gpus)'
    r'|network-(?:wired|wireless|vpn|virtual|other))"',
    re.IGNORECASE,
)

# A frontend that writes the ten persisted config fields itself.
LOCAL_CONFIG_FIELD = re.compile(
    r"\b(?:config|config_draft|cfg)\s*\.\s*show_(?:cpu|memory|disks|network|gpus)\b"
    r"|\bshow_network_(?:wired|wireless|vpn|virtual|other)\s*="
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


class DeviceVisibilityOwnerTests(unittest.TestCase):
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
            "scripts/quality/test_device_visibility_owner.py",
            source,
            "the device-visibility owner test must be wired into quick",
        )

    def test_no_frontend_declares_a_second_category_type(self):
        offenders: list[str] = []
        for path in frontend_sources():
            masked = code_only(path.read_text(encoding="utf-8"))
            for number, line in enumerate(masked.splitlines(), start=1):
                if LOCAL_CATEGORY_TYPE.search(line):
                    offenders.append(
                        f"{path.relative_to(CRATES).as_posix()}:{number}: {line.strip()}"
                    )
        self.assertEqual(
            offenders,
            [],
            "the category set has one owner "
            "(taskforest_core::core::config::DeviceVisibilityKey):\n"
            + "\n".join(offenders),
        )

    def test_no_frontend_declares_the_ten_variants(self):
        offenders: list[str] = []
        for path in frontend_sources():
            masked = code_only(path.read_text(encoding="utf-8"))
            for match in LOCAL_CATEGORY_VARIANTS.finditer(masked):
                body = match.group("body")
                hits = sum(1 for variant in CATEGORY_VARIANTS if variant in body)
                if hits >= 5:
                    offenders.append(
                        f"{path.relative_to(CRATES).as_posix()}: {match.group(0)[:60]}"
                    )
        self.assertEqual(
            offenders,
            [],
            "no renderer declares its own device-category ladder:\n"
            + "\n".join(offenders),
        )

    def test_no_frontend_restates_the_ten_way_ladder(self):
        offenders: list[str] = []
        for path in frontend_sources():
            masked = code_only(path.read_text(encoding="utf-8"))
            for number, line in enumerate(masked.splitlines(), start=1):
                if LOCAL_CATEGORY_LADDER.search(line):
                    offenders.append(
                        f"{path.relative_to(CRATES).as_posix()}:{number}: {line.strip()}"
                    )
        self.assertEqual(
            offenders,
            [],
            "the category order comes from DeviceVisibilityKey::ALL:\n"
            + "\n".join(offenders),
        )

    def test_no_frontend_spells_a_category_key_or_widget_id(self):
        offenders: list[str] = []
        for path in frontend_sources():
            masked = BLOCK_COMMENT.sub(
                lambda m: re.sub(r"[^\n]", " ", m.group(0)),
                path.read_text(encoding="utf-8"),
            )
            for number, line in enumerate(masked.splitlines(), start=1):
                if LOCAL_CATEGORY_KEY.search(line):
                    offenders.append(
                        f"{path.relative_to(CRATES).as_posix()}:{number}: {line.strip()}"
                    )
        self.assertEqual(
            offenders,
            [],
            "category wording and widget ids come from the owner:\n"
            + "\n".join(offenders),
        )

    def test_no_frontend_hand_writes_the_persisted_fields(self):
        offenders: list[str] = []
        for path in frontend_sources():
            masked = code_only(path.read_text(encoding="utf-8"))
            for number, line in enumerate(masked.splitlines(), start=1):
                if LOCAL_CONFIG_FIELD.search(line):
                    offenders.append(
                        f"{path.relative_to(CRATES).as_posix()}:{number}: {line.strip()}"
                    )
        self.assertEqual(
            offenders,
            [],
            "the persisted switches are written by the owner:\n" + "\n".join(offenders),
        )

    # ── The detector must bite a fork and spare a binding ──────────────────

    def test_the_detector_bites_a_forked_category_enum(self):
        self.assertTrue(
            LOCAL_CATEGORY_TYPE.search("pub enum DevicePreference { Cpu, Memory }")
        )
        self.assertTrue(LOCAL_CATEGORY_TYPE.search("pub enum DeviceKind { Cpu }"))

    def test_the_detector_leaves_owner_spending_alone(self):
        for shape in (
            "let visible = prefs.devices.visible(DeviceVisibilityKey::Cpu);",
            "t(kind.i18n_key())",
            "target(kind.stable_id())",
            "devices.apply_to(&mut config);",
        ):
            with self.subTest(shape=shape):
                self.assertFalse(LOCAL_CATEGORY_TYPE.search(shape))
                self.assertFalse(LOCAL_CATEGORY_LADDER.search(shape))


if __name__ == "__main__":
    unittest.main(verbosity=2)
