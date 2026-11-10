#!/usr/bin/env python3
"""One address for process-control action copy and failure reasons.

A control failure is described in two places in the product: the text the user
reads, and the reason it failed. Both were spelled per frontend. At the last
measured baseline GPUI held *two* second authorities for the reason — a local
`control_error_detail` table in `root/platform_lists.rs` that was byte-identical
to the owner, and a `typed_failure_reason` table inside
`root/process_feedback.rs` that disagreed with it on six of the nine
`FailureKind` variants, four of them by writing English detail strings straight
into source. That copy also kept a frontend-local action vocabulary
(`enum ProcessControlAction`) whose three extra variants nothing constructed,
and re-composed the `feedback.process_action_*` templates that the owner already
composed. The divergence was invisible because both tables returned *some*
reason: a user could read "process provider unavailable" from one path and the
catalog's localized reason from the other for the identical failure.

One owner now: `taskmanager-shell` owns the neutral `ProcessControlKind`
vocabulary, `presentation::help::control_error_detail` answers the reason, and
`process_control_notice_text` alone composes the template — keyed on
`(kind, pid, result)`, so a correlated completion and a submission rejected at
the boundary consume the same mapping. This guard fails the shapes that grew
back:

* a production file outside the owner composing the `feedback.process_action_*`
  templates (a second composition rule for one notice);
* a second declaration of the reason resolver, the notice projection, the
  action-label resolver, or either retired copy name;
* a frontend declaring its own control-action vocabulary instead of binding the
  neutral owner kind;
* a file that composes the notice without importing the owner's projection.

Deliberately NOT flagged: other `feedback.*` keys used inline by a frontend for
its own surface, a plain classifier that merely *returns* a `FailureKind`, and
helpers named `*_feedback` that answer a different question (the batch notice
and the startup/session action-bar copy are their own facts).

Usage::

    python3 scripts/quality/test_control_failure_copy_owner.py
"""

from __future__ import annotations

import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CRATES = ROOT / "crates"

PLATFORM_FEEDBACK = CRATES / "taskmanager-shell/src/app/platform_feedback.rs"
LIB = CRATES / "taskmanager-shell/src/lib.rs"
I18N = CRATES / "taskmanager-application/src/i18n.rs"
FAILURE = CRATES / "taskforest-core/src/core/failure.rs"
PLATFORM_FEEDBACK_REL = str(PLATFORM_FEEDBACK.relative_to(ROOT))
I18N_REL = str(I18N.relative_to(ROOT))
OWNER_FILES = {PLATFORM_FEEDBACK, I18N}

# The one notice template. Composing it outside the owner is a second spelling
# of the same sentence, however faithful the copy looks today.
TEMPLATE_KEYS = (
    "feedback.process_action_failed",
    "feedback.process_action_succeeded",
)

# name -> the single file allowed to declare it; `None` means the name is
# retired and no file may declare it (`typed_failure_reason` and
# `process_control_feedback` were GPUI's second authorities).
DECLARATIONS = {
    "control_error_detail": I18N_REL,
    "process_control_notice_text": PLATFORM_FEEDBACK_REL,
    "process_control_action_label": PLATFORM_FEEDBACK_REL,
    "control_outcome_text": I18N_REL,
    "typed_failure_reason": None,
    "process_control_feedback": None,
}

# The owner's notice projection, which a composing file must import by name.
NOTICE_SYMBOL = "process_control_notice_text"
NOTICE_IMPORT = "taskmanager_shell::process_control_notice_text"

ACTION_VOCABULARY = re.compile(r"\benum\s+ProcessControlAction\b")

