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
* the unified interaction matrix is consumed as a second declaration source:
  exact `test_name` anchors and GPUI's stable case-prefix channel both resolve
  against discovery, deleting either anchor is `dangling`, a TUI/iced/bevy
  explicit anchor reports the owning frontend, requirement id and case id, and
  malformed matrix rows are rejected;
* `test_name=pending` matrix rows are counted and reported, never dangling, and
  can never cover a requirement;
* the optional requirement authority rejects an unknown `p0_id`, reports
  per-frontend `P0-MC-*` coverage, and `--require-requirement-coverage` fails
  closed on an uncovered `(frontend, requirement)` cell;
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
INTERACTION_HEADER = "\t".join(resolver.INTERACTION_FIELDS)

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


def interaction_row(
    case_id: str,
    frontend: str,
    *,
    p0_id: str = "P0-MC-00",
    target: str = "lib",
    test_name: str = "-",
    paths: str = "success",
    capture_scenarios: str = "-",
    contract_tag: str | None = None,
    subject_kind: str = "interaction",
    platform: str = "",
) -> str:
    """One unified-matrix row; contract_tag defaults to the first paths token."""
    return "\t".join([
        subject_kind,
        case_id,
        frontend,
        p0_id,
        target,
        test_name,
        paths,
        capture_scenarios,
        contract_tag if contract_tag is not None else paths.split("|", 1)[0],
        platform,
    ])


def write_interaction_matrix(path: Path, rows: list[str]) -> Path:
    path.write_text("\n".join([INTERACTION_HEADER, *rows]) + "\n", encoding="utf-8")
    return path


