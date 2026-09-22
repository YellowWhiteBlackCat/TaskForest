#!/usr/bin/env python3
"""Resolve cross-frontend evidence anchors against `cargo nextest list`.

This is the P4 ("evidence closure") Layer B resolver for the W3-C pilot.  It
reads the committed declaration manifest and checks that every declared
`behavior` anchor names a test that the environment actually discovers.

Contract
--------
* Reads committed declaration data (the manifest) and environment discovery
  (`cargo nextest list --message-format json`, or the plain `--list` text).
* Never reads production Rust source; it only does set membership between a
  declared test id and the discovered test ids.  This keeps the repository
  discipline "tests prove behavior, never source text".
* Owns NO contract-tag vocabulary.  The single authority is the Rust
  `ContractTag` enum in `crates/taskmanager-ui-contract/src/conformance.rs`,
  and the Rust conformance test reads this manifest and rejects any unknown
  `contract_tag`.  This resolver treats the tag as an opaque required field:
  it discovers and counts anchors, it does not define tags.  Never add a copy
  of the tag set to this file, the manifest, or bash.
* `pending` cells are counted and reported, never treated as dangling.
* A `behavior` anchor that is not discoverable is `dangling` and fails the run.
* A `behavior` anchor whose declared frontend differs from its `target_or_validator`
  is `invalid` and fails the run (R6 ownership).

Exit codes
----------
0  every declared anchor resolved (pending/none allowed)
1  at least one dangling or invalid anchor
2  usage / IO / discovery error

The script is standard library only and read-only with respect to the repo
except for the optional machine-readable report it writes under `target/`.
"""

from __future__ import annotations

import argparse
import csv
import json
import subprocess
import sys
import time
from pathlib import Path

MANIFEST_FIELDS = (
    "subject_kind",
    "subject_id",
    "frontend",
    "status",
    "reason",
    "contract_tag",
    "evidence_kind",
    "test_id_or_scenario",
    "target_or_validator",
)

EVIDENCE_KINDS = {"behavior", "visual", "none", "pending"}
STATUSES = {"ready", "partial", "missing", "unsupported", "pending"}

# Frontend -> (cargo package, extra nextest args).
FRONTEND_PACKAGES = {
    "gpui": ("taskmanager-gpui", ["--features", "test-support"]),
    "iced": ("taskmanager-iced", []),
    "tui": ("taskmanager-tui", []),
    "bevy": ("taskmanager-bevy-ui", []),
}


class ResolveError(RuntimeError):
    """Fatal usage / IO / discovery error (exit code 2)."""


class DiscoveryError(ResolveError):
    pass


def read_manifest(path: Path) -> list[dict[str, str]]:
    if not path.is_file():
        raise ResolveError(f"manifest not found: {path}")
    rows: list[dict[str, str]] = []
    with path.open("r", encoding="utf-8", newline="") as handle:
        data_lines = [
            line for line in handle
            if line.strip() and not line.lstrip().startswith("#")
        ]
    reader = csv.DictReader(data_lines, delimiter="\t")
    if tuple(reader.fieldnames or ()) != MANIFEST_FIELDS:
        raise ResolveError(
            f"{path}: expected fields {MANIFEST_FIELDS}, got {reader.fieldnames}"
        )
    for lineno, row in enumerate(reader, start=2):
        if any(value is None for value in row.values()):
            raise ResolveError(f"{path}:{lineno}: malformed row (wrong field count)")
        missing = [
            field for field in MANIFEST_FIELDS
            if row[field].strip() == "" and field != "reason"
        ]
        if missing:
            raise ResolveError(f"{path}:{lineno}: empty field(s): {', '.join(missing)}")
        kind = row["evidence_kind"].strip()
        if kind not in EVIDENCE_KINDS:
            raise ResolveError(f"{path}:{lineno}: unknown evidence_kind {kind}")
        if row["status"].strip() not in STATUSES:
            raise ResolveError(f"{path}:{lineno}: unknown status {row['status']}")
        rows.append(row)
    if not rows:
        raise ResolveError(f"{path}: manifest has no data rows")
    return rows