# The surfaces that render the CPU-affinity read outcome (loading / observed /
# failed). Each must resolve the reason through the one owner: R140 found
# `taskmanager-iced/src/ui/affinity.rs` printing
# `format!("Current affinity unavailable: {failure:?}")` — an English sentence
# written in source plus a `Debug` dump of the typed failure — while the TUI's
# affinity row simply vanished and the Bevy modal fabricated an all-CPU mask.
AFFINITY_SURFACES = (
    "taskmanager-gpui/src/gpui_app/processes_view/chrome/overlays.rs",
    "taskmanager-tui/src/ui/affinity_modal.rs",
    "taskmanager-tui/src/ui/process_details/insights.rs",
    "taskmanager-iced/src/ui/affinity.rs",
    "taskmanager-bevy-ui/src/pages/processes/affinity.rs",
)

# A failure reason rendered by `Debug` is never the shared reason.
DEBUGGED_FAILURE = re.compile(r"\{(?:failure|kind|f):\?\}")

# A frontend-spelled affinity-failure sentence (the removed hardcoded English).
OWN_AFFINITY_SENTENCE = re.compile(r"(?i)affinity\s+unavailable")

# R141: session/startup/service control outcomes are one fact, composed once by
# `taskmanager-application::i18n::control_outcome_text`. The shell used to
# hardcode `"Session … request accepted"`, `"Startup … completed"`,
# `"Service … failed: {error:?}"` — English sentences that reached every
# language — and the application's `service_log_rejection_detail` used
# `format!("{failure:?}")` as the reason. The last one is why the key fold had
# to move DOWN to `taskforest-core::FailureKind::i18n_key`: the copy owner
# (application) is below the shell, so a resolver in the shell can never serve
# it without a second table.
OUTCOME_KEY_FOLD = re.compile(r"pub\s+const\s+fn\s+i18n_key\s*\(")
SOURCE_OUTCOME_SENTENCE = re.compile(
    r"\"(?:Session|Startup|Service)\s+[^\"]*(?:request accepted|completed|failed)[^\"]*\""
)
DEBUG_REASON = re.compile(r"\{(?:failure|kind|f):\?\}")
SURFACE_FILES = (
    "taskmanager-tui/src/ui/pages.rs",
    "taskmanager-iced/src/ui/users.rs",
    "taskmanager-bevy-ui/src/pages/sessions.rs",
    "taskmanager-application/src/i18n.rs",
)

FRONTEND_CRATES = (
    "taskmanager-gpui",
    "taskmanager-iced",
    "taskmanager-tui",
    "taskmanager-bevy-ui",
)


def production_sources() -> list[Path]:
    """Every `src/` Rust file of every crate, owner included."""
    return sorted(
        path
        for path in CRATES.glob("*/src/**/*.rs")
        if "target" not in path.parts and path.is_file()
    )


def code_lines(text: str) -> list[tuple[int, str]]:
    """The 1-based CODE lines of a source file.

    Comments are not scanned: prose cannot compose a notice, and a guard that
    reddens on prose is a false-FAIL generator that gets ignored.
    """
    lines = []
    for number, line in enumerate(text.splitlines(), 1):
        stripped = line.strip()
        if stripped.startswith(("///", "//!", "//", "/*", "*")):
            continue
        lines.append((number, line))
    return lines


def template_sites(text: str) -> list[int]:
    """CODE lines carrying one of the notice-template keys as a literal."""
    return [
        number
        for number, line in code_lines(text)
        if any(f'"{key}"' in line for key in TEMPLATE_KEYS)
    ]


def declaration_sites(text: str, name: str) -> list[int]:
    """CODE lines declaring `fn name(` — a definition, never a call."""
    pattern = re.compile(rf"\bfn\s+{re.escape(name)}\s*\(")
    return [number for number, line in code_lines(text) if pattern.search(line)]


def rel(path: Path) -> str:
    return str(path.relative_to(ROOT))


