#!/usr/bin/env python3
"""Validate the allowlisted TaskForest install inventory and source wiring."""

from __future__ import annotations

import argparse
import csv
import re
import sys
from pathlib import Path


REQUIRED_COLUMNS = (
    "id",
    "scope",
    "lifecycle",
    "destination",
    "artifact",
    "owner",
    "install_method",
    "purpose",
    "privilege",
    "conflict_policy",
    "removal_method",
    "source",
)
ALLOWED_LIFECYCLES = {
    "package-managed",
    "optional-root",
    "approved-pending",
    "developer-user",
    "user-managed",
}
PATH_ANNOTATION = re.compile(
    r"<annotate key=\"org\.freedesktop\.policykit\.exec\.path\">([^<]+)</annotate>"
)
PKG_DESTINATION = re.compile(r"\$pkgdir(/[^\s\"]+)")
SETUP_RULE = re.compile(r'const RULE_PATH: &str = "([^\"]+)";')
MANAGER_DESTINATIONS = (
    "/usr/libexec/taskmanager-privilege-helper",
    "/usr/share/polkit-1/actions/com.taskforest.perf-helper.policy",
    "/usr/libexec/taskmanager-net-launcher",
    "/usr/share/polkit-1/actions/com.taskforest.net-launcher.policy",
    "/usr/lib/taskforest-process-control-helper",
    "/usr/share/polkit-1/actions/com.taskforest.process-control.policy",
)


def load_rows(path: Path) -> list[dict[str, str]]:
    with path.open(newline="", encoding="utf-8") as handle:
        reader = csv.DictReader(handle, delimiter="\t")
        if tuple(reader.fieldnames or ()) != REQUIRED_COLUMNS:
            raise ValueError("manifest header does not match the required schema")
        rows = list(reader)
    if not rows:
        raise ValueError("manifest has no rows")
    return rows


def validate_rows(rows: list[dict[str, str]]) -> list[str]:
    findings: list[str] = []
    ids: set[str] = set()
    destinations: set[str] = set()
    for row in rows:
        row_id = row["id"]
        destination = row["destination"]
        if not row_id or row_id in ids:
            findings.append(f"duplicate/empty id: {row_id!r}")
        if not destination or destination in destinations:
            findings.append(f"duplicate/empty destination: {destination!r}")
        if row["lifecycle"] not in ALLOWED_LIFECYCLES:
            findings.append(f"unsupported lifecycle for {row_id}: {row['lifecycle']!r}")
        for field in REQUIRED_COLUMNS:
            if not row[field].strip():
                findings.append(f"empty {field} for {row_id}")
        ids.add(row_id)
        destinations.add(destination)
    return findings


def validate_wiring(root: Path, rows: list[dict[str, str]]) -> list[str]:
    findings: list[str] = []
    destinations = {row["destination"] for row in rows}

    for policy in sorted((root / "polkit").glob("*.policy.in")):
        text = policy.read_text(encoding="utf-8")
        for destination in PATH_ANNOTATION.findall(text):
            if destination not in destinations:
                findings.append(f"policy destination missing from manifest: {policy}: {destination}")

    setup = root / "crates/taskmanager-setup-helper/src/main.rs"
    setup_match = SETUP_RULE.search(setup.read_text(encoding="utf-8"))
    if setup_match and setup_match.group(1) not in destinations:
        findings.append(f"setup helper destination missing from manifest: {setup_match.group(1)}")

    pkgbuild = root / "packaging/arch/PKGBUILD"
    for destination in PKG_DESTINATION.findall(pkgbuild.read_text(encoding="utf-8")):
        destination = destination.replace("$pkgname", "taskforest-git")
        if destination not in destinations:
            findings.append(f"PKGBUILD destination missing from manifest: {destination}")

    manager = root / "scripts/manage-polkit-install.sh"
    manager_text = manager.read_text(encoding="utf-8")
    for destination in MANAGER_DESTINATIONS:
        if destination not in manager_text or destination not in destinations:
            findings.append(f"polkit manager/manifest wiring missing: {destination}")

    required_ids = {
        "DEV-FRONTENDS-STATE",
        "DEV-GPUI-DESKTOP",
        "DEV-ICED-DESKTOP",
        "DEV-ICON",
        "DEV-HICOLOR-INDEX",
    }
    ids = {row["id"] for row in rows}
    for row_id in sorted(required_ids - ids):
        findings.append(f"developer install row missing: {row_id}")

    for row in (row for row in rows if row["lifecycle"] == "developer-user"):
        authority = row["source"]
        if not authority.startswith("scripts/") or not (root / authority).is_file():
            findings.append(f"developer install authority is not an existing script: {row['id']}: {authority}")
            continue
        if authority not in row["install_method"]:
            findings.append(f"developer install method bypasses its authority: {row['id']}: {authority}")
        if authority not in row["removal_method"]:
            findings.append(f"developer removal method bypasses its authority: {row['id']}: {authority}")
    return findings