def parse_json_discovery(text: str) -> set[str]:
    try:
        payload = json.loads(text)
    except json.JSONDecodeError as exc:
        raise DiscoveryError(f"invalid nextest JSON: {exc}") from exc

    if isinstance(payload, list):
        suites = payload
    elif isinstance(payload, dict):
        suites = (
            payload.get("rust-suites")
            or payload.get("test-suites")
            or payload.get("suites")
            or []
        )
    else:
        raise DiscoveryError("unrecognised nextest JSON payload")

    if isinstance(suites, dict):
        suites = list(suites.values())

    names: set[str] = set()
    for suite in suites:
        if not isinstance(suite, dict):
            continue
        cases = suite.get("testcases", suite.get("test-cases", {}))
        if isinstance(cases, dict):
            names.update(str(name) for name in cases)
        elif isinstance(cases, list):
            for case in cases:
                if isinstance(case, dict) and case.get("name"):
                    names.add(str(case["name"]))
                elif isinstance(case, str):
                    names.add(case)
    if not names:
        raise DiscoveryError("nextest JSON discovered no test cases")
    return names


def parse_text_discovery(text: str) -> set[str]:
    names: set[str] = set()
    for raw in text.splitlines():
        line = raw.strip().rstrip("\r")
        if not line or line.startswith("#"):
            continue
        tokens = line.split()
        # `cargo nextest list` prints `<binary-id> <test-name>`; a bare test
        # name has a single token.  Anything with more tokens takes the last
        # token (test names never contain whitespace).
        if len(tokens) >= 2 and "::" in tokens[-1]:
            names.add(tokens[-1])
        elif len(tokens) == 1 and ("::" in line or line.isidentifier()):
            names.add(line)
    if not names:
        raise DiscoveryError("plain nextest list discovered no test cases")
    return names


def load_discovery(path: Path) -> set[str]:
    if not path.is_file():
        raise DiscoveryError(f"discovery artifact not found: {path}")
    text = path.read_text(encoding="utf-8", errors="replace")
    if path.suffix.lower() == ".json" or text.lstrip().startswith(("{", "[")):
        return parse_json_discovery(text)
    return parse_text_discovery(text)


def run_nextest(repo: Path, frontend: str, timeout_s: int) -> set[str]:
    package, extra = FRONTEND_PACKAGES[frontend]
    command = [
        "cargo", "nextest", "list",
        "--locked",
        "-p", package, *extra,
        "--message-format", "json",
    ]
    try:
        proc = subprocess.run(
            command,
            cwd=repo,
            check=False,
            capture_output=True,
            text=True,
            timeout=timeout_s,
            env={**__import__("os").environ, "CARGO_BUILD_JOBS": "4"},
        )
    except FileNotFoundError as exc:
        raise DiscoveryError("cargo is not available on PATH") from exc
    except subprocess.TimeoutExpired as exc:
        raise DiscoveryError(
            f"cargo nextest list timed out after {timeout_s}s for {package}"
        ) from exc
    if proc.returncode != 0:
        detail = (proc.stderr or proc.stdout or "").strip().splitlines()
        tail = " | ".join(detail[-3:])
        raise DiscoveryError(
            f"cargo nextest list failed for {package} (exit {proc.returncode}): {tail}"
        )
    return parse_json_discovery(proc.stdout)


def load_capture_scenarios(path: Path) -> set[str]:
    if not path.is_file():
        raise ResolveError(f"capture scenario file not found: {path}")
    names: set[str] = set()
    with path.open("r", encoding="utf-8", newline="") as handle:
        reader = csv.DictReader(handle, delimiter="\t")
        if not reader.fieldnames or "name" not in reader.fieldnames:
            raise ResolveError(f"{path}: capture scenario table needs a 'name' column")
        for row in reader:
            value = (row.get("name") or "").strip()
            if value:
                names.add(value)
    return names


def split_pairs(values: list[str], label: str) -> dict[str, Path]:
    result: dict[str, Path] = {}
    for raw in values:
        if "=" not in raw:
            raise ResolveError(f"--{label} expects KEY=PATH, got: {raw}")
        key, _, value = raw.partition("=")
        key = key.strip()
        if not key or not value:
            raise ResolveError(f"--{label} expects KEY=PATH, got: {raw}")
        result[key] = Path(value)
    return result


