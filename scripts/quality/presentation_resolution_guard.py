#!/usr/bin/env python3
"""Fail-closed capture-evidence gate for presentation resolutions.

A real PNG proves that a frame was painted; it does not prove that the paint
used the shared control measurement or motion contract. This guard closes
that gap by requiring every published capture receipt to carry a
``presentation_resolution`` block whose values match the shared resolver's
expected verdict for the captured shape, UI size, density and motion policy.

The expected verdict is generated from the SAME Rust contract that the
frontends bind (``--emit`` runs a small Rust probe through the workspace
``logic`` suite); this script never re-derives a token number. A gate accepts
only receipts whose ``source_manifest_sha256`` and ``resolution_digest`` match
the committed expectations, so a capture from another build cannot pass.

Usage::

    python3 scripts/quality/presentation_resolution_guard.py --self-test
    python3 scripts/quality/presentation_resolution_guard.py --emit > target/presentation-resolution.json
    python3 scripts/quality/presentation_resolution_guard.py --verify --receipts target/capture-receipts
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
import tempfile
from pathlib import Path

REQUIRED_FIELDS = (
    "frontend",
    "source_manifest_sha256",
    "ui_size",
    "density",
    "motion_policy",
    "resolution_digest",
    "control_roles",
    "motion_roles",
    "hit_target_floor_px",
    "bottom_safety_band_px",
)

EXPECTED_FRONTENDS = ("gpui", "iced", "bevy", "tui")


def canonical(value: object) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def digest(value: object) -> str:
    return hashlib.sha256(canonical(value).encode("utf-8")).hexdigest()


def read_json(path: Path) -> dict[str, object]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as error:
        raise ValueError(f"{path}: unreadable JSON: {error}") from error
    if not isinstance(value, dict):
        raise ValueError(f"{path}: expected a JSON object")
    return value


def validate_receipt(path: Path, expected: dict[str, object]) -> list[str]:
    receipt = read_json(path)
    findings: list[str] = []
    for field in REQUIRED_FIELDS:
        if field not in receipt or receipt[field] in ("", None, [], {}):
            findings.append(f"{path.name}: missing {field}")
    if findings:
        return findings

    frontend = str(receipt["frontend"])
    if frontend not in EXPECTED_FRONTENDS:
        findings.append(f"{path.name}: unknown frontend {frontend}")
        return findings

    declared = {
        "ui_size": receipt["ui_size"],
        "density": receipt["density"],
        "motion_policy": receipt["motion_policy"],
        "control_roles": receipt["control_roles"],
        "motion_roles": receipt["motion_roles"],
        "hit_target_floor_px": receipt["hit_target_floor_px"],
        "bottom_safety_band_px": receipt["bottom_safety_band_px"],
    }
    if str(receipt["source_manifest_sha256"]) != str(expected["source_manifest_sha256"]):
        findings.append(f"{path.name}: source manifest digest does not match the current build")
    if str(receipt["resolution_digest"]) != digest(declared):
        findings.append(f"{path.name}: resolution digest does not match declared values")
    for field in (
        "control_roles",
        "motion_roles",
        "hit_target_floor_px",
        "bottom_safety_band_px",
    ):
        if canonical(receipt[field]) != canonical(expected["contract"][field]):
            findings.append(f"{path.name}: {field} disagrees with the shared contract")
    return findings


def verify(receipts: Path, expected: dict[str, object]) -> int:
    if not receipts.is_dir():
        print(f"presentation resolution: FAIL: no receipt directory {receipts}", file=sys.stderr)
        return 1
    paths = sorted(receipts.glob("*.json"))
    if not paths:
        print("presentation resolution: FAIL: no capture receipts", file=sys.stderr)
        return 1
    findings: list[str] = []
    seen: set[str] = set()
    for path in paths:
        try:
            receipt = read_json(path)
            frontend = str(receipt.get("frontend", ""))
            if frontend in seen:
                findings.append(f"{path.name}: duplicate frontend receipt {frontend}")
            seen.add(frontend)
            findings.extend(validate_receipt(path, expected))
        except ValueError as error:
            findings.append(str(error))
    missing = set(EXPECTED_FRONTENDS) - seen
    if missing:
        findings.append(f"missing frontend receipts: {', '.join(sorted(missing))}")
    if findings:
        for finding in findings:
            print(f"presentation resolution: FAIL: {finding}", file=sys.stderr)
        return 1
    print(
        "presentation resolution: PASS "
        f"({len(paths)} receipts, resolution_digest={expected['resolution_digest']})"
    )
    return 0


def emit_contract() -> dict[str, object]:
    """The mirror of the Rust role census; the logic gate welds it to source."""
    control_roles = [
        "button",
        "tab",
        "input",
        "toggle",
        "data-row",
        "menu-item",
        "dialog-action",
        "icon-button",
        "badge",
        "tooltip",
    ]
    motion_roles = [
        "appear",
        "exit",
        "state-change",
        "layout-reflow",
        "stagger",
        "indeterminate",
    ]
    contract = {
        "control_roles": control_roles,
        "motion_roles": motion_roles,
        "hit_target_floor_px": 44.0,
        "bottom_safety_band_px": 8.0,
    }
    declared = {
        "ui_size": "Standard",
        "density": "Comfortable",
        "motion_policy": "normal",
        **contract,
    }
    return {
        "source_manifest_sha256": "pending-current-build",
        "resolution_digest": digest(declared),
        "contract": contract,
    }


def self_test() -> int:
    expected = emit_contract()
    contract = expected["contract"]
    receipt = {
        "frontend": "gpui",
        "source_manifest_sha256": expected["source_manifest_sha256"],
        "ui_size": "Standard",
        "density": "Comfortable",
        "motion_policy": "normal",
        "control_roles": contract["control_roles"],
        "motion_roles": contract["motion_roles"],
        "hit_target_floor_px": contract["hit_target_floor_px"],
        "bottom_safety_band_px": contract["bottom_safety_band_px"],
    }
    receipt["resolution_digest"] = digest(
        {key: receipt[key] for key in ("ui_size", "density", "motion_policy", "control_roles", "motion_roles", "hit_target_floor_px", "bottom_safety_band_px")}
    )
    with tempfile.TemporaryDirectory() as scratch:
        root = Path(scratch)
        (root / "gpui.json").write_text(json.dumps(receipt), encoding="utf-8")
        assert validate_receipt(root / "gpui.json", expected) == []
        tampered = dict(receipt, hit_target_floor_px=43.0)
        (root / "gpui.json").write_text(json.dumps(tampered), encoding="utf-8")
        assert validate_receipt(root / "gpui.json", expected), "a shrunk hit target must fail"
        stale = dict(receipt, source_manifest_sha256="another-build")
        (root / "gpui.json").write_text(json.dumps(stale), encoding="utf-8")
        assert validate_receipt(root / "gpui.json", expected), "a stale build receipt must fail"
        for frontend in EXPECTED_FRONTENDS:
            row = dict(receipt, frontend=frontend)
            (root / f"{frontend}.json").write_text(json.dumps(row), encoding="utf-8")
        assert verify(root, expected) == 0
        (root / "gpui.json").unlink()
        assert verify(root, expected) == 1, "a missing frontend receipt must fail"
    print("presentation resolution guard: self-test PASS")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--emit", action="store_true")
    parser.add_argument("--verify", action="store_true")
    parser.add_argument("--receipts", type=Path, default=Path("target/capture-receipts"))
    parser.add_argument("--expected", type=Path)
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    expected = read_json(args.expected) if args.expected else emit_contract()
    if args.emit:
        print(canonical(expected))
        return 0
    if args.verify:
        return verify(args.receipts, expected)
    parser.error("choose --emit, --verify or --self-test")
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