class TheCopyHasOneAddress(unittest.TestCase):
    def test_only_the_owner_composes_the_notice_template(self):
        offenders = {
            rel(path): lines
            for path in production_sources()
            if path not in OWNER_FILES and (lines := template_sites(path.read_text(encoding="utf-8")))
        }
        self.assertEqual(
            offenders,
            {},
            "the process-control notice template is composed once, in "
            "platform_feedback::process_control_notice_text; a second "
            "composition is a copy that can drift: "
            + ", ".join(f"{name}:{line}" for name, lines in offenders.items() for line in lines),
        )

    def test_each_owned_resolver_is_declared_in_exactly_one_place(self):
        found = {name: [] for name in DECLARATIONS}
        for path in production_sources():
            text = path.read_text(encoding="utf-8")
            for name in DECLARATIONS:
                if declaration_sites(text, name):
                    found[name].append(rel(path))
        for name, allowed in DECLARATIONS.items():
            with self.subTest(symbol=name):
                expected = [] if allowed is None else [allowed]
                if allowed is None:
                    message = (
                        f"{name} is a retired second authority; it must not be "
                        "declared anywhere"
                    )
                else:
                    message = (
                        f"{name} is declared in {found[name]}; one fact has one "
                        f"authority ({allowed})"
                    )
                self.assertEqual(found[name], expected, message)

    def test_no_frontend_declares_its_own_control_action_vocabulary(self):
        offenders = {
            rel(path): lines
            for path in production_sources()
            if (lines := [
                number
                for number, line in code_lines(path.read_text(encoding="utf-8"))
                if ACTION_VOCABULARY.search(line)
            ])
        }
        self.assertEqual(
            offenders,
            {},
            "a frontend-local control-action vocabulary is a second authority "
            "for the neutral ProcessControlKind; the three extra variants the "
            "last copy carried were never constructed: "
            + ", ".join(f"{name}:{line}" for name, lines in offenders.items() for line in lines),
        )

    def test_a_composing_file_binds_the_owner(self):
        offenders = []
        for crate in FRONTEND_CRATES:
            for path in sorted(CRATES.glob(f"{crate}/src/**/*.rs")):
                text = path.read_text(encoding="utf-8")
                if not re.search(rf"\b{NOTICE_SYMBOL}\s*\(", text):
                    continue
                if NOTICE_IMPORT not in "".join(text.split()):
                    offenders.append(
                        f"{rel(path)} names {NOTICE_SYMBOL} without importing "
                        f"{NOTICE_IMPORT}"
                    )
        self.assertEqual(
            offenders,
            [],
            "a frontend reaches the notice only by calling the owner's "
            "projection, imported by name",
        )

    def test_the_owner_exports_the_notice(self):
        self.assertIn(
            NOTICE_SYMBOL,
            LIB.read_text(encoding="utf-8"),
            "the notice projection must stay exported or each frontend will "
            "re-compose it",
        )
        self.assertTrue(
            PLATFORM_FEEDBACK.is_file() and I18N.is_file(),
            "the owner files must still exist",
        )


class TheAffinityReadOutcomeHasOneReason(unittest.TestCase):
    def test_each_affinity_surface_binds_the_owner_reason(self):
        for surface in AFFINITY_SURFACES:
            path = ROOT / "crates" / surface
            with self.subTest(surface=surface):
                self.assertTrue(path.is_file(), f"missing affinity surface {surface}")
                self.assertIn(
                    "control_error_detail",
                    path.read_text(encoding="utf-8"),
                    "an affinity surface must read the shared reason instead of "
                    "spelling (or Debug-formatting) its own",
                )

    def test_no_frontend_spells_its_own_affinity_failure_sentence(self):
        offenders = {}
        for path in production_sources():
            if not any(part.startswith("taskmanager-") for part in path.parts):
                continue
            sites = [
                number
                for number, line in code_lines(path.read_text(encoding="utf-8"))
                if OWN_AFFINITY_SENTENCE.search(line)
            ]
            if sites:
                offenders[rel(path)] = sites
        self.assertEqual(
            offenders,
            {},
            "the affinity-failure sentence comes from the catalog via the "
            "owner; an English sentence inside a taskmanager-* crate reaches "
            "every language: "
            + ", ".join(f"{name}:{line}" for name, lines in offenders.items() for line in lines),
        )

    def test_no_affinity_surface_debugs_a_typed_failure(self):
        offenders = {}
        for surface in AFFINITY_SURFACES:
            path = ROOT / "crates" / surface
            sites = [
                number
                for number, line in code_lines(path.read_text(encoding="utf-8"))
                if DEBUGGED_FAILURE.search(line)
            ]
            if sites:
                offenders[surface] = sites
        self.assertEqual(
            offenders,
            {},
            "a Debug-formatted failure is not the shared reason (the TUI's own "
            "G-04b rule, now enforced for every affinity surface): "
            + ", ".join(f"{name}:{line}" for name, lines in offenders.items() for line in lines),
        )