def resolve(args: argparse.Namespace) -> dict:
    repo = Path(args.repo).resolve()
    manifest_path = Path(args.manifest)
    if not manifest_path.is_absolute():
        manifest_path = repo / manifest_path
    rows = read_manifest(manifest_path)

    provided = split_pairs(args.discovery, "discovery")
    unknown = sorted(set(provided) - set(FRONTEND_PACKAGES))
    if unknown:
        raise ResolveError(f"unknown frontend(s) in --discovery: {', '.join(unknown)}")

    scenarios = split_pairs(args.capture_scenarios, "capture-scenarios")

    required_frontends = sorted({row["frontend"] for row in rows})
    badly_named = sorted(set(required_frontends) - set(FRONTEND_PACKAGES))
    if badly_named:
        raise ResolveError(f"manifest uses unknown frontend(s): {', '.join(badly_named)}")

    discovered: dict[str, set[str]] = {}
    discovery_source: dict[str, str] = {}
    if args.nextest:
        for frontend in required_frontends:
            if frontend in provided:
                discovered[frontend] = load_discovery(provided[frontend])
                discovery_source[frontend] = str(provided[frontend])
            else:
                discovered[frontend] = run_nextest(repo, frontend, args.nextest_timeout)
                discovery_source[frontend] = (
                    f"cargo nextest list -p {FRONTEND_PACKAGES[frontend][0]}"
                )
    else:
        for frontend, path in provided.items():
            discovered[frontend] = load_discovery(path)
            discovery_source[frontend] = str(path)
        missing = [f for f in required_frontends if f not in discovered]
        if missing:
            raise ResolveError(
                "no discovery for frontend(s): "
                + ", ".join(missing)
                + " (pass --discovery FRONTEND=PATH or --nextest)"
            )

    cells = anchored_behavior = anchored_visual = pending = none = 0
    dangling: list[dict] = []
    invalid: list[dict] = []
    visual_unverified: list[dict] = []
    pending_cells: list[dict] = []
    seen: set[tuple[str, str, str]] = set()

    for row in rows:
        cells += 1
        key = (row["subject_kind"], row["subject_id"], row["frontend"])
        if key in seen:
            invalid.append({
                "reason": "duplicate (subject_kind, subject_id, frontend) cell",
                "subject_kind": row["subject_kind"],
                "subject_id": row["subject_id"],
                "frontend": row["frontend"],
            })
        seen.add(key)

        kind = row["evidence_kind"]
        if kind == "behavior":
            anchored_behavior += 1
            test_id = row["test_id_or_scenario"]
            target = row["target_or_validator"]
            if target != row["frontend"]:
                invalid.append({
                    "reason": "target/frontend mismatch (R6)",
                    "subject_kind": row["subject_kind"],
                    "subject_id": row["subject_id"],
                    "frontend": row["frontend"],
                    "test_id": test_id,
                    "target": target,
                })
                continue
            if test_id not in discovered[row["frontend"]]:
                dangling.append({
                    "reason": "anchor not discovered by cargo nextest list (R4)",
                    "subject_kind": row["subject_kind"],
                    "subject_id": row["subject_id"],
                    "frontend": row["frontend"],
                    "test_id": test_id,
                })
        elif kind == "visual":
            anchored_visual += 1
            scenario = row["test_id_or_scenario"]
            validator = row["target_or_validator"]
            if validator in scenarios:
                if scenario not in scenarios[validator]:
                    dangling.append({
                        "reason": "visual scenario not present in capture table (R5)",
                        "subject_kind": row["subject_kind"],
                        "subject_id": row["subject_id"],
                        "frontend": row["frontend"],
                        "scenario": scenario,
                        "validator": validator,
                    })
            else:
                visual_unverified.append({
                    "reason": "no capture scenario table supplied for validator",
                    "subject_kind": row["subject_kind"],
                    "subject_id": row["subject_id"],
                    "frontend": row["frontend"],
                    "scenario": scenario,
                    "validator": validator,
                })
        elif kind == "pending":
            pending += 1
            pending_cells.append({
                "subject_kind": row["subject_kind"],
                "subject_id": row["subject_id"],
                "frontend": row["frontend"],
            })
        else:  # none
            none += 1

    status = "pass" if not dangling and not invalid else "fail"
    return {
        "manifest": str(manifest_path),
        "generated_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "status": status,
        "counts": {
            "cells": cells,
            "anchored_behavior": anchored_behavior,
            "anchored_visual": anchored_visual,
            "pending": pending,
            "none": none,
            "dangling": len(dangling),
            "invalid": len(invalid),
            "visual_unverified": len(visual_unverified),
        },
        "frontends": {
            frontend: {
                "discovered_tests": len(discovered[frontend]),
                "discovery_source": discovery_source[frontend],
            }
            for frontend in required_frontends
        },
        "dangling": dangling,
        "invalid": invalid,
        "visual_unverified": visual_unverified,
        "pending": sorted(
            pending_cells, key=lambda cell: (cell["subject_kind"], cell["subject_id"], cell["frontend"])
        ),
    }


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Resolve cross-frontend evidence anchors against nextest discovery."
    )
    parser.add_argument(
        "--manifest",
        default="scripts/parity/cross_frontend_manifest.tsv",
        help="committed declaration manifest (default: %(default)s)",
    )
    parser.add_argument(
        "--discovery",
        action="append",
        default=[],
        metavar="FRONTEND=PATH",
        help="pre-generated nextest discovery artifact (JSON or plain --list text)",
    )
    parser.add_argument(
        "--capture-scenarios",
        action="append",
        default=[],
        metavar="VALIDATOR=PATH",
        help="capture scenario table for visual anchors (optional)",
    )
    parser.add_argument(
        "--nextest",
        action="store_true",
        help="run `cargo nextest list -j 4` for frontends without --discovery",
    )
    parser.add_argument(
        "--nextest-timeout", type=int, default=1800,
        help="seconds before a cargo nextest list run is killed (default: %(default)s)",
    )
    parser.add_argument("--repo", default=".", help="repository root (default: cwd)")
    parser.add_argument("--report", default=None, help="report JSON path")
    parser.add_argument("--no-report", action="store_true", help="do not write a report file")
    parser.add_argument("--json", action="store_true", help="print the report JSON to stdout")
    return parser


