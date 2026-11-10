#!/usr/bin/env python3
"""One failure-severity ranking for `FailureKind`, at one address.

`FailureKind` is the vocabulary every provider uses to say why a field is
missing, and the product answers one question with it: when several fields of
one source failed differently, **which single failure represents the source**.
That answer is a ranking, and before the owner existed it was spelled out 24
times in four crates (`taskforest-core`, `taskmanager-platform-linux`,
`taskmanager-platform-runtime`, `taskmanager-platform-windows`) under four
function names, in **nine mutually inconsistent orderings**: seventeen of the
24 disagreed with the one the owner adopted, and nine of them tied
`IdentityChanged` with `Rejected` at the bottom, so those subsystems had no
total order and the merge outcome depended on argument order.

The disagreements were not cosmetic. `{Rejected, Unsupported}` reported
`Unsupported` in most subsystems but `Rejected` in the service inventory;
`{ProviderFault, TimedOut}` flipped between the domain collector and the
network counters; the GPU field reader ranked a mid-read identity change third
of nine, above a provider timeout. Two surfaces could therefore print
different headline causes for one identical set of field failures. Only the
health surface documented a principle ("the most actionable kind wins a
merge"), and no test had ever pinned a merge outcome — which is how the drift
survived. That adopted table is now `FailureKind::severity_rank`, with
`FailureKind::stronger` and `FailureKind::strongest` as the only folds.

The rank itself is crate-private in `taskforest-core`, so a comparison outside
the owner does not compile; this guard covers what the compiler cannot see:

* any file outside the owner that binds `FailureKind` variants to integer ranks;
* the owner ranking more than one variant per rank, or losing a variant;
* the owner losing its ranking or its folds;
* a site outside the owner reading `severity_rank` at all — that read is a
  hand-spelled fold, and 27 of them were the second half of this drift;
* a re-export of the ranking as a forwarding facade.

Deliberately NOT flagged: rankings over a *different* failure vocabulary.
`ProviderFailure`, `SmartProviderFailureKind`, `NvmlFailureKind` and
`ProcessMetadataFailure` answer their own questions and keep their own tables —
folding them in would be one layer redefining another's fact. Plain
classifiers that merely *return* a `FailureKind` are not rankings either.

Usage::

    python3 scripts/quality/test_failure_severity_owner.py
"""

from __future__ import annotations

import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CRATES = ROOT / "crates"

OWNER_CRATE = "taskforest-core"
OWNER_PATH = CRATES / OWNER_CRATE / "src/core/failure.rs"
OWNER_REL = str(OWNER_PATH.relative_to(ROOT))
# The owner's own pinned tests live in the crate but in a separate file; they
# are the only other place allowed to read the rank.
OWNER_TEST_PATH = CRATES / OWNER_CRATE / "tests/headless/core_core_failure_tests.rs"
OWNER_TEST_REL = str(OWNER_TEST_PATH.relative_to(ROOT))
OWNER_FILES = {OWNER_PATH: OWNER_REL, OWNER_TEST_PATH: OWNER_TEST_REL}

# The owner must hold the ranking and both folds.
OWNER_SYMBOLS = (
    "const fn severity_rank(self) -> u8",
    "pub const fn stronger(self, other: Self) -> Self",
    "pub const fn strongest(current: Option<Self>, candidate: Self) -> Self",
)

# A ranking table binds FailureKind variants (alternation allowed) to integers.
# Detecting the ARMS rather than a signature is the point: the drift never
# repeated a name, and a table could equally be a const array. The lookbehind
# keeps a *different* vocabulary that merely ends in `FailureKind`
# (`SmartProviderFailureKind`) out of the census.
RANK_ARM = re.compile(
    r"(?<!\w)FailureKind::\w+(?:\s*\|\s*(?<!\w)FailureKind::\w+)*\s*=>\s*\d+"
)

# Three arms is the floor: a two-way comparison is a decision, not a ranking of
# a nine-variant vocabulary.
TABLE_FLOOR = 3

# Reading the rank is a fold written by hand. The owner exposes the folds, not
# the number, so outside the owner this reference is always a second spelling.
RANK_READ = re.compile(r"\bseverity_rank\b")

FACADE = re.compile(r"pub\s+use\s+taskforest_core::core::failure::[^;]*severity_rank")


def rank_arms(text: str) -> list[str]:
    return RANK_ARM.findall(text)


def source_files():
    for path in sorted(CRATES.rglob("*.rs")):
        yield path, path.read_text(encoding="utf-8", errors="replace")


class OwnerHoldsTheRanking(unittest.TestCase):
    def test_owner_defines_the_ranking_and_the_folds(self):
        self.assertTrue(OWNER_PATH.is_file(), f"missing owner {OWNER_PATH}")
        text = OWNER_PATH.read_text(encoding="utf-8")
        for symbol in OWNER_SYMBOLS:
            with self.subTest(symbol=symbol):
                self.assertIn(symbol, text)

    def test_owner_ranks_every_variant_exactly_once(self):
        arms = rank_arms(OWNER_PATH.read_text(encoding="utf-8"))
        self.assertEqual(len(arms), 9, f"owner ranked {len(arms)} variants: {arms}")
        self.assertEqual(len(set(arms)), len(arms), "two variants share a rank")
        ranks = sorted(int(arm.rsplit("=>", 1)[1]) for arm in arms)
        self.assertEqual(ranks, list(range(1, 10)), "ranks must be a strict total order")

    def test_rank_is_not_public_so_a_second_fold_cannot_compile(self):
        text = OWNER_PATH.read_text(encoding="utf-8")
        self.assertNotIn("pub const fn severity_rank", text, "the rank escaped the crate")

    def test_real_tree_ranks_failure_kind_in_exactly_one_file(self):
        found = {
            str(path.relative_to(ROOT)): len(arms)
            for path, text in source_files()
            if (arms := rank_arms(text)) and len(arms) >= TABLE_FLOOR
        }
        self.assertEqual(found, {OWNER_REL: 9},
                         "FailureKind may carry a variant-to-rank table in exactly "
                         "one place; a second table lets two surfaces name different "
                         "headline causes for one set of field failures")

    def test_real_tree_folds_only_through_the_owner(self):
        offenders = [
            str(path.relative_to(ROOT))
            for path, text in source_files()
            if path not in OWNER_FILES and RANK_READ.search(text)
        ]
        self.assertEqual(offenders, [],
                         "reading FailureKind::severity_rank outside the owner is a "
                         "hand-spelled fold; call FailureKind::stronger or "
                         "FailureKind::strongest instead")

    def test_no_forwarding_facade_for_the_ranking(self):
        offenders = [
            str(path.relative_to(ROOT))
            for path, text in source_files()
            if path not in OWNER_FILES and FACADE.search(text)
        ]
        self.assertEqual(offenders, [], "the ranking is bound by call, not re-exported")


