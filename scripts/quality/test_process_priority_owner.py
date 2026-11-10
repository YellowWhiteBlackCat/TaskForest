#!/usr/bin/env python3
"""One process-priority vocabulary, one wording fold, one gate.

`taskforest_core::core::process::PriorityTier` owns the priority fact: the
offered set and its order (`PriorityTier::ALL`), the locale key that names a
tier (`PriorityTier::i18n_key`) and the batch action that applies it
(`ProcessBatchAction::SetPriority`). `taskmanager_shell::presentation` owns
the wording folds every frontend spends — `priority_tier_label` for the tier
word and `priority_tier_tooltip_key` for the action tooltip key.

A frontend that wraps that fact in its own `enum PriorityChoice { High,
Normal, Low }` creates a second semantic vocabulary: it can drift from the
core set, from the catalog and from the other shapes. A frontend that spells
the tier's own locale key creates a second wording fold.

This gate fails the shapes that made the drift possible:

* a frontend that declares its own priority-choice variants;
* a frontend that forks the core tier type's name;
* a frontend that spells a tier's locale key or tooltip key itself;
* a frontend that restates the offered tier ladder instead of spending
  `PriorityTier::ALL`.

Toolkit bindings that CARRY the core tier as a payload (e.g.
`ProcessToolbarAction::SetPriority(PriorityTier)`, or an Iced pick-list
newtype over `PriorityTier`) are bindings, not second vocabularies: they
declare no tier of their own and are allowed.

Usage::

    python3 scripts/quality/test_process_priority_owner.py
"""

from __future__ import annotations

import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CRATES = ROOT / "crates"
LOCAL_GATES = ROOT / "scripts/quality/local-gates.sh"

# ── The owners ──────────────────────────────────────────────────────────────

# The core fact: the offered set, its order, the locale key, the action.
CORE_TIER = CRATES / "taskforest-core/src/core/process/control.rs"
CORE_OWNER_SYMBOLS = (
    "pub enum PriorityTier",
    "pub const ALL: [Self; 3]",
    "pub const fn i18n_key(",
    "SetPriority(PriorityTier)",
)

# The wording folds every frontend spends.
SHELL_PRESENTATION = CRATES / "taskmanager-shell/src/presentation.rs"
SHELL_OWNER_SYMBOLS = (
    "pub fn priority_tier_label(",
    "pub const fn priority_tier_tooltip_key(",
)

FRONTEND_CRATES = (
    "taskmanager-gpui",
    "taskmanager-iced",
    "taskmanager-bevy-ui",
    "taskmanager-tui",
)

# ── The forks this gate refuses ─────────────────────────────────────────────

# A local priority vocabulary: a priority-named enum that declares tier
# variants of its own. A toolkit binding that only CARRIES `PriorityTier`
# declares no variant and does not match.
LOCAL_PRIORITY_VOCAB = re.compile(
    r"\benum\s+(?P<name>\w*[Pp]riority\w*)\b\s*(?::[^{;]*)?\{(?P<body>[^{}]*)\}",
    re.DOTALL,
)
LOCAL_TIER_VARIANT = re.compile(r"(?<![:\w])(?:High|Normal|Low)\b")

# A frontend that renames or redefines the core tier type itself.
LOCAL_TIER_TYPE = re.compile(r"\b(?:enum|struct|type)\s+PriorityTier\b")

# A frontend that spells a tier's locale key or its tooltip key itself.
LOCAL_TIER_KEY = re.compile(
    r'"(?:proc\.(?:high|normal|low)|tooltip\.proc_(?:high|normal|low))"',
    re.IGNORECASE,
)

# A frontend that restates the offered ladder instead of spending the owner.
LOCAL_TIER_LADDER = re.compile(
    r"\[\s*PriorityTier::High\s*,\s*PriorityTier::Normal\s*,\s*PriorityTier::Low\s*\]"
)

BLOCK_COMMENT = re.compile(r"/\*.*?\*/", re.DOTALL)
LINE_COMMENT = re.compile(r"//[^\n]*")
STRING_LITERAL = re.compile(r'"((?:[^"\\]|\\.)*)"')


def mask_comments(text: str) -> str:
    """Replace Rust comments with spaces, preserving offsets and lines."""
    text = BLOCK_COMMENT.sub(lambda m: re.sub(r"[^\n]", " ", m.group(0)), text)
    return LINE_COMMENT.sub(lambda m: " " * len(m.group(0)), text)


def mask_strings(text: str) -> str:
    """Replace Rust string literals with spaces, preserving offsets and lines."""
    return STRING_LITERAL.sub(lambda m: re.sub(r"[^\n]", " ", m.group(0)), text)


def code_only(source: str) -> str:
    """Comments and string literals removed, line and column offsets kept."""
    return mask_strings(mask_comments(source))


def frontend_sources() -> list[Path]:
    files: list[Path] = []
    for crate in FRONTEND_CRATES:
        src = CRATES / crate / "src"
        if src.is_dir():
            files.extend(sorted(src.rglob("*.rs")))
    return files


