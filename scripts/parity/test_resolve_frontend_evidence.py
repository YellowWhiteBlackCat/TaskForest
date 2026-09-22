#!/usr/bin/env python3
"""Self-test for scripts/parity/resolve_frontend_evidence.py.

Pure standard library.  It builds synthetic manifests and synthetic nextest
discovery payloads and proves the resolver is not a rubber stamp:

* a legal, discoverable anchor passes;
* a dangling anchor (declared but not discovered) is reported and fails;
* `pending` is counted separately from `dangling`;
* deleting a referenced test (discovery no longer lists it) turns the run red;
* a target/frontend ownership mismatch is rejected;
* malformed declarations fail loudly instead of being skipped.

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


def namespace(manifest: Path, discovery: dict[str, Path], repo: Path) -> argparse.Namespace:
    return argparse.Namespace(
        manifest=str(manifest),
        discovery=[f"{frontend}={path}" for frontend, path in discovery.items()],
        capture_scenarios=[],
        nextest=False,
        nextest_timeout=30,
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

    if FAILURES:
        print(f"\nself-test: FAIL ({len(FAILURES)}/{CHECKS} checks failed)")
        for failure in FAILURES:
            print(f"  - {failure}")
        return 1
    print(f"\nself-test: PASS ({CHECKS}/{CHECKS} checks)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