class TheGuardBites(unittest.TestCase):
    def test_a_second_table_is_reported_whatever_it_is_called(self):
        for name in ("failure_priority", "priority", "gpu_failure_priority",
                     "smart_failure_priority", "rank_failure"):
            sample = f"""
const fn {name}(failure: FailureKind) -> u8 {{
    match failure {{
        FailureKind::RequiresEscalation => 9,
        FailureKind::PermissionDenied => 8,
        FailureKind::Unsupported => 1,
    }}
}}
"""
            with self.subTest(name=name):
                self.assertGreaterEqual(len(rank_arms(sample)), TABLE_FLOOR)

    def test_an_alternation_arm_that_ties_two_variants_is_still_counted(self):
        # A table may hide a tie behind `|`; the owner forbids ties, and nine
        # of the 24 drifted tables tied IdentityChanged with Rejected.
        sample = """
const fn priority(failure: FailureKind) -> u8 {
    match failure {
        FailureKind::RequiresEscalation => 9,
        FailureKind::PermissionDenied => 8,
        FailureKind::IdentityChanged | FailureKind::Rejected => 1,
    }
}
"""
        self.assertGreaterEqual(len(rank_arms(sample)), TABLE_FLOOR)

    def test_a_table_that_is_not_a_function_is_still_reported(self):
        sample = """
const RANKS: [(FailureKind, u8); 3] = [
    (FailureKind::TimedOut => 6, 0),
    (FailureKind::ProviderFault => 5, 0),
    (FailureKind::Rejected => 1, 0),
];
"""
        self.assertGreaterEqual(len(rank_arms(sample)), TABLE_FLOOR)

    def test_each_drifted_fold_spelling_is_reported(self):
        # The four dialects the 27 hand-spelled folds were written in.
        for label, sample in {
            "comparison": """
if left.severity_rank() < right.severity_rank() {
    right
} else {
    left
}
""",
            "guard_then_replace": """
if current.is_none_or(|current| current.severity_rank() < candidate.severity_rank()) {
    *current = Some(candidate);
}
""",
            "max_by_key": """
let worst = failures.iter().max_by_key(|failure| failure.severity_rank());
""",
            "qualified_read": "let delta = FailureKind::severity_rank(kind);\n",
        }.items():
            with self.subTest(dialect=label):
                self.assertTrue(RANK_READ.search(sample), f"{label} fold escaped the guard")


class TheGuardDoesNotOverReach(unittest.TestCase):
    def test_other_failure_vocabularies_are_not_the_owner_fact(self):
        for typ in ("ProviderFailure", "SmartProviderFailureKind",
                    "ProcessMetadataFailure", "NvmlFailureKind"):
            sample = f"""
const fn failure_priority(failure: {typ}) -> u8 {{
    match failure {{
        {typ}::PidRace => 9,
        {typ}::PermissionDenied => 8,
        {typ}::Unsupported => 2,
    }}
}}
"""
            with self.subTest(kind=typ):
                self.assertEqual(rank_arms(sample), [])

    def test_a_classifier_returning_a_failure_is_not_a_ranking(self):
        sample = """
const fn classify_io(error: &io::Error) -> FailureKind {
    match error.kind() {
        io::ErrorKind::NotFound => FailureKind::Unsupported,
        io::ErrorKind::PermissionDenied => FailureKind::PermissionDenied,
        _ => FailureKind::TemporarilyUnavailable,
    }
}
"""
        self.assertEqual(rank_arms(sample), [])
        self.assertIsNone(RANK_READ.search(sample))

    def test_a_call_site_of_the_owner_fold_is_not_a_second_table(self):
        for label, sample in {
            "pairwise": "let worst = current.stronger(candidate);\n",
            "accumulator": """
failure = Some(FailureKind::strongest(failure, candidate));
""",
            "reduce": "let worst = failures.reduce(FailureKind::stronger);\n",
            "optional_pair_adapter": """
fn preferred_failure(current: Option<FailureKind>, other: Option<FailureKind>)
    -> Option<FailureKind> {
    match (current, other) {
        (Some(current), Some(other)) => Some(current.stronger(other)),
        (Some(current), None) => Some(current),
        (None, other) => other,
    }
}
"""
        }.items():
            with self.subTest(shape=label):
                self.assertEqual(rank_arms(sample), [])
                self.assertIsNone(RANK_READ.search(sample),
                                  f"{label} bound to the owner was read as a fold")


if __name__ == "__main__":
    suite = unittest.TestLoader().loadTestsFromModule(__import__("__main__"))
    result = unittest.TextTestRunner(verbosity=1).run(suite)
    raise SystemExit(0 if result.wasSuccessful() else 1)