class ProcessPriorityOwnerTests(unittest.TestCase):
    def test_the_core_owner_publishes_the_fact(self):
        self.assertTrue(CORE_TIER.is_file(), CORE_TIER)
        source = CORE_TIER.read_text(encoding="utf-8")
        for symbol in CORE_OWNER_SYMBOLS:
            with self.subTest(symbol=symbol):
                self.assertIn(symbol, source)

    def test_the_shell_owner_publishes_the_wording_folds(self):
        self.assertTrue(SHELL_PRESENTATION.is_file(), SHELL_PRESENTATION)
        source = SHELL_PRESENTATION.read_text(encoding="utf-8")
        for symbol in SHELL_OWNER_SYMBOLS:
            with self.subTest(symbol=symbol):
                self.assertIn(symbol, source)

    def test_the_quick_gate_runs_this_check(self):
        """An unwired owner test is a dead test, not evidence."""
        source = LOCAL_GATES.read_text(encoding="utf-8")
        self.assertIn(
            "scripts/quality/test_process_priority_owner.py",
            source,
            "the priority-vocabulary owner test must be wired into quick",
        )

    def test_no_frontend_declares_its_own_tier_variants(self):
        offenders: list[str] = []
        for path in frontend_sources():
            masked = code_only(path.read_text(encoding="utf-8"))
            for match in LOCAL_PRIORITY_VOCAB.finditer(masked):
                if LOCAL_TIER_VARIANT.search(match.group("body")):
                    offenders.append(
                        f"{path.relative_to(CRATES).as_posix()}: "
                        f"{match.group('name')} declares its own tiers"
                    )
        self.assertEqual(
            offenders,
            [],
            "the priority vocabulary has one owner "
            "(taskforest_core::core::process::PriorityTier):\n" + "\n".join(offenders),
        )

    def test_no_frontend_forks_the_core_tier_type(self):
        offenders: list[str] = []
        for path in frontend_sources():
            masked = code_only(path.read_text(encoding="utf-8"))
            for number, line in enumerate(masked.splitlines(), start=1):
                if LOCAL_TIER_TYPE.search(line):
                    offenders.append(
                        f"{path.relative_to(CRATES).as_posix()}:{number}: {line.strip()}"
                    )
        self.assertEqual(
            offenders,
            [],
            "the core tier type is not redeclared by a frontend:\n" + "\n".join(offenders),
        )

    def test_no_frontend_spells_a_tier_key_itself(self):
        offenders: list[str] = []
        for path in frontend_sources():
            masked = mask_comments(path.read_text(encoding="utf-8"))
            for number, line in enumerate(masked.splitlines(), start=1):
                if LOCAL_TIER_KEY.search(line):
                    offenders.append(
                        f"{path.relative_to(CRATES).as_posix()}:{number}: {line.strip()}"
                    )
        self.assertEqual(
            offenders,
            [],
            "tier wording comes from the shell folds:\n" + "\n".join(offenders),
        )

    def test_no_frontend_restates_the_offered_ladder(self):
        offenders: list[str] = []
        for path in frontend_sources():
            masked = code_only(path.read_text(encoding="utf-8"))
            for number, line in enumerate(masked.splitlines(), start=1):
                if LOCAL_TIER_LADDER.search(line):
                    offenders.append(
                        f"{path.relative_to(CRATES).as_posix()}:{number}: {line.strip()}"
                    )
        self.assertEqual(
            offenders,
            [],
            "the offered set and order come from PriorityTier::ALL:\n"
            + "\n".join(offenders),
        )

    def test_the_owners_are_spent_by_the_frontends(self):
        """An owner nobody spends is a dead abstraction, not a contract."""
        sources = "\n".join(path.read_text(encoding="utf-8") for path in frontend_sources())
        for symbol in ("priority_tier_label", "priority_tier_tooltip_key"):
            with self.subTest(symbol=symbol):
                self.assertIn(symbol, sources)

    # ── The detector must bite a fork and spare a binding ──────────────────

    def test_the_detector_bites_a_forked_choice_enum(self):
        match = LOCAL_PRIORITY_VOCAB.search("enum PriorityChoice { High, Normal, Low }")
        self.assertIsNotNone(match)
        self.assertTrue(LOCAL_TIER_VARIANT.search(match.group("body")))

    def test_the_detector_leaves_toolkit_bindings_alone(self):
        for shape in (
            "enum ProcessToolbarAction { SetPriority(PriorityTier) }",
            "enum PriorityAction { SetPriority(PriorityTier) }",
            "struct PriorityOption(PriorityTier);",
            "let label = priority_tier_label(tier);",
        ):
            with self.subTest(shape=shape):
                match = LOCAL_PRIORITY_VOCAB.search(shape)
                if match is None:
                    continue
                self.assertFalse(LOCAL_TIER_VARIANT.search(match.group("body")))


if __name__ == "__main__":
    unittest.main(verbosity=2)