def validate_receipt(receipt: Path, rows: list[dict[str, str]]) -> list[str]:
    if not receipt.is_file():
        return [f"host receipt missing: {receipt}"]
    findings: list[str] = []
    with receipt.open(newline="", encoding="utf-8") as handle:
        reader = csv.DictReader(handle, delimiter="\t")
        required = {"audit_date", "id", "destination", "state", "file_type", "sha256"}
        if not required.issubset(set(reader.fieldnames or ())):
            return ["host receipt header is incomplete"]
        known = {row["id"]: row["destination"] for row in rows}
        seen: set[str] = set()
        for row in reader:
            row_id = row["id"]
            if row_id in seen:
                findings.append(f"duplicate host receipt id: {row_id}")
            seen.add(row_id)
            if row_id not in known:
                findings.append(f"host receipt id missing from manifest: {row_id}")
            elif row["destination"] == "" or (row["destination"] not in known.values() and not row["destination"].startswith("/home/")):
                findings.append(f"host receipt destination is not allowlisted: {row_id}: {row['destination']}")
            if row["state"] == "present":
                if row["file_type"] == "directory":
                    if row["sha256"] != "-":
                        findings.append(f"directory receipt must not fabricate sha256: {row_id}")
                elif not re.fullmatch(r"[0-9a-f]{64}", row["sha256"]):
                    findings.append(f"present receipt has malformed sha256: {row_id}")
    return findings


def run(root: Path, receipt: Path | None = None) -> list[str]:
    manifest = root / "docs/system-install-manifest.tsv"
    rows = load_rows(manifest)
    findings = validate_rows(rows) + validate_wiring(root, rows)
    if receipt is not None:
        findings.extend(validate_receipt(receipt, rows))
    return findings


def self_test() -> None:
    rows = []
    for suffix, destination in (("one", "/one"), ("two", "/two")):
        row = {field: f"{field}-{suffix}" for field in REQUIRED_COLUMNS}
        row["id"] = f"ID-{suffix}"
        row["destination"] = destination
        row["lifecycle"] = "package-managed"
        rows.append(row)
    assert not validate_rows(rows)
    duplicate = [rows[0], rows[0]]
    assert any("duplicate" in finding for finding in validate_rows(duplicate))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, default=Path.cwd())
    parser.add_argument(
        "--receipt",
        type=Path,
        help="optional local host receipt; do not commit this file to the public repository",
    )
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("System-install manifest guard self-test: PASS")
        return 0
    try:
        findings = run(args.repo_root.resolve(), args.receipt)
    except (OSError, ValueError) as error:
        print(f"System-install manifest guard: ERROR {error}", file=sys.stderr)
        return 1
    for finding in findings:
        print(f"{finding}")
    print(f"System-install manifest guard: findings={len(findings)}")
    return int(bool(findings))


if __name__ == "__main__":
    raise SystemExit(main())
