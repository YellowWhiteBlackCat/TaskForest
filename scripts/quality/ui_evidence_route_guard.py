#!/usr/bin/env python3
"""Mechanically pin the ui-evidence route's impact classification.

``scripts/quality/ui-evidence-route.sh`` decides, for a diff, which frontends
owe a fresh pixel receipt.  Its classification is a hand-maintained path table
plus a content-level locale classifier, so a later edit can silently
*re-broaden* it (demand a receipt for a dev-only test tree that cannot move a
shipped pixel, or for a key-set-only locale edit) or *weaken* it (drop a demand
for a path that really reaches a frontend's paint path, let an unknown
ui-contract path fall through to a headless-only verdict, or let a translated
value change skip its receipts).  Either drift is a gate defect: the first
spends capture effort on a change no frame can reveal, the second ships a pixel
change with no frame at all.

This guard runs the *real* route script (copied verbatim into a throwaway git
repository) against a fixed set of synthetic diffs and asserts the verdict for
each.  It is a behavior test: it never parses the route's source text.  A
re-broadening or a weakening changes the observed verdict and fails here.

The expectations encode these proofs:

* A frontend integration-test tree (``tests/gui/**`` and
  ``crates/taskmanager-{gpui,ui,tui,iced,bevy-ui}/tests/**``) is a separate
  compilation unit.  ``cargo build`` emits the product binary from ``src/**``
  and its normal dependencies; it never links a ``tests/**`` file into the
  image, so a change confined there cannot move a shipped pixel.  The route
  keeps the headless channel (``ui_touched``) and owes no capture receipt.
* ``crates/taskmanager-ui-contract/tests/**`` and its declaration-only modules
  (``capabilities``/``conformance``/``functional``/``keybindings``/``message``/
  ``feature_coverage*``/``accessibility*``) are consumed by declaration
  adapters and headless gates, not by a paint path.
* ``ui-contract/src/{focus,columns,icon,command,navigation}.rs`` are imported
  by real render consumers -- ``taskmanager-ui`` (linked only by GPUI) for
  ``focus``, the GPUI/Iced/Bevy tables for ``columns``, the shared shell
  presentation for ``icon``/``command``/``navigation`` -- so they keep exactly
  those consumers' receipts.
* An unlisted ``ui-contract`` path (``lib.rs``, ``Cargo.toml``, a new module)
  may reach any consumer: ``lib.rs`` can re-point a re-export and
  ``Cargo.toml`` can change a dependency's behavior, neither provably
  pixel-neutral, so it stays fail-closed at all four.
* ``locales/*`` strings are ``include_str!``-embedded by the shared application
  layer every product links, so a translated value change can repaint all four.
  The route therefore reads the change's *content* through
  ``scripts/quality/locale_keyset_classifier.py``: a key-set-only edit (whole
  keys added/removed, no surviving value changed, catalog set and cross-catalog
  key relationship preserved) owes only the headless channel, while a
  value-affecting edit, an asymmetric add/remove, an unparsable or missing
  catalog, or a changed catalog file set stays fail-closed at all four.  The
  fixtures below pin both halves, and the self-test proves the guard goes red
  when the route ignores the classifier or inverts it.

The helper is a shipped part of the route: the guard copies it next to the
route in every fixture and fails closed (exit 2) when it is missing.

Exit codes: 0 the live route matches every expectation, 1 a classification
finding, 2 the guard could not build or parse its fixture (fail closed).

Usage::

    python3 scripts/quality/ui_evidence_route_guard.py
    python3 scripts/quality/ui_evidence_route_guard.py --self-test
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
ROUTE_REL = "scripts/quality/ui-evidence-route.sh"
HELPER_REL = "scripts/quality/locale_keyset_classifier.py"

# Every baseline path the scenarios modify.  The untracked-create scenario
# deliberately omits its path so the route sees it only through
# `git ls-files --others`.
BASELINE_PATHS = (
    "crates/taskmanager-ui-contract/tests/headless/ui_focus.rs",
    "crates/taskmanager-ui-contract/src/feature_coverage.rs",
    "crates/taskmanager-ui-contract/src/columns.rs",
    "crates/taskmanager-ui-contract/src/focus.rs",
    "crates/taskmanager-gpui/src/gpui_app/root.rs",
    "crates/taskmanager-gpui/tests/gui/gpui_app/tests.rs",
    "crates/taskmanager-ui/tests/gui/selectable_text.rs",
    "crates/taskmanager-tui/tests/gui/header.rs",
    "crates/taskmanager-iced/tests/gui/overlay.rs",
    "crates/taskmanager-bevy-ui/tests/headless/capabilities.rs",
    "tests/gui/accessibility_behavior.rs",
    "locales/en.json",
    "locales/zh.json",
    "crates/taskmanager-core/src/lib.rs",
)

# Valid, symmetric catalogs for the locale scenarios (a non-JSON placeholder
# would classify as unparsable and never exercise the key-set path).  English
# deliberately carries one extra key, mirroring the repository's real fallback
# fixture: the classifier must tolerate a *pre-existing* asymmetry while
# refusing a new one.
LOCALE_EN_KEYS = {
    "tab.performance": "Performance",
    "tooltip.cancel": "Cancel",
    "fallback.sample": "Sample",
}
LOCALE_ZH_KEYS = {
    "tab.performance": "性能",
    "tooltip.cancel": "取消",
}


def _locale_json(keys: dict[str, str]) -> str:
    return json.dumps(keys, ensure_ascii=False) + "\n"


BASELINE_CONTENTS = {
    "locales/en.json": _locale_json(LOCALE_EN_KEYS),
    "locales/zh.json": _locale_json(LOCALE_ZH_KEYS),
}


# The case-statement anchor the --self-test mutations are injected after.  It
# must be unique; run_self_test fails closed otherwise so a future second
# `case "$path" in` cannot silently redirect the mutation.
CASE_ANCHOR = '    case "$path" in\n'


@dataclass(frozen=True)
class Scenario:
    name: str
    path: str
    verdict: str  # "no-ui" | "headless-only" | "capture"
    missing: tuple[str, ...] = ()
    create: bool = False
    # Explicit current contents, used by the locale scenarios to write valid
    # (or deliberately invalid) catalogs instead of appending "// changed".
    writes: tuple[tuple[str, str], ...] = ()


SCENARIOS: tuple[Scenario, ...] = (
    # --- ui-contract: declaration/test surface stays headless -------------
    Scenario(
        "ui-contract-test",
        "crates/taskmanager-ui-contract/tests/headless/ui_focus.rs",
        "headless-only",
    ),
    Scenario(
        "ui-contract-declaration",
        "crates/taskmanager-ui-contract/src/feature_coverage.rs",
        "headless-only",
    ),
    # --- ui-contract: paint-relevant inputs keep their real consumers -----
    Scenario(
        "ui-contract-columns",
        "crates/taskmanager-ui-contract/src/columns.rs",
        "capture",
        missing=("gpui", "iced", "bevy"),
    ),
    Scenario(
        "ui-contract-focus",
        "crates/taskmanager-ui-contract/src/focus.rs",
        "capture",
        missing=("gpui",),
    ),
    # --- fail-closed default for an unlisted ui-contract path -------------
    Scenario(
        "ui-contract-unknown",
        "crates/taskmanager-ui-contract/src/zz_new_module.rs",
        "capture",
        missing=("gpui", "tui", "iced", "bevy"),
        create=True,
    ),
    # --- dev-only frontend test trees stay headless -----------------------
    Scenario(
        "test-tree-gpui",
        "crates/taskmanager-gpui/tests/gui/gpui_app/tests.rs",
        "headless-only",
    ),
    Scenario(
        "test-tree-ui",
        "crates/taskmanager-ui/tests/gui/selectable_text.rs",
        "headless-only",
    ),
    Scenario(
        "test-tree-tui",
        "crates/taskmanager-tui/tests/gui/header.rs",
        "headless-only",
    ),
    Scenario(
        "test-tree-iced",
        "crates/taskmanager-iced/tests/gui/overlay.rs",
        "headless-only",
    ),
    Scenario(
        "test-tree-bevy",
        "crates/taskmanager-bevy-ui/tests/headless/capabilities.rs",
        "headless-only",
    ),
    Scenario(
        "test-tree-root-gui",
        "tests/gui/accessibility_behavior.rs",
        "headless-only",
    ),
    # --- product frontends and shared pixels still owe receipts -----------
    Scenario(
        "gpui-product",
        "crates/taskmanager-gpui/src/gpui_app/root.rs",
        "capture",
        missing=("gpui",),
    ),
    # --- locale catalogs: content decides, not the path -------------------
    # Pure key addition (to every catalog at once) moves no painted string.
    Scenario(
        "locale-keyset-add",
        "locales/en.json",
        "headless-only",
        writes=(
            ("locales/en.json", _locale_json({**LOCALE_EN_KEYS, "tab.alerts": "Alerts"})),
            ("locales/zh.json", _locale_json({**LOCALE_ZH_KEYS, "tab.alerts": "警报"})),
        ),
    ),
    # Pure key removal (from every catalog at once) moves no painted string.
    # The pre-existing en-only fallback key is untouched, so the relationship
    # survives; removing it would be a relationship change (see the helper).
    Scenario(
        "locale-keyset-remove",
        "locales/en.json",
        "headless-only",
        writes=(
            (
                "locales/en.json",
                _locale_json({"tab.performance": "Performance", "fallback.sample": "Sample"}),
            ),
            ("locales/zh.json", _locale_json({"tab.performance": "性能"})),
        ),
    ),
    # A surviving translated value changed: every frontend can repaint.
    Scenario(
        "locale-value-change",
        "locales/zh.json",
        "capture",
        missing=("gpui", "tui", "iced", "bevy"),
        writes=(
            ("locales/en.json", _locale_json(LOCALE_EN_KEYS)),
            ("locales/zh.json", _locale_json({**LOCALE_ZH_KEYS, "tab.performance": "效能"})),
        ),
    ),
    # A value changed on a key that is simultaneously removed elsewhere: the
    # removal must not mask the value change.
    Scenario(
        "locale-value-change-on-removed-key",
        "locales/en.json",
        "capture",
        missing=("gpui", "tui", "iced", "bevy"),
        writes=(
            ("locales/en.json", _locale_json({**LOCALE_EN_KEYS, "tab.performance": "Perf"})),
            ("locales/zh.json", _locale_json({"tooltip.cancel": "取消"})),
        ),
    ),
    # An unparsable catalog cannot be classified safely: fail closed.
    Scenario(
        "locale-unparsable",
        "locales/en.json",
        "capture",
        missing=("gpui", "tui", "iced", "bevy"),
        writes=(
            ("locales/en.json", "{ this is not valid json\n"),
            ("locales/zh.json", _locale_json(LOCALE_ZH_KEYS)),
        ),
    ),
    # An asymmetric add would move the cross-catalog key relationship (and fail
    # the symmetry nextest gate): fail closed.
    Scenario(
        "locale-asymmetric-add",
        "locales/en.json",
        "capture",
        missing=("gpui", "tui", "iced", "bevy"),
        writes=(
            ("locales/en.json", _locale_json({**LOCALE_EN_KEYS, "tab.alerts": "Alerts"})),
            ("locales/zh.json", _locale_json(LOCALE_ZH_KEYS)),
        ),
    ),
    # --- an unrelated change never enters the UI route --------------------
    Scenario(
        "unrelated",
        "crates/taskmanager-core/src/lib.rs",
        "no-ui",
    ),
)

EXPECTED_SELF_TEST_FAILURES = {
    "rebroaden-contract-tests": {"ui-contract-test"},
    "weaken-contract-table": {
        "ui-contract-columns",
        "ui-contract-focus",
        "ui-contract-unknown",
    },
    # Ignoring the locale content classifier re-broadens a key-set-only edit to
    # all four receipts.
    "remove-locales-classifier": {"locale-keyset-add", "locale-keyset-remove"},
    # Inverting it sends a value-affecting edit to headless-only and a
    # key-set-only edit to all four.
    "invert-locales-classifier": {
        "locale-keyset-add",
        "locale-keyset-remove",
        "locale-value-change",
        "locale-value-change-on-removed-key",
        "locale-unparsable",
        "locale-asymmetric-add",
    },
}

SELF_TEST_MUTATIONS = {
    "rebroaden-contract-tests": (
        "    crates/taskmanager-ui-contract/tests/*)\n"
        "        ui_touched=1\n"
        "        gpui_touched=1\n"
        "        ;;\n"
    ),
    "weaken-contract-table": (
        "    crates/taskmanager-ui-contract/*)\n"
        "        ui_touched=1\n"
        "        ;;\n"
    ),
    # Force the old path-only locales arm, defeating the classifier.
    "remove-locales-classifier": (
        "    locales/*)\n"
        "        ui_touched=1\n"
        "        gpui_touched=1\n"
        "        tui_touched=1\n"
        "        iced_touched=1\n"
        "        bevy_touched=1\n"
        "        ;;\n"
    ),
    # Swap the two verdicts: key-set-only now demands all four and a value
    # change demands none.
    "invert-locales-classifier": (
        "    locales/*)\n"
        "        ui_touched=1\n"
        "        if [[ \"$locales_classification\" == \"key-set-only\" ]]; then\n"
        "            gpui_touched=1\n"
        "            tui_touched=1\n"
        "            iced_touched=1\n"
        "            bevy_touched=1\n"
        "        fi\n"
        "        ;;\n"
    ),
}


class FixtureError(RuntimeError):
    """The guard could not build or run its throwaway repository."""


def _git(repo: Path, *args: str) -> str:
    try:
        completed = subprocess.run(
            ["git", "-C", str(repo), *args],
            check=True,
            capture_output=True,
            text=True,
            timeout=60,
            env={
                **os.environ,
                "GIT_AUTHOR_NAME": "ui-route-guard",
                "GIT_AUTHOR_EMAIL": "ui-route-guard@users.noreply.github.com",
                "GIT_COMMITTER_NAME": "ui-route-guard",
                "GIT_COMMITTER_EMAIL": "ui-route-guard@users.noreply.github.com",
            },
        )
    except (OSError, subprocess.SubprocessError) as exc:
        detail = getattr(exc, "stderr", "") or str(exc)
        raise FixtureError(f"git {' '.join(args)} failed: {detail.strip()}") from exc
    return completed.stdout


def build_fixture(tmp: Path, route_text: str) -> tuple[Path, str]:
    """Create a throwaway git repo holding the route, helper, and a baseline."""
    tmp.mkdir(parents=True, exist_ok=True)
    _git(tmp, "init", "-q")
    route_path = tmp / ROUTE_REL
    route_path.parent.mkdir(parents=True, exist_ok=True)
    route_path.write_text(route_text, encoding="utf-8")
    # The route invokes the content classifier by path; a fixture without it
    # would fail closed and the guard could not observe the key-set verdict.
    helper_source = REPO_ROOT / HELPER_REL
    if not helper_source.is_file():
        raise FixtureError(f"route helper missing: {helper_source}")
    (tmp / HELPER_REL).write_text(helper_source.read_text(encoding="utf-8"), encoding="utf-8")
    (tmp / ".tmp").mkdir(exist_ok=True)
    for rel in BASELINE_PATHS:
        target = tmp / rel
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(
            BASELINE_CONTENTS.get(rel, "// baseline placeholder\n"), encoding="utf-8"
        )
    _git(tmp, "add", "-A")
    _git(tmp, "commit", "-q", "-m", "baseline")
    return tmp, _git(tmp, "rev-parse", "HEAD").strip()


def apply_scenario(tmp: Path, base: str, scenario: Scenario) -> None:
    _git(tmp, "reset", "-q", "--hard", base)
    _git(tmp, "clean", "-qfdx")
    (tmp / ".tmp").mkdir(exist_ok=True)
    if scenario.writes:
        for rel, content in scenario.writes:
            target = tmp / rel
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text(content, encoding="utf-8")
        return
    target = tmp / scenario.path
    if scenario.create:
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text("// created by ui_evidence_route_guard\n", encoding="utf-8")
    else:
        target.write_text(target.read_text(encoding="utf-8") + "// changed\n", encoding="utf-8")


def run_route(tmp: Path, base: str) -> tuple[int, str, str]:
    try:
        completed = subprocess.run(
            [
                "bash",
                ROUTE_REL,
                "--base",
                base,
                "--with-gui",
                "--require-capture",
            ],
            cwd=tmp,
            capture_output=True,
            text=True,
            timeout=120,
        )
    except (OSError, subprocess.TimeoutExpired) as exc:
        raise FixtureError(f"route invocation failed: {exc}") from exc
    return completed.returncode, completed.stdout, completed.stderr


def observed_verdict(rc: int, stdout: str, stderr: str) -> tuple[str, tuple[str, ...]]:
    if rc == 0 and "no UI boundary changes" in stdout:
        return "no-ui", ()
    if rc == 0 and "headless-only" in stdout:
        return "headless-only", ()
    match = re.search(r"missing:(.*?)\s*\(run", stdout + stderr, re.DOTALL)
    if match is not None:
        tokens = tuple(
            sorted(
                token[: -len("-capture")]
                for token in match.group(1).split()
                if token.endswith("-capture")
            )
        )
        return "capture", tokens
    raise FixtureError(f"unrecognised route verdict (rc={rc}): {stdout}{stderr}")


def evaluate(
    route_text: str, verbose: bool
) -> tuple[list[str], dict[str, tuple[str, tuple[str, ...]]]]:
    findings: list[str] = []
    observed: dict[str, tuple[str, tuple[str, ...]]] = {}
    with tempfile.TemporaryDirectory(prefix="ui-route-guard-") as tmp_dir:
        tmp = Path(tmp_dir)
        _, base = build_fixture(tmp, route_text)
        for scenario in SCENARIOS:
            apply_scenario(tmp, base, scenario)
            rc, stdout, stderr = run_route(tmp, base)
            verdict, missing = observed_verdict(rc, stdout, stderr)
            observed[scenario.name] = (verdict, missing)
            if verbose:
                print(f"--- {scenario.name} ({scenario.path}) rc={rc}")
                print((stdout + stderr).rstrip())
            if verdict != scenario.verdict or sorted(missing) != sorted(scenario.missing):
                findings.append(
                    f"{scenario.name}: expected {scenario.verdict} {tuple(sorted(scenario.missing))}, "
                    f"observed {verdict} {missing}"
                )
    return findings, observed


def run_live(route_path: Path, verbose: bool) -> int:
    if not route_path.is_file():
        print(f"ui-evidence-route-guard: missing {route_path}", file=sys.stderr)
        return 2
    findings, _ = evaluate(route_path.read_text(encoding="utf-8"), verbose)
    if findings:
        print("ui-evidence-route-guard: FAIL", file=sys.stderr)
        for finding in findings:
            print(f"  finding: {finding}", file=sys.stderr)
        return 1
    print(f"ui-evidence-route-guard: PASS ({len(SCENARIOS)} classifications pinned)")
    return 0


def run_self_test(route_path: Path) -> int:
    if not route_path.is_file():
        print(f"ui-evidence-route-guard: missing {route_path}", file=sys.stderr)
        return 2
    base_text = route_path.read_text(encoding="utf-8")
    if base_text.count(CASE_ANCHOR) != 1:
        print(
            "ui-evidence-route-guard: parse failure, case anchor is not unique in route",
            file=sys.stderr,
        )
        return 2
    failures: list[str] = []
    for label, mutation in SELF_TEST_MUTATIONS.items():
        mutated = base_text.replace(CASE_ANCHOR, CASE_ANCHOR + mutation, 1)
        try:
            findings, observed = evaluate(mutated, verbose=False)
        except FixtureError as exc:
            print(f"ui-evidence-route-guard: self-test {label} error: {exc}", file=sys.stderr)
            return 2
        seen = {finding.split(":", 1)[0] for finding in findings}
        expected = EXPECTED_SELF_TEST_FAILURES[label]
        if not expected <= seen:
            failures.append(
                f"{label}: guard did not flag {sorted(expected - seen)} "
                f"(observed {observed})"
            )
    if failures:
        for failure in failures:
            print(f"ui-evidence-route-guard: self-test FAIL: {failure}", file=sys.stderr)
        return 1
    print("ui-evidence-route-guard: self-test PASS (goes red on every route mutation)")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="prove the guard goes red on a re-broadened or weakened route",
    )
    parser.add_argument(
        "--route",
        default=str(REPO_ROOT / ROUTE_REL),
        help="route script under test (defaults to the live one)",
    )
    parser.add_argument("--verbose", action="store_true", help="print every route verdict")
    args = parser.parse_args(argv)

    route_path = Path(args.route).resolve()
    try:
        if args.self_test:
            return run_self_test(route_path)
        return run_live(route_path, args.verbose)
    except FixtureError as exc:
        print(f"ui-evidence-route-guard: parse failure: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