class TheControlOutcomeHasOneComposition(unittest.TestCase):
    def test_the_key_fold_lives_in_core_and_resolution_lives_in_application(self):
        self.assertTrue(FAILURE.is_file(), "FailureKind owner must exist")
        self.assertIn(
            "pub const fn i18n_key(self)",
            FAILURE.read_text(encoding="utf-8"),
            "FailureKind owns the key vocabulary (a pure core fact)",
        )
        i18n_text = I18N.read_text(encoding="utf-8")
        self.assertIn(
            "pub fn control_error_detail(failure: FailureKind)", i18n_text,
            "the application i18n owner is the one key→language resolver",
        )
        self.assertIn(
            "pub fn control_outcome_text(", i18n_text,
            "one composition for session/startup/service outcomes",
        )

    def test_no_table_outside_the_key_owner_maps_failure_variants_to_feedback_keys(self):
        offenders = {}
        for path in production_sources():
            if path == FAILURE or path == I18N:
                continue
            lines = [
                number
                for number, line in code_lines(path.read_text(encoding="utf-8"))
                if re.search(r"FailureKind::\w+.*=>\s*\"feedback\.", line)
                or re.search(r"match\s+(?:kind|failure)\s*\{", line)
                and "feedback." in line
            ]
            if lines:
                offenders[rel(path)] = lines
        self.assertEqual(
            offenders, {},
            "a FailureKind → feedback key table outside core is a second "
            "authority for the reason: "
            + ", ".join(f"{name}:{line}" for name, lines in offenders.items() for line in lines),
        )

    def test_no_source_writes_an_english_outcome_sentence(self):
        offenders = {}
        for path in production_sources():
            lines = [
                number
                for number, line in code_lines(path.read_text(encoding="utf-8"))
                if SOURCE_OUTCOME_SENTENCE.search(line)
            ]
            if lines:
                offenders[rel(path)] = lines
        self.assertEqual(
            offenders, {},
            "session/startup/service outcome sentences come from the catalog: "
            + ", ".join(f"{name}:{line}" for name, lines in offenders.items() for line in lines),
        )

    def test_no_outcome_surface_debugs_a_typed_failure_as_the_reason(self):
        offenders = {}
        for surface in SURFACE_FILES:
            path = ROOT / "crates" / surface
            self.assertTrue(path.is_file(), f"missing outcome surface {surface}")
            lines = [
                number
                for number, line in code_lines(path.read_text(encoding="utf-8"))
                if DEBUG_REASON.search(line)
            ]
            if lines:
                offenders[surface] = lines
        self.assertEqual(
            offenders, {},
            "a Debug dump of typed failure is not localized copy: "
            + ", ".join(f"{name}:{line}" for name, lines in offenders.items() for line in lines),
        )

    def test_the_frontend_session_lines_bind_the_owner(self):
        for surface in SURFACE_FILES[:3]:
            text = (ROOT / "crates" / surface).read_text(encoding="utf-8")
            with self.subTest(surface=surface):
                self.assertIn(
                    "control_outcome_text", text,
                    f"{surface} must compose through the one owner",
                )