def write_requirements(path: Path, ids: list[str]) -> Path:
    path.write_text(
        "\n".join(["requirement_id", *ids]) + "\n", encoding="utf-8"
    )
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
    interaction_matrix: Path | None = None,
    requirements: Path | None = None,
    require_requirement_coverage: bool = False,
) -> argparse.Namespace:
    return argparse.Namespace(
        manifest=str(manifest),
        interaction_matrix=str(interaction_matrix) if interaction_matrix else None,
        discovery=[f"{frontend}={path}" for frontend, path in discovery.items()],
        capture_scenarios=[],
        requirements=str(requirements) if requirements else None,
        require_requirement_coverage=require_requirement_coverage,
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


INTERACTION_GPUI_TEST = "gpui_behavior::nav_chrome::mc00_page_sweep_case_strip_renders_tabs"
INTERACTION_ICED_TEST = "ui::tests::pages::page_sweep_covers_pages"
INTERACTION_BEVY_TEST = "pages::process_tree::tests::collapse_hides_descendants"
INTERACTION_TUI_TEST = "ui::tests::pages::tui_page_sweep_covers_pages"

INTERACTION_ROWS = [
    interaction_row("mc00-page-sweep", "gpui", target="gui", paths="success|responsive"),
    interaction_row(
        "mc00-nav-keyboard",
        "iced",
        test_name=INTERACTION_ICED_TEST,
        paths="success|keyboard",
    ),
    interaction_row(
        "bev-tree-collapse",
        "bevy",
        p0_id="-",
        test_name=INTERACTION_BEVY_TEST,
        paths="keyboard|lifecycle|success",
    ),
    interaction_row(
        "mc00-tui-page-sweep",
        "tui",
        test_name=INTERACTION_TUI_TEST,
        paths="success|responsive",
    ),
    interaction_row("mc05-tui-chart-hover", "tui", test_name="pending", paths="pointer"),
]


def interaction_discovery(tmp: Path) -> dict[str, Path]:
    """Discovery carrying both the facet anchors and the interaction anchors."""
    return {
        "gpui": write_json_discovery(
            tmp / "gpui.json", ["suite::alpha_ok", INTERACTION_GPUI_TEST]
        ),
        "iced": write_json_discovery(
            tmp / "iced.json", ["suite::beta_ok", INTERACTION_ICED_TEST]
        ),
        "tui": write_json_discovery(
            tmp / "tui.json", ["suite::tui_ok", INTERACTION_TUI_TEST]
        ),
        "bevy": write_json_discovery(
            tmp / "bevy.json", ["suite::bevy_ok", INTERACTION_BEVY_TEST]
        ),
    }


def run_interaction(
    tmp: Path,
    rows: list[str],
    discovery: dict[str, Path],
    *,
    manifest_rows: list[str] | None = None,
    requirements: Path | None = None,
    require_requirement_coverage: bool = False,
):
    manifest = write_manifest(tmp / "manifest.tsv", manifest_rows or BASE_ROWS)
    matrix = write_interaction_matrix(tmp / "interaction.tsv", rows)
    return resolver.resolve(
        namespace(
            manifest,
            discovery,
            tmp,
            interaction_matrix=matrix,
            requirements=requirements,
            require_requirement_coverage=require_requirement_coverage,
        )
    )


def check_interaction_error(rows: list[str], label: str) -> None:
    with tempfile.TemporaryDirectory() as raw:
        tmp = Path(raw)
        manifest = write_manifest(tmp / "manifest.tsv", BASE_ROWS)
        matrix = write_interaction_matrix(tmp / "interaction.tsv", rows)
        try:
            resolver.resolve(
                namespace(
                    manifest, base_discovery(tmp), tmp, interaction_matrix=matrix
                )
            )
        except resolver.ResolveError:
            check(True, label)
        else:
            check(False, label)


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
    "scripts/parity/cross_frontend_matrix.tsv",
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
            report["counts"]["interaction_cells"] == 0
            and report["interaction_matrix"]["cases"] == 0,
            "an out-of-scope run carries the empty interaction schema",
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


def test_interaction_matrix_resolves(tmp: Path) -> None:
    report = run_interaction(tmp, INTERACTION_ROWS, interaction_discovery(tmp))
    check(report["status"] == "pass", "a consistent interaction matrix passes")
    check(report["counts"]["interaction_cells"] == 5, "interaction cells counted")
    check(report["counts"]["interaction_anchored"] == 4, "interaction anchors counted")
    check(
        report["counts"]["interaction_pending"] == 1,
        "pending interaction rows are counted separately",
    )
    check(report["counts"]["interaction_dangling"] == 0, "no interaction dangling")
    check(report["counts"]["dangling"] == 0, "interaction cells add no dangling")
    check(
        report["counts"]["cells"] == 4 and report["counts"]["pending"] == 2,
        "facet grid counts stay independent of the interaction matrix "
        "(one facet + one interaction pending cell)",
    )
    check(
        report["interaction_matrix"]["cases"] == 5
        and report["interaction_matrix"]["path"].endswith("interaction.tsv"),
        "the report records the interaction matrix source",
    )
    check(
        report["interaction_matrix"]["anchored"] == 4
        and report["interaction_matrix"]["pending"] == 1
        and report["interaction_matrix"]["dangling"] == 0,
        "the interaction block mirrors anchored/pending/dangling counts",
    )
    check(
        report["requirement_coverage"] is None,
        "without a requirement authority the coverage block stays null",
    )


def test_interaction_pending_rows() -> None:
    with tempfile.TemporaryDirectory() as raw:
        tmp = Path(raw)
        discovery = interaction_discovery(tmp)
        # The pending row must not depend on the frontend's discovery at all.
        write_json_discovery(discovery["tui"], ["suite::tui_ok"])
        report = run_interaction(tmp, INTERACTION_ROWS, discovery)
        check(
            report["status"] == "fail",
            "deleting the anchored TUI test fails while the pending row stays inert",
        )
        pending = [
            cell
            for cell in report["pending"]
            if cell["subject_kind"] == "interaction"
        ]
        check(
            pending == [
                {
                    "subject_kind": "interaction",
                    "subject_id": "mc05-tui-chart-hover",
                    "frontend": "tui",
                }
            ],
            "a pending matrix row lands in the shared pending list",
        )
        check(
            report["counts"]["interaction_dangling"] == 1
            and report["counts"]["interaction_anchored"] == 4,
            "a pending row is neither anchored nor dangling",
        )


def test_interaction_tui_anchor_dangling_report(tmp: Path) -> None:
    discovery = interaction_discovery(tmp)
    # Simulate renaming/deleting the anchored TUI test.
    write_json_discovery(discovery["tui"], ["suite::tui_ok"])
    report = run_interaction(tmp, INTERACTION_ROWS, discovery)
    by_subject = {item["subject_id"]: item for item in report["dangling"]}
    item = by_subject["mc00-tui-page-sweep"]
    check(
        item["frontend"] == "tui"
        and item["test_id"] == INTERACTION_TUI_TEST
        and item["channel"] == "test-name"
        and item["p0_id"] == "P0-MC-00"
        and item["subject_kind"] == "interaction",
        "a dangling TUI anchor names frontend, requirement, case and test id",
    )


def test_requirement_coverage_report(tmp: Path) -> None:
    requirements = write_requirements(tmp / "requirements.tsv", ["P0-MC-00", "P0-MC-01"])
    rows = [
        interaction_row(
            "mc00-nav-keyboard", "iced", test_name=INTERACTION_ICED_TEST, paths="success"
        ),
        interaction_row(
            "bev-tree-collapse",
            "bevy",
            p0_id="-",
            test_name=INTERACTION_BEVY_TEST,
            paths="success",
        ),
        interaction_row("mc05-tui-chart-hover", "tui", test_name="pending", paths="success"),
    ]
    report = run_interaction(
        tmp, rows, interaction_discovery(tmp), requirements=requirements
    )
    check(report["status"] == "pass", "a coverage report alone does not fail a run")
    coverage = report["requirement_coverage"]
    check(
        coverage is not None and coverage["requirements"] == ["P0-MC-00", "P0-MC-01"],
        "the coverage block names the requirement authority's ids",
    )
    check(
        coverage["frontends"]["iced"]["covered"] == ["P0-MC-00"]
        and coverage["frontends"]["iced"]["missing"] == ["P0-MC-01"],
        "an anchored success row covers its requirement on its own frontend",
    )
    check(
        coverage["frontends"]["bevy"]["covered"] == []
        and coverage["frontends"]["bevy"]["unmapped_cells"] == 1,
        "an unmapped cell never covers a requirement",
    )
    check(
        coverage["frontends"]["tui"]["covered"] == []
        and coverage["frontends"]["tui"]["pending_cells"] == 1,
        "a pending cell never covers a requirement",
    )
    check(
        coverage["frontends"]["gpui"]["missing"] == ["P0-MC-00", "P0-MC-01"],
        "a frontend with no interaction rows misses every requirement",
    )


def test_require_requirement_coverage_fails_closed(tmp: Path) -> None:
    requirements = write_requirements(tmp / "requirements.tsv", ["P0-MC-00", "P0-MC-01"])
    rows = [
        interaction_row(
            "mc00-nav-keyboard", "iced", test_name=INTERACTION_ICED_TEST, paths="success"
        ),
    ]
    report = run_interaction(
        tmp,
        rows,
        interaction_discovery(tmp),
        requirements=requirements,
        require_requirement_coverage=True,
    )
    check(
        report["status"] == "fail",
        "--require-requirement-coverage fails on an uncovered (frontend, requirement) cell",
    )
    pairs = {
        (item["frontend"], item["subject_id"])
        for item in report["invalid"]
        if item.get("reason", "").startswith("requirement has no success-path")
    }
    check(
        ("bevy", "P0-MC-01") in pairs and ("tui", "P0-MC-00") in pairs,
        "every uncovered pair is reported by frontend and requirement id",
    )
    check(
        ("iced", "P0-MC-01") in pairs and ("iced", "P0-MC-00") not in pairs,
        "the covered cell is not reported",
    )


def test_requirement_authority_rejects_unknown_id(tmp: Path) -> None:
    requirements = write_requirements(tmp / "requirements.tsv", ["P0-MC-00"])
    rows = [
        interaction_row(
            "mc99-unknown", "iced", test_name=INTERACTION_ICED_TEST, p0_id="P0-MC-99"
        ),
    ]
    report = run_interaction(
        tmp,
        rows,
        interaction_discovery(tmp),
        requirements=requirements,
        require_requirement_coverage=True,
    )
    check(
        report["status"] == "fail",
        "an unknown p0_id fails once the requirement authority is supplied",
    )
    check(
        any(
            item.get("reason", "").startswith("unknown requirement id")
            and item["subject_id"] == "P0-MC-99"
            for item in report["invalid"]
        ),
        "the unknown requirement id is reported precisely",
    )
    with tempfile.TemporaryDirectory() as raw:
        missing = Path(raw) / "absent.tsv"
        try:
            run_interaction(
                tmp,
                rows,
                interaction_discovery(tmp),
                requirements=missing,
            )
        except resolver.ResolveError:
            check(True, "a missing requirement authority is a fatal usage error")
        else:
            check(False, "a missing requirement authority is a fatal usage error")


def test_interaction_matrix_dangling(tmp: Path) -> None:
    discovery = interaction_discovery(tmp)
    # Deleting the referenced iced test turns the exact-anchor cell dangling.
    write_json_discovery(discovery["iced"], ["suite::beta_ok"])
    # Deleting the GPUI case-prefix test turns the prefix-channel cell dangling.
    write_json_discovery(discovery["gpui"], ["suite::alpha_ok"])
    report = run_interaction(tmp, INTERACTION_ROWS, discovery)
    check(report["status"] == "fail", "deleting an interaction anchor fails the run")
    check(
        report["counts"]["interaction_dangling"] == 2,
        "both interaction channels report dangling",
    )
    by_subject = {item["subject_id"]: item for item in report["dangling"]}
    check(
        by_subject["mc00-nav-keyboard"]["test_id"] == INTERACTION_ICED_TEST
        and by_subject["mc00-nav-keyboard"]["frontend"] == "iced"
        and by_subject["mc00-nav-keyboard"]["subject_kind"] == "interaction",
        "the exact-anchor cell reports its intercepted test id and frontend",
    )
    check(
        by_subject["mc00-page-sweep"]["channel"] == "case-prefix"
        and by_subject["mc00-page-sweep"]["test_id"] == "mc00_page_sweep_case_*",
        "the GPUI row reports the stable case-prefix channel",
    )
    check(
        report["counts"]["dangling"] == 2,
        "interaction dangling feeds the shared dangling count",
    )
    check(
        report["counts"]["interaction_anchored"] == 4,
        "dangling cells are still counted as anchored declarations",
    )


def test_interaction_matrix_malformed() -> None:
    check_interaction_error(
        [interaction_row("case-a", "gpui", subject_kind="facet")],
        "unknown interaction subject_kind is rejected",
    )
    check_interaction_error(
        [interaction_row("case-a", "gtk")],
        "unknown interaction frontend is rejected",
    )
    check_interaction_error(
        [interaction_row("case-a", "gpui", target="bin")],
        "unknown interaction target is rejected",
    )
    check_interaction_error(
        [interaction_row("case-a", "iced")],
        "an implicit anchor channel is rejected for a frontend without one",
    )
    check_interaction_error(
        [interaction_row("case-a", "tui")],
        "a TUI row without a test_name is rejected (TUI has no prefix channel)",
    )
    check_interaction_error(
        [interaction_row("case-a", "bevy", test_name="pending", contract_tag="failure")],
        "a pending row still has to declare contract_tag = first paths token",
    )
    check_interaction_error(
        [
            interaction_row(
                "case-a", "gpui", paths="success|failure", contract_tag="failure"
            )
        ],
        "contract_tag must be the first paths token",
    )
    check_interaction_error(
        [interaction_row("case-a", "gpui"), interaction_row("case-a", "gpui")],
        "duplicate (frontend, case_id) matrix cells are rejected",
    )
    check_interaction_error(
        [interaction_row("case-a", "gpui", paths="")],
        "an empty paths declaration is rejected",
    )
    with tempfile.TemporaryDirectory() as raw:
        tmp = Path(raw)
        manifest = write_manifest(tmp / "manifest.tsv", BASE_ROWS)
        discovery = base_discovery(tmp)
        try:
            resolver.resolve(
                namespace(
                    manifest, discovery, tmp, interaction_matrix=tmp / "missing.tsv"
                )
            )
        except resolver.ResolveError:
            check(True, "a missing interaction matrix is a fatal usage error")
        else:
            check(False, "a missing interaction matrix is a fatal usage error")

        bad = tmp / "bad.tsv"
        bad.write_text("case_id\tfrontend\ttest_name\ncase-a\tgpui\t-\n", encoding="utf-8")
        try:
            resolver.resolve(
                namespace(manifest, discovery, tmp, interaction_matrix=bad)
            )
        except resolver.ResolveError:
            check(True, "an unexpected interaction matrix header is rejected")
        else:
            check(False, "an unexpected interaction matrix header is rejected")


def test_interaction_frontend_needs_discovery(tmp: Path) -> None:
    manifest = write_manifest(
        tmp / "manifest.tsv",
        [manifest_row("alpha", "gpui", anchor="suite::alpha_ok")],
    )
    matrix = write_interaction_matrix(
        tmp / "interaction.tsv",
        [interaction_row("case-a", "iced", test_name="suite::beta_ok")],
    )
    discovery = {"gpui": write_json_discovery(tmp / "gpui.json", ["suite::alpha_ok"])}
    try:
        resolver.resolve(
            namespace(manifest, discovery, tmp, interaction_matrix=matrix)
        )
    except resolver.ResolveError:
        check(True, "an interaction-only frontend still requires discovery")
    else:
        check(False, "an interaction-only frontend still requires discovery")


def test_interaction_matrix_absent_leaves_report_unchanged(tmp: Path) -> None:
    report = run(tmp, BASE_ROWS, base_discovery(tmp))
    check(
        report["interaction_matrix"]
        == {"path": None, "cases": 0, "anchored": 0, "pending": 0, "dangling": 0},
        "without --interaction-matrix the report stays interaction-empty",
    )
    check(
        report["counts"]["interaction_cells"] == 0
        and report["counts"]["interaction_pending"] == 0
        and report["counts"]["interaction_dangling"] == 0,
        "without --interaction-matrix no interaction count moves",
    )
    check(
        report["requirement_coverage"] is None,
        "without a requirement authority there is no coverage block",
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
        test_interaction_matrix_resolves(tmp)
        test_interaction_pending_rows()
        test_interaction_matrix_dangling(tmp)
        test_interaction_tui_anchor_dangling_report(tmp)
        test_interaction_matrix_malformed()
        test_interaction_frontend_needs_discovery(tmp)
        test_requirement_coverage_report(tmp)
        test_require_requirement_coverage_fails_closed(tmp)
        test_requirement_authority_rejects_unknown_id(tmp)
        test_interaction_matrix_absent_leaves_report_unchanged(tmp)

    if FAILURES:
        print(f"\nself-test: FAIL ({len(FAILURES)}/{CHECKS} checks failed)")
        for failure in FAILURES:
            print(f"  - {failure}")
        return 1
    print(f"\nself-test: PASS ({CHECKS}/{CHECKS} checks)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
