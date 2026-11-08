#!/usr/bin/env python3
"""One typed vocabulary, one owner, one localized resolver (ADR-063).

`locales/*.json` and `taskmanager-application::i18n` own every piece of
product copy. A typed vocabulary (an alert metric, a service action, a
session verb, an NPU engine kind, a startup tier …) owns its catalog **key**;
the application i18n owner owns the one fold that turns that key into a
localized label.

A frontend that composes `t(<something>.i18n_key())` on its own is a second
resolver: it can drift from the owner, and it puts catalog knowledge outside
the copy authority. This check fails that shape.

The single exception is recorded below with its reason and its trigger
condition — an allowlist that is reviewed, not a blanket exemption.
"""

from __future__ import annotations

import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CRATES = ROOT / "crates"
OWNER = CRATES / "taskmanager-application/src/i18n.rs"

# Frontends (and the GPUI component layer) may not own or resolve a
# vocabulary key: they bind the owner's named resolver instead.
FRONTEND_CRATES = (
    "taskmanager-gpui",
    "taskmanager-iced",
    "taskmanager-tui",
    "taskmanager-bevy-ui",
    "taskmanager-ui",
)

# The owner must export one named resolver per product vocabulary.
EXPECTED_OWNER_RESOLVERS = (
    "pub fn alert_metric_label(",
    "pub fn alert_severity_label(",
    "pub fn service_action_label(",
    "pub fn session_control_action_label(",
    "pub fn npu_engine_kind_label(",
    "pub fn startup_impact_label(",
    "pub fn startup_scope_label(",
    "pub fn startup_control_policy_label(",
    "pub fn more_rows_label(",
    "pub fn process_details_section_label(",
)

# A frontend-local vocabulary: the enum, its catalog keys, and every call
# site live in one reviewed file. One consumer, one key map, no fork risk.
ALLOWED_FRONTEND_VOCABULARIES = (
    {
        "crate": "taskmanager-gpui",
        "path": "src/gpui_app/chrome.rs",
        "symbol": "DecorationOutcomeNotice",
        "reason": "GPUI-only verdict on a window-decoration preference the "
        "compositor refused. One consumer (the GPUI root toast), one key map, "
        "no other frontend renders this fact, so promoting it would create a "
        "second owner for a single-consumer vocabulary.",
        "trigger": "A second frontend renders a decoration refusal → move the "
        "vocabulary to the application i18n owner and drop this entry.",
    },
)

CALL_PATTERN = re.compile(r"\.i18n_key\s*\(\s*\)")
DEFINITION_PATTERN = re.compile(r"\bfn\s+i18n_key\s*\(")


def _frontend_sources() -> list[Path]:
    files: list[Path] = []
    for crate in FRONTEND_CRATES:
        src = CRATES / crate / "src"
        if not src.is_dir():
            continue
        files.extend(sorted(src.rglob("*.rs")))
    return files


def _relative(path: Path) -> str:
    return path.relative_to(CRATES).as_posix()


def _allowed(path: Path) -> dict | None:
    relative = _relative(path)
    for entry in ALLOWED_FRONTEND_VOCABULARIES:
        crate, _, rest = relative.partition("/")
        if crate == entry["crate"] and rest == entry["path"]:
            return entry
    return None


class I18nVocabularyOwnerTests(unittest.TestCase):
    def test_owner_exports_one_resolver_per_vocabulary(self):
        source = OWNER.read_text(encoding="utf-8")
        for resolver in EXPECTED_OWNER_RESOLVERS:
            with self.subTest(resolver=resolver):
                self.assertIn(resolver, source)

    def test_no_frontend_resolves_a_vocabulary_key(self):
        offenders: list[str] = []
        for path in _frontend_sources():
            source = path.read_text(encoding="utf-8")
            if not CALL_PATTERN.search(source) and not DEFINITION_PATTERN.search(source):
                continue
            if _allowed(path) is not None:
                continue
            for number, line in enumerate(source.splitlines(), start=1):
                if CALL_PATTERN.search(line) or DEFINITION_PATTERN.search(line):
                    offenders.append(f"{_relative(path)}:{number}: {line.strip()}")
        self.assertEqual(
            offenders,
            [],
            "frontends must call the i18n owner's named resolver instead of "
            "composing a localized string from a vocabulary key:\n"
            + "\n".join(offenders),
        )

    def test_the_allowlist_stays_a_single_reviewed_entry(self):
        self.assertEqual(len(ALLOWED_FRONTEND_VOCABULARIES), 1)
        for entry in ALLOWED_FRONTEND_VOCABULARIES:
            with self.subTest(entry=entry["crate"]):
                self.assertTrue(entry["reason"].strip())
                self.assertTrue(entry["trigger"].strip())
                path = CRATES / entry["crate"] / entry["path"]
                self.assertTrue(path.is_file(), path)

    def test_a_fork_mutation_fails(self):
        source = (CRATES / "taskmanager-iced/src/ui/containers.rs").read_text(
            encoding="utf-8"
        )
        forked = source.replace(
            "use taskmanager_application::i18n;",
            "use taskmanager_application::i18n;\n"
            "fn forked_label(metric: u32) -> &'static str { metric.i18n_key() }",
            1,
        )
        self.assertNotEqual(source, forked)
        self.assertIsNotNone(CALL_PATTERN.search(forked))


if __name__ == "__main__":
    unittest.main(verbosity=2)