class TheGuardBites(unittest.TestCase):
    def test_a_frontend_re_spelling_the_template_is_reported(self):
        sample = """
fn local_notice() -> String {
    t("feedback.process_action_failed")
        .replace("{action}", &label)
}
"""
        self.assertEqual(template_sites(sample), [3])

    def test_a_local_reason_table_is_reported_whatever_it_returns(self):
        for name in ("control_error_detail", "typed_failure_reason"):
            sample = f"""
pub(crate) fn {name}(kind: FailureKind) -> &'static str {{
    match kind {{
        FailureKind::TimedOut => i18n::t("feedback.timed_out"),
        _ => i18n::t("feedback.unsupported"),
    }}
}}
"""
            with self.subTest(name=name):
                self.assertEqual(declaration_sites(sample, name), [2])

    def test_a_local_action_vocabulary_is_reported(self):
        sample = """
#[derive(Clone, Copy)]
pub enum ProcessControlAction {
    EndTask,
    Kill,
}
"""
        self.assertEqual(
            [number for number, line in code_lines(sample) if ACTION_VOCABULARY.search(line)],
            [3],
        )

    def test_the_removed_affinity_sentence_is_reported(self):
        sample = """
        let status = format!("Current affinity unavailable: {failure:?}");
"""
        self.assertEqual(
            [
                number
                for number, line in code_lines(sample)
                if OWN_AFFINITY_SENTENCE.search(line)
            ],
            [2],
        )
        self.assertEqual(
            [number for number, line in code_lines(sample) if DEBUGGED_FAILURE.search(line)],
            [2],
        )

    def test_the_removed_shell_outcome_sentences_and_debug_reason_are_reported(self):
        samples = (
            'format!("Session {} {:?} request accepted", id, action)',
            'format!("Startup {} {intent} completed", name)',
            'format!("Service {} {:?} failed: {error:?}", id, action)',
            't("service.log_request_rejected").replace("{reason}", &format!("{failure:?}"))',
        )
        for sample in samples:
            with self.subTest(sample=sample):
                self.assertEqual(
                    [1],
                    [
                        number
                        for number, line in code_lines(sample)
                        if SOURCE_OUTCOME_SENTENCE.search(line)
                        or DEBUG_REASON.search(line)
                    ],
                    "the retired shape must be reported",
                )


class TheGuardDoesNotOverReach(unittest.TestCase):
    def test_a_classifier_that_returns_a_failure_is_not_a_reason_table(self):
        sample = """
const fn submission_error_kind(kind: SubmissionErrorKind) -> FailureKind {
    match kind {
        SubmissionErrorKind::Busy => FailureKind::Rejected,
        SubmissionErrorKind::InvalidRequest => FailureKind::ProviderFault,
    }
}
"""
        for name in DECLARATIONS:
            self.assertEqual(declaration_sites(sample, name), [])
        self.assertEqual(template_sites(sample), [])
        self.assertEqual(
            [line for line in code_lines(sample) if ACTION_VOCABULARY.search(line[1])],
            [],
        )

    def test_other_feedback_helpers_and_keys_are_other_facts(self):
        sample = """
fn control_feedback(result: Result<(), FailureKind>, action: &str) -> ActionFeedback {
    let text = t("feedback.process_action_queued").to_string();
    let reason = control_error_detail(kind);
    ActionFeedback::from_result(&reason, action, &text)
}
"""
        # A different template key, a call (not a declaration) of the owner
        # resolver, and a helper named `*_feedback` are not this fact.
        self.assertEqual(template_sites(sample), [])
        for name in DECLARATIONS:
            self.assertEqual(declaration_sites(sample, name), [])

    def test_prose_in_a_doc_comment_is_not_scanned(self):
        sample = """
//! The retired copy spelled `typed_failure_reason` and the literal
//! "feedback.process_action_failed" inside the frontend; this explains why
//! it is gone.
/// Historically `fn control_error_detail` lived here.
fn current() {}
"""
        self.assertEqual(template_sites(sample), [])
        for name in DECLARATIONS:
            self.assertEqual(declaration_sites(sample, name), [])


if __name__ == "__main__":
    suite = unittest.TestLoader().loadTestsFromModule(__import__("__main__"))
    result = unittest.TextTestRunner(verbosity=1).run(suite)
    raise SystemExit(0 if result.wasSuccessful() else 1)