def format_summary(report: dict) -> str:
    counts = report["counts"]
    lines = [
        f"manifest: {report['manifest']}",
        f"status:   {report['status'].upper()}",
        (
            "cells:    {cells} total | {anchored_behavior} behavior anchors | "
            "{pending} pending | {none} none | {dangling} dangling | {invalid} invalid"
        ).format(**counts),
    ]
    for frontend, info in report["frontends"].items():
        lines.append(
            f"  {frontend:5s} discovered {info['discovered_tests']} tests  ({info['discovery_source']})"
        )
    for item in report["dangling"]:
        lines.append(
            "  DANGLING {frontend}: {subject_kind}/{subject_id} -> {test_id} ({reason})".format(
                **item
            )
        )
    for item in report["invalid"]:
        lines.append(
            "  INVALID  {frontend}: {subject_kind}/{subject_id} ({reason})".format(**item)
        )
    for item in report["visual_unverified"]:
        lines.append(
            "  NOTE     {frontend}: {subject_kind}/{subject_id} visual anchor unverified "
            "({reason})".format(**item)
        )
    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        report = resolve(args)
    except ResolveError as exc:
        print(f"resolve_frontend_evidence: ERROR: {exc}", file=sys.stderr)
        return 2

    if args.json:
        print(json.dumps(report, indent=2, sort_keys=True))
    else:
        print(format_summary(report))

    if not args.no_report:
        if args.report:
            report_path = Path(args.report)
            if not report_path.is_absolute():
                report_path = Path(args.repo).resolve() / report_path
        else:
            stamp = time.strftime("%Y%m%dT%H%M%SZ", time.gmtime())
            report_path = (
                Path(args.repo).resolve()
                / "target" / "cross-frontend-evidence" / stamp
                / "manifest-validation.json"
            )
        report_path.parent.mkdir(parents=True, exist_ok=True)
        report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        if not args.json:
            print(f"report:   {report_path}")

    return 0 if report["status"] == "pass" else 1


if __name__ == "__main__":
    sys.exit(main())
