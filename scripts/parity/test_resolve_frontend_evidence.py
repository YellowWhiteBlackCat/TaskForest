#!/usr/bin/env python3
"""Self-test for scripts/parity/resolve_frontend_evidence.py.

Pure standard library.  It builds synthetic manifests and synthetic nextest
discovery payloads and proves the resolver is not a rubber stamp:

* a legal, discoverable anchor passes;
* a dangling anchor (declared but not discovered) is reported and fails;
* `pending` is counted separately from `dangling`;
* deleting a referenced test (discovery no longer lists it) turns the run red;
* a target/frontend ownership mismatch is rejected;
* malformed declarations fail loudly instead of being skipped;
* the `--scope auto` diff-scope only skips when no evidence-relevant path
  changed, evaluates fail-closed when the git probe fails, and never treats a
  skipped diff as a resolved anchor set.

Run:  python3 scripts/parity/test_resolve_frontend_evidence.py
Exit: 0 when every check passes, 1 otherwise.
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
RESOLVER = HERE / "resolve_frontend_evidence.py"

_spec = importlib.util.spec_from_file_location("resolve_frontend_evidence", RESOLVER)
assert _spec and _spec.loader
resolver = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(resolver)

HEADER = "\t".join(resolver.MANIFEST_FIELDS)

CHECKS = 0
FAILURES: list[str] = []


def check(condition: bool, label: str) -> None:
    global CHECKS
    CHECKS += 1
    if condition:
        print(f"ok   - {label}")
    else:
        FAILURES.append(label)
        print(f"FAIL - {label}")


def manifest_row(
    subject_id: str,
    frontend: str,
    *,
    status: str = "ready",
    contract_tag: str = "keyboard",
    evidence_kind: str = "behavior",
    anchor: str = "-",
    target: str | None = None,
    platform: str = "",
) -> str:
    return "\t".join([
        "facet",
        subject_id,
        frontend,
        status,
        "",
        contract_tag,
        evidence_kind,
        anchor,
        target if target is not None else frontend,
        platform,
    ])


def write_manifest(path: Path, rows: list[str]) -> Path:
    path.write_text("\n".join([HEADER, *rows]) + "\n", encoding="utf-8")
    return path


def write_json_discovery(path: Path, names: list[str]) -> Path:
    payload = {
        "rust-suites": {
            "taskmanager-x": {
                "kind": "lib",
                "binary-name": "taskmanager_x",
                "testcases": {name: {"ignored": False} for name in names},
            }
        }
    }
    path.write_text(json.dumps(payload), encoding="utf-8")
    return path


def namespace(
    manifest: Path,
    discovery: dict[str, Path],
    repo: Path,
    *,
    scope: str = "all",
    base: str | None = None,
) -> argparse.Namespace:
    return argparse.Namespace(
        manifest=str(manifest),
        discovery=[f"{frontend}={path}" for frontend, path in discovery.items()],
        capture_scenarios=[],
        nextest=False,
        nextest_timeout=30,
        scope=scope,
        base=base,
        repo=str(repo),
        report=None,
        no_report=True,
        json=False,
    )


def run(tmp: Path, rows: list[str], discovery: dict[str, Path]):
    manifest = write_manifest(tmp / "manifest.tsv", rows)
    return resolver.resolve(namespace(manifest, discovery, tmp))


def check_error(rows: list[str], discovery: dict[str, Path], label: str) -> None:
    with tempfile.TemporaryDirectory() as raw:
        tmp = Path(raw)
        manifest = write_manifest(tmp / "manifest.tsv", rows)
        try:
            resolver.resolve(namespace(manifest, discovery, tmp))
        except resolver.ResolveError:
            check(True, label)
        else:
            check(False, label)


BASE_ROWS = [
    manifest_row("alpha", "gpui", anchor="suite::alpha_ok"),
    manifest_row("alpha", "iced", anchor="suite::beta_ok"),
    manifest_row("alpha", "tui", evidence_kind="pending", anchor="-", target="-"),
    manifest_row("alpha", "bevy", status="partial", anchor="suite::bevy_ok"),
]


def base_discovery(tmp: Path) -> dict[str, Path]:
    return {
        "gpui": write_json_discovery(tmp / "gpui.json", ["suite::alpha_ok"]),
        "iced": write_json_discovery(tmp / "iced.json", ["suite::beta_ok"]),
        "tui": write_json_discovery(tmp / "tui.json", ["suite::tui_ok"]),
        "bevy": write_json_discovery(tmp / "bevy.json", ["suite::bevy_ok"]),
    }


def test_legal_anchor_passes(tmp: Path) -> None:
    report = run(tmp, BASE_ROWS, base_discovery(tmp))
    check(report["status"] == "pass", "consistent manifest passes")
    check(report["counts"]["anchored_behavior"] == 3, "three behavior anchors counted")
    check(report["counts"]["pending"] == 1, "one pending cell counted")
    check(report["counts"]["dangling"] == 0, "no dangling anchors")
    check(report["counts"]["invalid"] == 0, "no invalid cells")


def test_dangling_anchor_fails(tmp: Path) -> None:
    discovery = base_discovery(tmp)
    # Simulate deleting the referenced iced test.
    write_json_discovery(discovery["iced"], ["suite::unrelated"])
    report = run(tmp, BASE_ROWS, discovery)
    check(report["status"] == "fail", "deleting a referenced test fails the run")
    check(report["counts"]["dangling"] == 1, "deleted anchor is dangling")
    check(
        report["dangling"][0]["test_id"] == "suite::beta_ok"
        and report["dangling"][0]["frontend"] == "iced",
        "dangling report names the subject/frontend/test",
    )
    # pending must be unaffected by dangling
    check(report["counts"]["pending"] == 1, "pending stays separate from dangling")


def test_ownership_mismatch(tmp: Path) -> None:
    rows = [
        manifest_row("alpha", "gpui", anchor="suite::alpha_ok"),
        manifest_row("alpha", "iced", anchor="suite::beta_ok", target="tui"),
        manifest_row("alpha", "tui", evidence_kind="pending", anchor="-", target="-"),
        manifest_row("alpha", "bevy", anchor="suite::bevy_ok"),
    ]
    report = run(tmp, rows, base_discovery(tmp))
    check(report["status"] == "fail", "target/frontend mismatch fails")
    check(report["counts"]["invalid"] == 1, "ownership mismatch counted as invalid")
    check(report["counts"]["dangling"] == 0, "ownership mismatch is not dangling")


def test_duplicate_cell(tmp: Path) -> None:
    rows = BASE_ROWS + [manifest_row("alpha", "gpui", anchor="suite::alpha_ok")]
    report = run(tmp, rows, base_discovery(tmp))
    check(report["status"] == "fail", "duplicate (subject, frontend) cell fails")
    check(report["counts"]["invalid"] == 1, "duplicate cell counted as invalid")


def test_plain_text_discovery(tmp: Path) -> None:
    text = tmp / "gpui-list.txt"
    text.write_text(
        "taskmanager-gpui suite::alpha_ok\n"
        "taskmanager-gpui suite::extra\n",
        encoding="utf-8",
    )
    discovery = base_discovery(tmp) | {"gpui": text}
    report = run(tmp, BASE_ROWS, discovery)
    check(report["status"] == "pass", "plain --list text discovery is accepted")
    check(report["frontends"]["gpui"]["discovered_tests"] == 2, "plain text names parsed")


def test_malformed_manifest_fails_loudly(tmp: Path) -> None:
    discovery = base_discovery(tmp)
    # Contract-tag membership is owned by the Rust `ContractTag` authority and
    # its conformance test; the resolver only requires the field to be present.
    check_error(
        [manifest_row("alpha", "gpui", anchor="suite::alpha_ok", contract_tag="")],
        discovery,
        "empty contract_tag is rejected",
    )
    check_error(
        [manifest_row("alpha", "gpui", anchor="suite::alpha_ok", evidence_kind="guess")],
        discovery,
        "unknown evidence_kind is rejected",
    )
    check_error(
        [manifest_row("alpha", "gpui", anchor="suite::alpha_ok", status="sometimes")],
        discovery,
        "unknown status is rejected",
    )
    check_error(
        [manifest_row("alpha", "gpui", anchor="")],
        discovery,
        "empty anchor field is rejected",
    )


def test_missing_discovery_errors(tmp: Path) -> None:
    manifest = write_manifest(tmp / "manifest.tsv", BASE_ROWS)
    partial = {"gpui": write_json_discovery(tmp / "gpui.json", ["suite::alpha_ok"])}
    try:
        resolver.resolve(namespace(manifest, partial, tmp))
    except resolver.ResolveError:
        check(True, "missing frontend discovery is a fatal usage error")
    else:
        check(False, "missing frontend discovery is a fatal usage error")


# The committed evidence surface: every one of these can move a declared
# anchor, so `--scope auto` must evaluate.  Keep this list aligned with
# EVIDENCE_SCOPE_PATTERNS (the test is the conscious-change guard).
SCOPE_IN_PATH = [
    "scripts/parity/cross_frontend_manifest.tsv",
    "scripts/parity/resolve_frontend_evidence.py",
    "crates/taskmanager-ui-contract/src/conformance.rs",
    "crates/taskmanager-ui-contract/src/feature_coverage.rs",
    "crates/taskmanager-ui-contract/src/feature_coverage/semantic_spec.rs",
    "crates/taskmanager-ui-contract/tests/headless/ui_conformance.rs",
    "crates/taskmanager-gpui/src/gpui_app/feature_coverage.rs",
    "crates/taskmanager-iced/src/feature_coverage.rs",
    "crates/taskmanager-iced/tests/gui/feature_coverage_tests.rs",
    "crates/taskmanager-tui/src/feature_coverage.rs",
    "crates/taskmanager-bevy-ui/tests/headless/feature_coverage.rs",
]

SCOPE_OUT_PATH = [
    "crates/taskmanager-core/src/lib.rs",
    "crates/taskmanager-gpui/src/gpui_app/root.rs",
    "docs/QUALITY_GATES.md",
    "tests/logic/four_frontend_parity_ledger_test.rs",
    "scripts/accept-iced-interactions.sh",
]


def test_scope_patterns() -> None:
    missed = [path for path in SCOPE_IN_PATH if not resolver.evidence_relevant(path)]
    check(not missed, f"evidence-relevant paths are in scope (missed: {missed})")
    extra = [path for path in SCOPE_OUT_PATH if resolver.evidence_relevant(path)]
    check(not extra, f"unrelated paths stay out of scope (matched: {extra})")
    check(
        resolver.evidence_relevant("./scripts/parity/README.md"),
        "a leading ./ is normalised away",
    )

    all_decision = resolver.evaluate_scope([], "all", None)
    check(
        all_decision.relevant and all_decision.mode == "all",
        "--scope all always evaluates",
    )

    out = resolver.evaluate_scope(["docs/QUALITY_GATES.md"], "auto", "BASE")
    check(not out.relevant, "a docs-only diff is out of scope")
    check(out.base == "BASE", "the scope decision records its base")
    check(out.matched_paths == (), "no matched paths for a docs-only diff")

    inside = resolver.evaluate_scope(
        ["scripts/parity/cross_frontend_manifest.tsv", "docs/README.md"],
        "auto",
        "BASE",
    )
    check(inside.relevant, "a manifest change is in scope")
    check(
        inside.matched_paths == ("scripts/parity/cross_frontend_manifest.tsv",),
        "only evidence-relevant paths are matched",
    )

    broken = resolver.evaluate_scope([], "auto", "BASE", "git unavailable")
    check(broken.relevant, "a failed diff probe evaluates fail-closed")
    check("fail-closed" in broken.reason, "the fail-closed reason is explicit")


def with_git_paths(paths: list[str], error: str | None, body) -> None:
    """Run `body` with `git_changed_paths` stubbed out (restores after)."""
    original = resolver.git_changed_paths
    resolver.git_changed_paths = lambda repo, base: (paths, error)
    try:
        body()
    finally:
        resolver.git_changed_paths = original


def test_auto_scope_short_circuit(tmp: Path) -> None:
    manifest = write_manifest(tmp / "manifest.tsv", BASE_ROWS)

    def body() -> None:
        report = resolver.resolve(
            namespace(manifest, {}, tmp, scope="auto", base="BASE")
        )
        check(report["status"] == "pass", "an out-of-scope run passes")
        check(report["skipped"] is True, "an out-of-scope run is marked skipped")
        check(
            report["counts"]["cells"] == 0 and report["frontends"] == {},
            "an out-of-scope run resolves nothing and discovers nothing",
        )
        check(
            report["scope"]["relevant"] is False
            and report["scope"]["base"] == "BASE",
            "an out-of-scope run records the scope decision",
        )
        # The report still parses as the resolver's schema.
        check(
            "no evidence-relevant change" in report["scope"]["reason"],
            "the skip reason names the scope",
        )

    with_git_paths([], None, body)


def test_auto_scope_relevant_runs(tmp: Path) -> None:
    manifest = write_manifest(tmp / "manifest.tsv", BASE_ROWS)
    discovery = base_discovery(tmp)

    def body() -> None:
        report = resolver.resolve(
            namespace(manifest, discovery, tmp, scope="auto", base="BASE")
        )
        check(report["skipped"] is False, "an in-scope run is not skipped")
        check(report["scope"]["relevant"] is True, "an in-scope run records scope")
        check(
            report["scope"]["matched_paths"]
            == ["scripts/parity/cross_frontend_manifest.tsv"],
            "matched paths reach the report",
        )
        check(report["counts"]["anchored_behavior"] == 3, "anchors are evaluated")

    with_git_paths(["scripts/parity/cross_frontend_manifest.tsv"], None, body)

    def failing_probe() -> None:
        report = resolver.resolve(
            namespace(manifest, discovery, tmp, scope="auto", base="BASE")
        )
        check(report["skipped"] is False, "a failed diff probe still evaluates")
        check(
            "fail-closed" in report["scope"]["reason"],
            "a failed diff probe is reported as fail-closed",
        )

    with_git_paths([], "git exploded", failing_probe)


def test_platform_column_is_reserved(tmp: Path) -> None:
    discovery = base_discovery(tmp)
    reserved = run(
        tmp,
        [manifest_row("alpha", "gpui", anchor="suite::alpha_ok")],
        discovery,
    )
    check(
        reserved["status"] == "pass" and reserved["counts"]["cells"] == 1,
        "an empty platform field is the frontend axis and passes",
    )
    # The field is opaque here: a future P5 vocabulary is not this resolver's
    # to validate (like contract_tag, its authority lands with the axis).
    opaque = run(
        tmp,
        [manifest_row("alpha", "gpui", anchor="suite::alpha_ok", platform="linux")],
        discovery,
    )
    check(opaque["status"] == "pass", "a filled platform field stays opaque")
    check_error(
        [manifest_row("alpha", "gpui", anchor="suite::alpha_ok").rsplit("\t", 1)[0]],
        discovery,
        "a manifest row without the reserved platform column fails loudly",
    )


def main() -> int:
    with tempfile.TemporaryDirectory() as raw:
        tmp = Path(raw)
        test_legal_anchor_passes(tmp)
        test_dangling_anchor_fails(tmp)
        test_ownership_mismatch(tmp)
        test_duplicate_cell(tmp)
        test_plain_text_discovery(tmp)
        test_malformed_manifest_fails_loudly(tmp)
        test_missing_discovery_errors(tmp)
        test_scope_patterns()
        test_auto_scope_short_circuit(tmp)
        test_auto_scope_relevant_runs(tmp)
        test_platform_column_is_reserved(tmp)

    if FAILURES:
        print(f"\nself-test: FAIL ({len(FAILURES)}/{CHECKS} checks failed)")
        for failure in FAILURES:
            print(f"  - {failure}")
        return 1
    print(f"\nself-test: PASS ({CHECKS}/{CHECKS} checks)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
