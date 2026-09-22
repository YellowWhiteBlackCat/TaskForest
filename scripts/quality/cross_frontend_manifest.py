#!/usr/bin/env python3
"""Aggregate per-frontend run segments into one cross-frontend run manifest.

This is the P4/P6 execution side of the evidence-closure plan: the committed
declaration lists (``scripts/parity/cross_frontend_manifest.tsv`` and/or
``scripts/parity/cross_frontend_matrix.tsv``) stay the single authority, while
this aggregator folds the per-frontend ``run_manifest.json`` segments produced
by the S5 accept driver into one machine-readable table.

Contract
--------
* Reads committed declaration data plus generated run segments and the
  frontend-scoped source manifests produced by
  ``scripts/frontend_source_manifest.py``.  It never reads Rust source and never
  invents a test id: an anchor is declared by hand and the resolver (Layer B)
  checks discovery membership.
* The schema is ``scripts/parity/run_manifest.schema.json``.  It is normative;
  this module carries the standard-library mirror of its field sets and
  ``--self-test`` asserts that mirror against the file, so the two cannot drift
  silently.
* Every declared non-pending cell must be covered by exactly one coverable
  (``ok``/``pass``) run cell.  Duplicate, undeclared, failed, or skipped cells
  are findings, not a pass.
* Fingerprint rule (mechanical, fail-closed): a segment's
  ``source_manifest_sha256`` must equal the sha256 of the current
  frontend-scoped source manifest supplied with ``--source-manifest``.  A
  mismatch, a null digest, or no supplied fingerprint makes that frontend
  **SKIP** -- never PASS.  ``--verify`` exits non-zero unless the aggregate
  status is ``pass``.

Exit codes
----------
0  aggregation succeeded; the run manifest was written
1  ``overall_status`` is ``fail``, or ``--verify`` was passed and the aggregate
   is not ``pass``
2  usage / IO / schema-violation error (malformed segment, missing field,
   duplicate frontend segment, unknown frontend, malformed declaration)

The script is standard library only and read-only with respect to the
repository except for the ``--out`` file it writes (under ``target/`` in the
wired flow).
"""

from __future__ import annotations

import argparse
import csv
import glob
import hashlib
import json
import re
import sys
import tempfile
import time
from collections import Counter
from dataclasses import dataclass
from pathlib import Path

SCHEMA_VERSION = 1
RECORD_KIND = "cross_frontend_run_manifest"
SEGMENT_RECORD_KIND = "frontend_run_segment"
FRONTENDS = ("gpui", "iced", "tui", "bevy")
DEFAULT_MANIFEST = "scripts/parity/cross_frontend_matrix.tsv"
SCHEMA_PATH = Path(__file__).resolve().parents[1] / "parity" / "run_manifest.schema.json"

HASH_PATTERN = re.compile(r"^[0-9a-f]{64}$")
GENERATED_AT_PATTERN = re.compile(
    r"^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$"
)

# Field sets mirrored by run_manifest.schema.json.  The self-test compares each
# set with the schema file, so a schema edit without the Python mirror (or the
# other way round) fails mechanically.
AGGREGATE_REQUIRED = (
    "schema_version",
    "record_kind",
    "generated_at",
    "run_id",
    "declaration_manifest",
    "fingerprint_policy",
    "frontends",
    "coverage",
    "overall_status",
    "findings",
)
AGGREGATE_OPTIONAL: tuple[str, ...] = ()
SEGMENT_REQUIRED = (
    "schema_version",
    "record_kind",
    "frontend",
    "run_id",
    "generated_at",
    "source_manifest_sha256",
    "discovery",
    "run",
    "cells",
    "overall_status",
)
SEGMENT_OPTIONAL = (
    "git_head",
    "worktree",
    "rust",
    "source_manifest_path",
    "receipts",
)
CELL_FIELDS = (
    "subject_kind",
    "case_id",
    "subject_id",
    "scenario",
    "contract_tag",
    "test_id",
    "paths",
    "receipt",
    "outcome",
)
CELL_REQUIRED = ("outcome",)
FINDING_FIELDS = ("severity", "code", "message", "frontend", "cell")
FINDING_REQUIRED = ("severity", "code", "message")
OBSERVED_CELL_FIELDS = CELL_FIELDS + ("cell_key",)
OBSERVED_CELL_REQUIRED = CELL_REQUIRED + ("cell_key",)

OUTCOMES = ("ok", "pass", "fail", "error", "skipped")
COVERABLE_OUTCOMES = frozenset({"ok", "pass"})
RECORD_RESULTS = OUTCOMES
SEVERITIES = ("fail", "skip", "note")
STATUSES = ("pass", "fail", "skip")
FINGERPRINT_STATES = ("match", "mismatch", "missing", "unverified", "absent")
WORKTREE_STATES = ("clean", "dirty", "unknown")

INTERACTION_FIELDS = ("subject_kind", "case_id", "frontend", "test_name", "contract_tag")
FACET_FIELDS = ("subject_kind", "subject_id", "frontend", "evidence_kind", "test_id_or_scenario")
INTERACTION_SUBJECT_KIND = "interaction"
FACET_EVIDENCE_KINDS = ("behavior", "visual", "none", "pending")

SEVERITY_RANK = {"fail": 0, "skip": 1, "note": 2}
MATCH_RULE = (
    "sha256 of the current frontend source manifest must equal the segment's "
    "source_manifest_sha256"
)


class AggregateError(Exception):
    """Usage, IO, or schema-violation error; reported with exit code 2."""


@dataclass(frozen=True)
class DeclaredCell:
    """One expected declaration cell, keyed the same way a run cell is keyed."""

    frontend: str
    cell_key: str
    subject_kind: str
    subject_id: str
    anchor_test_id: str | None


@dataclass(frozen=True)
class Declaration:
    """The committed declaration list this aggregate is checked against."""

    display_path: str
    kind: str
    sha256: str
    total_cells: int
    cells: tuple[DeclaredCell, ...]
    pending: tuple[DeclaredCell, ...]


def sha256_file(path: Path) -> str:
    hasher = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            hasher.update(chunk)
    return hasher.hexdigest()


def utc_now() -> str:
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())


def resolve_path(repo: Path, value: str) -> Path:
    path = Path(value)
    return path if path.is_absolute() else repo / path


def display_path(path: Path, repo: Path) -> str:
    try:
        return path.resolve().relative_to(repo.resolve()).as_posix()
    except ValueError:
        return path.as_posix()


def is_int(value: object) -> bool:
    return isinstance(value, int) and not isinstance(value, bool)


def require_string(value: object, label: str) -> str:
    if not isinstance(value, str) or not value:
        raise AggregateError(f"{label} must be a non-empty string")
    return value


def require_object(value: object, label: str) -> dict:
    if not isinstance(value, dict):
        raise AggregateError(f"{label} must be an object")
    return value


def reject_unknown_fields(value: dict, allowed: tuple[str, ...], label: str) -> None:
    unknown = sorted(set(value) - set(allowed))
    if unknown:
        raise AggregateError(f"{label} has unknown field(s): {', '.join(unknown)}")


def require_fields(value: dict, required: tuple[str, ...], label: str) -> None:
    missing = sorted(set(required) - set(value))
    if missing:
        raise AggregateError(f"{label} is missing required field(s): {', '.join(missing)}")


# ---------------------------------------------------------------------------
# Declaration manifest (committed authority)
# ---------------------------------------------------------------------------


def read_tsv_rows(path: Path) -> tuple[list[str], list[dict[str, str]]]:
    """Read a committed TSV, skipping its `#` comment header and blank lines."""

    try:
        text = path.read_text(encoding="utf-8")
    except OSError as error:
        raise AggregateError(f"cannot read declaration manifest: {path}: {error}") from error
    lines = [
        line
        for line in text.splitlines()
        if line.strip() and not line.startswith("#")
    ]
    if not lines:
        raise AggregateError(f"declaration manifest has no data rows: {path}")
    reader = csv.DictReader(lines, delimiter="\t")
    fieldnames = list(reader.fieldnames or ())
    if not fieldnames:
        raise AggregateError(f"declaration manifest has no header: {path}")
    return fieldnames, list(reader)


def load_declaration(path: Path, repo: Path) -> Declaration:
    fieldnames, rows = read_tsv_rows(path)
    if "case_id" in fieldnames:
        kind = "interaction_matrix"
        required = INTERACTION_FIELDS
    elif "evidence_kind" in fieldnames:
        kind = "facet_manifest"
        required = FACET_FIELDS
    else:
        raise AggregateError(
            f"declaration manifest {path} is neither an interaction matrix "
            "(needs case_id) nor a facet manifest (needs evidence_kind)"
        )
    missing = sorted(set(required) - set(fieldnames))
    if missing:
        raise AggregateError(
            f"declaration manifest {path} is missing column(s): {', '.join(missing)}"
        )

    cells: list[DeclaredCell] = []
    pending: list[DeclaredCell] = []
    seen: set[tuple[str, str]] = set()
    for number, row in enumerate(rows, start=2):
        label = f"{path}:{number}"
        frontend = (row.get("frontend") or "").strip()
        if frontend not in FRONTENDS:
            raise AggregateError(f"{label} declares unknown frontend: {frontend!r}")
        if kind == "interaction_matrix":
            subject_kind = (row.get("subject_kind") or "").strip()
            if subject_kind != INTERACTION_SUBJECT_KIND:
                raise AggregateError(
                    f"{label} subject_kind must be {INTERACTION_SUBJECT_KIND!r}: "
                    f"{subject_kind!r}"
                )
            case_id = (row.get("case_id") or "").strip()
            if not case_id:
                raise AggregateError(f"{label} has no case_id")
            test_name = (row.get("test_name") or "").strip()
            if test_name == "pending":
                cell = DeclaredCell(frontend, f"interaction:{case_id}", subject_kind, case_id, None)
                pending.append(cell)
                continue
            anchor = None if test_name == "-" else test_name
            cell = DeclaredCell(frontend, f"interaction:{case_id}", subject_kind, case_id, anchor)
        else:
            subject_kind = (row.get("subject_kind") or "").strip()
            subject_id = (row.get("subject_id") or "").strip()
            if not subject_id:
                raise AggregateError(f"{label} has no subject_id")
            evidence_kind = (row.get("evidence_kind") or "").strip()
            if evidence_kind not in FACET_EVIDENCE_KINDS:
                raise AggregateError(f"{label} has unknown evidence_kind: {evidence_kind!r}")
            cell_key = f"{subject_kind}:{subject_id}"
            if evidence_kind == "pending":
                pending.append(
                    DeclaredCell(frontend, cell_key, subject_kind, subject_id, None)
                )
                continue
            if evidence_kind == "none":
                # An honest no-anchor cell (unsupported): declared, not expected.
                continue
            anchor = (row.get("test_id_or_scenario") or "").strip()
            if evidence_kind == "visual":
                cell = DeclaredCell(frontend, f"visual:{anchor}", subject_kind, subject_id, None)
            else:
                cell = DeclaredCell(frontend, cell_key, subject_kind, subject_id, anchor)
        if (frontend, cell.cell_key) in seen:
            raise AggregateError(f"{label} duplicates declared cell {frontend}:{cell.cell_key}")
        seen.add((frontend, cell.cell_key))
        cells.append(cell)

    return Declaration(
        display_path=display_path(path, repo),
        kind=kind,
        sha256=sha256_file(path),
        total_cells=len(rows),
        cells=tuple(cells),
        pending=tuple(pending),
    )


# ---------------------------------------------------------------------------
# Run segments (generated input)
# ---------------------------------------------------------------------------


def validate_io_record(value: object, label: str) -> None:
    record = require_object(value, label)
    reject_unknown_fields(record, ("command", "result", "counts"), label)
    require_fields(record, ("command", "result", "counts"), label)
    require_string(record["command"], f"{label}.command")
    if record["result"] not in RECORD_RESULTS:
        raise AggregateError(f"{label}.result must be one of {RECORD_RESULTS}: {record['result']!r}")
    counts = require_object(record["counts"], f"{label}.counts")
    for key, value in counts.items():
        if not is_int(value) or value < 0:
            raise AggregateError(f"{label}.counts.{key} must be a non-negative integer")


def validate_cell(value: object, label: str) -> None:
    cell = require_object(value, label)
    reject_unknown_fields(cell, CELL_FIELDS, label)
    require_fields(cell, CELL_REQUIRED, label)
    if cell["outcome"] not in OUTCOMES:
        raise AggregateError(f"{label}.outcome must be one of {OUTCOMES}: {cell['outcome']!r}")
    for field in (
        "subject_kind",
        "case_id",
        "subject_id",
        "scenario",
        "contract_tag",
        "test_id",
        "receipt",
    ):
        if field in cell:
            require_string(cell[field], f"{label}.{field}")
    if "paths" in cell:
        paths = cell["paths"]
        if not isinstance(paths, list) or not all(isinstance(item, str) and item for item in paths):
            raise AggregateError(f"{label}.paths must be a list of non-empty strings")


def validate_segment(segment: dict, label: str) -> None:
    reject_unknown_fields(segment, SEGMENT_REQUIRED + SEGMENT_OPTIONAL, label)
    require_fields(segment, SEGMENT_REQUIRED, label)
    if segment["schema_version"] != SCHEMA_VERSION:
        raise AggregateError(f"{label}.schema_version must be {SCHEMA_VERSION}")
    if segment["record_kind"] != SEGMENT_RECORD_KIND:
        raise AggregateError(f"{label}.record_kind must be {SEGMENT_RECORD_KIND!r}")
    if segment["frontend"] not in FRONTENDS:
        raise AggregateError(f"{label}.frontend must be one of {FRONTENDS}: {segment['frontend']!r}")
    require_string(segment["run_id"], f"{label}.run_id")
    generated_at = require_string(segment["generated_at"], f"{label}.generated_at")
    if not GENERATED_AT_PATTERN.match(generated_at):
        raise AggregateError(f"{label}.generated_at must be UTC `YYYY-MM-DDTHH:MM:SSZ`")
    digest = segment["source_manifest_sha256"]
    if digest is not None and not (isinstance(digest, str) and HASH_PATTERN.match(digest)):
        raise AggregateError(f"{label}.source_manifest_sha256 must be a lowercase sha256 or null")
    if "worktree" in segment and segment["worktree"] not in WORKTREE_STATES:
        raise AggregateError(f"{label}.worktree must be one of {WORKTREE_STATES}")
    if "receipts" in segment:
        receipts = segment["receipts"]
        if not isinstance(receipts, list) or not all(isinstance(item, str) and item for item in receipts):
            raise AggregateError(f"{label}.receipts must be a list of non-empty strings")
    validate_io_record(segment["discovery"], f"{label}.discovery")
    validate_io_record(segment["run"], f"{label}.run")
    cells = segment["cells"]
    if not isinstance(cells, list):
        raise AggregateError(f"{label}.cells must be a list")
    for number, cell in enumerate(cells):
        validate_cell(cell, f"{label}.cells[{number}]")
    if segment["overall_status"] not in STATUSES:
        raise AggregateError(
            f"{label}.overall_status must be one of {STATUSES}: {segment['overall_status']!r}"
        )


def load_segment(path: Path) -> dict:
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except OSError as error:
        raise AggregateError(f"cannot read run segment: {path}: {error}") from error
    except json.JSONDecodeError as error:
        raise AggregateError(f"run segment is not valid JSON: {path}: {error}") from error
    segment = require_object(data, str(path))
    validate_segment(segment, str(path))
    return segment


def observed_cell_key(cell: dict) -> str:
    case_id = cell.get("case_id")
    if case_id:
        return f"interaction:{case_id}"
    scenario = cell.get("scenario")
    if scenario:
        return f"visual:{scenario}"
    subject_id = cell.get("subject_id")
    if subject_id:
        return f"{cell.get('subject_kind') or 'facet'}:{subject_id}"
    raise AggregateError(
        "run cell has no identity: declare case_id, scenario, or subject_id"
    )


def normalize_cell(cell: dict) -> dict:
    normalized = {"cell_key": observed_cell_key(cell)}
    for field in CELL_FIELDS:
        if field in cell:
            normalized[field] = cell[field]
    return normalized


def expand_inputs(raw_values: list[str], repo: Path) -> list[tuple[str | None, Path]]:
    """Expand `--inputs` values: `PATH`, `FRONTEND=PATH`, a directory, or a glob."""

    paths: list[tuple[str | None, Path]] = []
    for raw in raw_values:
        key, value = None, raw
        if "=" in raw:
            candidate, _, remainder = raw.partition("=")
            if candidate in FRONTENDS and remainder:
                key, value = candidate, remainder
        if any(character in value for character in "*?["):
            matches = sorted(glob.glob(str(resolve_path(repo, value))))
            if not matches:
                raise AggregateError(f"--inputs matched no file: {raw}")
            paths.extend((key, Path(match)) for match in matches)
            continue
        path = resolve_path(repo, value)
        if path.is_dir():
            matches = sorted(path.glob("*.json"))
            if not matches:
                raise AggregateError(f"--inputs directory has no *.json segments: {path}")
            paths.extend((key, match) for match in matches)
            continue
        paths.append((key, path))
    return paths


# ---------------------------------------------------------------------------
# Aggregation
# ---------------------------------------------------------------------------


def finding(
    severity: str,
    code: str,
    message: str,
    frontend: str | None = None,
    cell: str | None = None,
) -> dict:
    return {
        "severity": severity,
        "code": code,
        "message": message,
        "frontend": frontend,
        "cell": cell,
    }


def fingerprint_finding(
    frontend: str, state: str, declared: str | None, current: str | None
) -> dict:
    if state == "mismatch":
        return finding(
            "skip",
            "fingerprint_mismatch",
            f"{frontend}: declared source manifest {declared[:12]}... does not match the "
            f"current {current[:12]}...; frontend is SKIP, not PASS",
            frontend,
        )
    if state == "missing":
        return finding(
            "skip",
            "fingerprint_missing",
            f"{frontend}: segment declares no source_manifest_sha256; frontend is SKIP, not PASS",
            frontend,
        )
    return finding(
        "skip",
        "fingerprint_unverified",
        f"{frontend}: no current source manifest supplied for this frontend; "
        "frontend is SKIP, not PASS",
        frontend,
    )


def aggregate_run(
    declaration: Declaration,
    segments: dict[str, dict],
    current_fingerprints: dict[str, tuple[Path, str]],
    expected_frontends: tuple[str, ...],
    run_id: str,
    generated_at: str,
) -> dict:
    findings: list[dict] = []
    records: dict[str, dict] = {}
    verified: list[str] = []
    skipped: list[str] = []
    present_list: list[str] = []
    absent: list[str] = []

    for frontend in FRONTENDS:
        declared_here = [cell for cell in declaration.cells if cell.frontend == frontend]
        pending_here = [cell for cell in declaration.pending if cell.frontend == frontend]
        expected = {cell.cell_key: cell for cell in declared_here}
        pending_keys = {cell.cell_key for cell in pending_here}
        segment = segments.get(frontend)

        if segment is None:
            absent.append(frontend)
            if frontend in expected_frontends:
                findings.append(
                    finding(
                        "fail",
                        "expected_frontend_absent",
                        f"{frontend}: expected in this run but no segment was supplied "
                        f"({len(declared_here)} declared cell(s) unverified)",
                        frontend,
                    )
                )
            elif declared_here:
                findings.append(
                    finding(
                        "note",
                        "frontend_absent",
                        f"{frontend}: no segment in this run; {len(declared_here)} declared "
                        "cell(s) stay unverified",
                        frontend,
                    )
                )
            records[frontend] = {
                "present": False,
                "overall_status": "skip",
                "fingerprint": "absent",
                "source_manifest_sha256": None,
                "current_source_manifest_sha256": None,
                "run_id": None,
                "discovery": None,
                "run": None,
                "counts": {
                    "declared_cells": len(declared_here),
                    "expected_cells": len(expected),
                    "observed_cells": 0,
                    "covered_cells": 0,
                    "uncovered_cells": len(expected),
                    "duplicate_cells": 0,
                    "undeclared_cells": 0,
                    "failed_cells": 0,
                    "pending_cells": len(pending_here),
                },
                "cells": [],
                "receipts": [],
            }
            continue

        present_list.append(frontend)
        local: list[dict] = []

        if segment["overall_status"] == "fail":
            local.append(
                finding(
                    "fail",
                    "segment_status_fail",
                    f"{frontend}: run segment reports status fail",
                    frontend,
                )
            )
        elif segment["overall_status"] == "skip":
            local.append(
                finding(
                    "skip",
                    "segment_status_skip",
                    f"{frontend}: run segment reports status skip",
                    frontend,
                )
            )
        for label, record in (("discovery", segment["discovery"]), ("run", segment["run"])):
            if record["result"] in ("fail", "error"):
                local.append(
                    finding(
                        "fail",
                        f"{label}_result_not_ok",
                        f"{frontend}: {label} result is {record['result']} ({record['command']})",
                        frontend,
                    )
                )
            elif record["result"] == "skipped":
                local.append(
                    finding(
                        "skip",
                        f"{label}_result_skipped",
                        f"{frontend}: {label} was skipped",
                        frontend,
                    )
                )

        declared_digest = segment["source_manifest_sha256"]
        current = current_fingerprints.get(frontend)
        current_digest = current[1] if current else None
        if current is None:
            fingerprint_state = "unverified"
        elif declared_digest is None:
            fingerprint_state = "missing"
        elif declared_digest != current_digest:
            fingerprint_state = "mismatch"
        else:
            fingerprint_state = "match"
        if fingerprint_state != "match":
            local.append(fingerprint_finding(frontend, fingerprint_state, declared_digest, current_digest))

        observed: dict[str, dict] = {}
        duplicate_cells = 0
        for cell in segment["cells"]:
            key = observed_cell_key(cell)
            if key in observed:
                duplicate_cells += 1
                local.append(
                    finding(
                        "fail",
                        "duplicate_run_cell",
                        f"{frontend}: cell {key} reported more than once",
                        frontend,
                        key,
                    )
                )
                continue
            observed[key] = cell

        covered = 0
        failed_cells = 0
        for key, declared_cell in expected.items():
            cell = observed.get(key)
            if cell is None:
                local.append(
                    finding(
                        "fail",
                        "declared_cell_uncovered",
                        f"{frontend}: declared cell {key} has no run cell",
                        frontend,
                        key,
                    )
                )
                continue
            outcome = cell["outcome"]
            if outcome in ("fail", "error"):
                failed_cells += 1
                local.append(
                    finding(
                        "fail",
                        "observed_cell_failed",
                        f"{frontend}: cell {key} outcome is {outcome}",
                        frontend,
                        key,
                    )
                )
                continue
            if outcome not in COVERABLE_OUTCOMES:
                local.append(
                    finding(
                        "skip",
                        "declared_cell_skipped",
                        f"{frontend}: cell {key} outcome is {outcome}; not covered",
                        frontend,
                        key,
                    )
                )
                continue
            covered += 1
            anchor = declared_cell.anchor_test_id
            observed_test = cell.get("test_id")
            if anchor and observed_test and anchor != observed_test:
                local.append(
                    finding(
                        "fail",
                        "anchor_test_id_mismatch",
                        f"{frontend}: cell {key} ran {observed_test} but the declaration anchors {anchor}",
                        frontend,
                        key,
                    )
                )

        undeclared_cells = 0
        for key, cell in observed.items():
            if key in expected:
                continue
            if key in pending_keys:
                local.append(
                    finding(
                        "fail",
                        "pending_declared_with_outcome",
                        f"{frontend}: cell {key} is declared pending but the run reports "
                        f"{cell['outcome']}; promote the declaration in the same change",
                        frontend,
                        key,
                    )
                )
                continue
            undeclared_cells += 1
            local.append(
                finding(
                    "fail",
                    "undeclared_run_cell",
                    f"{frontend}: cell {key} is not declared",
                    frontend,
                    key,
                )
            )

        if not declared_here and not pending_here:
            local.append(
                finding(
                    "note",
                    "no_declared_cells",
                    f"{frontend}: declaration has no cells for this frontend",
                    frontend,
                )
            )

        findings.extend(local)
        status = "pass"
        if any(item["severity"] == "fail" for item in local):
            status = "fail"
        elif (
            any(item["severity"] == "skip" for item in local)
            or fingerprint_state != "match"
            or segment["overall_status"] == "skip"
        ):
            status = "skip"
        if status == "pass":
            verified.append(frontend)
        else:
            skipped.append(frontend)

        records[frontend] = {
            "present": True,
            "overall_status": status,
            "fingerprint": fingerprint_state,
            "source_manifest_sha256": declared_digest,
            "current_source_manifest_sha256": current_digest,
            "run_id": segment["run_id"],
            "discovery": segment["discovery"],
            "run": segment["run"],
            "counts": {
                "declared_cells": len(declared_here),
                "expected_cells": len(expected),
                "observed_cells": len(observed),
                "covered_cells": covered,
                "uncovered_cells": len(expected) - covered,
                "duplicate_cells": duplicate_cells,
                "undeclared_cells": undeclared_cells,
                "failed_cells": failed_cells,
                "pending_cells": len(pending_here),
            },
            "cells": [normalize_cell(cell) for _, cell in sorted(observed.items())],
            "receipts": list(segment.get("receipts", ())),
        }

    if any(item["severity"] == "fail" for item in findings):
        overall_status = "fail"
    elif any(item["severity"] == "skip" for item in findings):
        overall_status = "skip"
    elif not verified:
        overall_status = "skip"
    else:
        overall_status = "pass"

    findings.sort(
        key=lambda item: (
            SEVERITY_RANK[item["severity"]],
            FRONTENDS.index(item["frontend"]) if item["frontend"] in FRONTENDS else len(FRONTENDS),
            item["code"],
            item["cell"] or "",
        )
    )
    return {
        "schema_version": SCHEMA_VERSION,
        "record_kind": RECORD_KIND,
        "generated_at": generated_at,
        "run_id": run_id,
        "declaration_manifest": {
            "path": declaration.display_path,
            "kind": declaration.kind,
            "sha256": declaration.sha256,
            "declared_cells": declaration.total_cells,
            "expected_cells": len(declaration.cells),
            "pending_cells": len(declaration.pending),
        },
        "fingerprint_policy": {
            "algorithm": "sha256",
            "authority": "scripts/frontend_source_manifest.py",
            "match_rule": MATCH_RULE,
            "mismatch_policy": "skip",
        },
        "frontends": records,
        "coverage": {
            "expected_frontends": [
                frontend for frontend in FRONTENDS if frontend in expected_frontends
            ],
            "present_frontends": present_list,
            "verified_frontends": verified,
            "skipped_frontends": skipped,
            "absent_frontends": absent,
            "declared_cells": sum(
                record["counts"]["declared_cells"] for record in records.values()
            ),
            "expected_cells": sum(
                record["counts"]["expected_cells"] for record in records.values()
            ),
            "covered_cells": sum(
                record["counts"]["covered_cells"] for record in records.values()
            ),
        },
        "overall_status": overall_status,
        "findings": findings,
    }


def exit_code_for(aggregate: dict, verify: bool) -> int:
    if aggregate["overall_status"] == "fail":
        return 1
    if verify and aggregate["overall_status"] != "pass":
        return 1
    return 0


def split_required_pair(raw: str, label: str) -> tuple[str, str]:
    key, separator, value = raw.partition("=")
    if not separator or not key or not value or key not in FRONTENDS:
        raise AggregateError(f"--{label} expects FRONTEND=PATH, got: {raw}")
    return key, value


def run(args: argparse.Namespace) -> tuple[int, dict]:
    repo = Path(args.repo_root).resolve()
    if not args.inputs:
        raise AggregateError("--inputs is required (at least one run segment)")
    if args.out is None:
        raise AggregateError("--out is required")

    declaration = load_declaration(resolve_path(repo, args.manifest), repo)

    segments: dict[str, dict] = {}
    segment_paths: dict[str, Path] = {}
    for key, path in expand_inputs(args.inputs, repo):
        segment = load_segment(path)
        frontend = segment["frontend"]
        if key is not None and key != frontend:
            raise AggregateError(
                f"--inputs {key}={path} carries frontend {frontend!r}; the pair must match the segment"
            )
        if frontend in segments:
            raise AggregateError(
                f"duplicate frontend segment for {frontend}: {segment_paths[frontend]} and {path}"
            )
        segments[frontend] = segment
        segment_paths[frontend] = path

    current_fingerprints: dict[str, tuple[Path, str]] = {}
    for raw in args.source_manifest:
        frontend, value = split_required_pair(raw, "source-manifest")
        path = resolve_path(repo, value)
        if not path.is_file():
            raise AggregateError(f"--source-manifest file not found: {path}")
        current_fingerprints[frontend] = (path, sha256_file(path))

    expected_frontends = tuple(args.expect_frontend or ())
    generated_at = utc_now()
    run_id = args.run_id or time.strftime("%Y%m%dT%H%M%SZ", time.gmtime())
    aggregate = aggregate_run(
        declaration,
        segments,
        current_fingerprints,
        expected_frontends,
        run_id,
        generated_at,
    )

    out_path = resolve_path(repo, args.out)
    try:
        out_path.parent.mkdir(parents=True, exist_ok=True)
        out_path.write_text(json.dumps(aggregate, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    except OSError as error:
        raise AggregateError(f"cannot write run manifest: {out_path}: {error}") from error
    return exit_code_for(aggregate, args.verify), aggregate


def format_summary(aggregate: dict, out_path: Path) -> str:
    lines = [f"cross-frontend manifest: {out_path}"]
    lines.append(f"status:   {aggregate['overall_status'].upper()}")
    declaration = aggregate["declaration_manifest"]
    lines.append(
        f"declaration: {declaration['path']} ({declaration['kind']}; "
        f"{declaration['declared_cells']} declared / {declaration['expected_cells']} expected / "
        f"{declaration['pending_cells']} pending)"
    )
    parts = []
    for frontend, record in aggregate["frontends"].items():
        if not record["present"]:
            parts.append(f"{frontend}=absent")
            continue
        marker = f",{record['fingerprint']}" if record["fingerprint"] != "match" else ""
        counts = record["counts"]
        parts.append(
            f"{frontend}={record['overall_status']}"
            f"({counts['covered_cells']}/{counts['expected_cells']}{marker})"
        )
    lines.append("frontends: " + " ".join(parts))
    coverage = aggregate["coverage"]
    lines.append(
        "verified:  "
        + (",".join(coverage["verified_frontends"]) or "-")
        + "  skipped: "
        + (",".join(coverage["skipped_frontends"]) or "-")
        + "  absent: "
        + (",".join(coverage["absent_frontends"]) or "-")
    )
    counts = Counter(item["severity"] for item in aggregate["findings"])
    lines.append(
        f"findings: {len(aggregate['findings'])} "
        f"(fail={counts['fail']} skip={counts['skip']} note={counts['note']})"
    )
    for item in aggregate["findings"]:
        location = f" [{item['frontend']}:{item['cell']}]" if item["frontend"] and item["cell"] else ""
        if item["frontend"] and not item["cell"]:
            location = f" [{item['frontend']}]"
        lines.append(f"  {item['severity'].upper():4} {item['code']}: {item['message']}{location}")
    return "\n".join(lines)


# ---------------------------------------------------------------------------
# Self-test
# ---------------------------------------------------------------------------


def assert_schema_parity(schema: dict) -> None:
    assert schema["properties"]["schema_version"]["const"] == SCHEMA_VERSION
    assert schema["properties"]["record_kind"]["const"] == RECORD_KIND
    segment = schema["$defs"]["frontend_run_segment"]
    segment_properties = segment["properties"]
    assert segment_properties["schema_version"]["const"] == SCHEMA_VERSION
    assert segment_properties["record_kind"]["const"] == SEGMENT_RECORD_KIND
    assert set(schema["required"]) == set(AGGREGATE_REQUIRED + AGGREGATE_OPTIONAL)
    assert set(schema["properties"]) == set(AGGREGATE_REQUIRED + AGGREGATE_OPTIONAL)
    assert set(segment["required"]) == set(SEGMENT_REQUIRED)
    assert set(segment_properties) == set(SEGMENT_REQUIRED + SEGMENT_OPTIONAL)
    cell = schema["$defs"]["segment_cell"]
    assert set(cell["required"]) == set(CELL_REQUIRED)
    assert set(cell["properties"]) == set(CELL_FIELDS)
    observed = schema["$defs"]["observed_cell"]
    assert set(observed["required"]) == set(OBSERVED_CELL_REQUIRED)
    assert set(observed["properties"]) == set(OBSERVED_CELL_FIELDS)
    item = schema["$defs"]["finding"]
    assert set(item["required"]) == set(FINDING_REQUIRED)
    assert set(item["properties"]) == set(FINDING_FIELDS)
    assert tuple(schema["$defs"]["frontend_id"]["enum"]) == FRONTENDS
    assert set(schema["$defs"]["outcome"]["enum"]) == set(OUTCOMES)
    assert set(schema["$defs"]["record_result"]["enum"]) == set(RECORD_RESULTS)
    assert set(schema["$defs"]["status"]["enum"]) == set(STATUSES)
    assert set(schema["$defs"]["severity"]["enum"]) == set(SEVERITIES)
    assert set(schema["$defs"]["fingerprint_state"]["enum"]) == set(FINGERPRINT_STATES)
    assert set(schema["$defs"]["worktree_state"]["enum"]) == set(WORKTREE_STATES)


def assert_aggregate_shape(schema: dict, aggregate: dict) -> None:
    """Prove the generated aggregate uses exactly the fields the schema declares."""

    assert set(aggregate) == set(schema["required"])
    frontend_record = schema["$defs"]["frontend_record"]
    observed = schema["$defs"]["observed_cell"]
    finding_def = schema["$defs"]["finding"]
    counts_def = schema["$defs"]["frontend_counts"]
    coverage_def = schema["$defs"]["coverage"]
    declaration_def = schema["$defs"]["declaration_manifest"]
    policy_def = schema["$defs"]["fingerprint_policy"]
    assert set(aggregate["coverage"]) == set(coverage_def["required"])
    assert set(aggregate["declaration_manifest"]) == set(declaration_def["required"])
    assert set(aggregate["fingerprint_policy"]) == set(policy_def["required"])
    for frontend, record in aggregate["frontends"].items():
        assert frontend in schema["$defs"]["frontend_id"]["enum"]
        assert set(record) == set(frontend_record["properties"])
        assert set(frontend_record["required"]) <= set(record)
        assert set(record["counts"]) == set(counts_def["properties"])
        for io_record in (record["discovery"], record["run"]):
            if io_record is not None:
                assert set(io_record) == {"command", "result", "counts"}
        for cell in record["cells"]:
            assert set(cell) <= set(observed["properties"])
            assert set(observed["required"]) <= set(cell)
    for item in aggregate["findings"]:
        assert set(item) == set(finding_def["properties"])


def write_json(path: Path, data: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def interaction_declaration() -> str:
    header = (
        "subject_kind\tcase_id\tfrontend\tp0_id\ttarget\ttest_name\tpaths\t"
        "capture_scenarios\tcontract_tag\tplatform"
    )
    rows = [
        "interaction\tmc00-gpui\tgpui\tP0-MC-00\tgui\tui::tests::mc00_case_page\tsuccess\t-\tsuccess\t",
        "interaction\tmc01-iced\ticed\tP0-MC-01\tlib\tui::tests::mc01_case_page\tsuccess+toggle\t-\tsuccess\t",
        "interaction\tmc02-tui\ttui\t-\tlib\tpending\tsuccess\t-\tsuccess\t",
        "interaction\tmc03-gpui\tgpui\t-\tgui\tpending\tsuccess\t-\tsuccess\t",
    ]
    return "\n".join([header, *rows]) + "\n"


def facet_declaration() -> str:
    header = (
        "subject_kind\tsubject_id\tfrontend\tstatus\treason\tcontract_tag\t"
        "evidence_kind\ttest_id_or_scenario\ttarget_or_validator\tplatform"
    )
    rows = [
        "facet\tkeyboard-shortcuts\tgpui\tready\t\tkeyboard\tbehavior\tui::tests::shortcuts\tgpui\t",
        "facet\tsettings-capture\tgpui\tready\t\tcapture-visual\tvisual\tsettings\tgpu\t",
        "facet\tthermal-events\tbevy\tunsupported\tno source\thonesty\tnone\t-\tbevy\t",
    ]
    # `reason` is required only for non-ready statuses; the tsv carries the
    # column anyway.  The unsupported row must not produce a run expectation.
    return "\n".join([header, *rows]) + "\n"


def segment(
    frontend: str,
    run_id: str,
    digest: str | None,
    cells: list[dict],
    status: str = "pass",
    discovery_result: str = "ok",
    run_result: str = "ok",
) -> dict:
    return {
        "schema_version": SCHEMA_VERSION,
        "record_kind": SEGMENT_RECORD_KIND,
        "frontend": frontend,
        "run_id": run_id,
        "generated_at": utc_now(),
        "git_head": "0" * 40,
        "worktree": "dirty",
        "rust": "rustc test",
        "source_manifest_sha256": digest,
        "source_manifest_path": f"target/frontend-source-manifests/{frontend}.txt",
        "discovery": {
            "command": f"cargo nextest list -p taskmanager-{frontend} --message-format json",
            "result": discovery_result,
            "counts": {"discovered_tests": 3},
        },
        "run": {
            "command": f"cargo nextest run -p taskmanager-{frontend}",
            "result": run_result,
            "counts": {"cells": len(cells)},
        },
        "cells": cells,
        "receipts": [],
        "overall_status": status,
    }


def interaction_case(root: Path) -> dict:
    """Synthetic fixture: one declaration TSV, two source manifests, two segments."""

    case = root / "interaction"
    case.mkdir(parents=True, exist_ok=True)
    (case / "declaration.tsv").write_text(interaction_declaration(), encoding="utf-8")
    (case / "source-gpui.txt").write_text("gpui-source\n", encoding="utf-8")
    (case / "source-iced.txt").write_text("iced-source\n", encoding="utf-8")
    gpui_digest = sha256_file(case / "source-gpui.txt")
    iced_digest = sha256_file(case / "source-iced.txt")
    write_json(
        case / "segments" / "gpui.json",
        segment(
            "gpui",
            "run-gpui",
            gpui_digest,
            [
                {
                    "subject_kind": "interaction",
                    "case_id": "mc00-gpui",
                    "contract_tag": "success",
                    "test_id": "ui::tests::mc00_case_page",
                    "outcome": "ok",
                    "paths": ["success"],
                }
            ],
        ),
    )
    write_json(
        case / "segments" / "iced.json",
        segment(
            "iced",
            "run-iced",
            iced_digest,
            [
                {
                    "subject_kind": "interaction",
                    "case_id": "mc01-iced",
                    "contract_tag": "success",
                    "test_id": "ui::tests::mc01_case_page",
                    "outcome": "ok",
                    "paths": ["success", "toggle"],
                }
            ],
        ),
    )
    return {
        "case": case,
        "gpui_digest": gpui_digest,
        "iced_digest": iced_digest,
        "out": case / "target" / "manifest.json",
    }


def parser_args(parser: argparse.ArgumentParser, values: list[str]) -> argparse.Namespace:
    return parser.parse_args(values)


def self_test() -> None:
    assert_schema_parity(json.loads(SCHEMA_PATH.read_text(encoding="utf-8")))
    parser = build_parser()

    with tempfile.TemporaryDirectory(prefix="cross-frontend-manifest-") as directory:
        root = Path(directory)
        fixture = interaction_case(root)
        case = fixture["case"]
        base = [
            "--repo-root",
            str(root),
            "--manifest",
            str(case / "declaration.tsv"),
        ]

        # 1. All declared cells covered, fingerprints match -> PASS / exit 0.
        code, aggregate = run(
            parser_args(
                parser,
                base
                + [
                    "--inputs",
                    str(case / "segments"),
                    "--source-manifest",
                    f"gpui={case / 'source-gpui.txt'}",
                    "--source-manifest",
                    f"iced={case / 'source-iced.txt'}",
                    "--out",
                    str(fixture["out"]),
                    "--verify",
                ],
            )
        )
        assert code == 0, code
        assert aggregate["overall_status"] == "pass", aggregate["overall_status"]
        assert aggregate["coverage"]["verified_frontends"] == ["gpui", "iced"]
        assert aggregate["coverage"]["absent_frontends"] == ["tui", "bevy"]
        assert aggregate["coverage"]["covered_cells"] == 2
        assert aggregate["coverage"]["expected_cells"] == 2
        assert aggregate["frontends"]["gpui"]["fingerprint"] == "match"
        assert aggregate["frontends"]["gpui"]["counts"]["covered_cells"] == 1
        assert aggregate["frontends"]["tui"]["fingerprint"] == "absent"
        assert aggregate["frontends"]["tui"]["counts"]["pending_cells"] == 1
        written = json.loads(fixture["out"].read_text(encoding="utf-8"))
        assert written == aggregate
        assert_aggregate_shape(json.loads(SCHEMA_PATH.read_text(encoding="utf-8")), written)
        assert aggregate["declaration_manifest"]["kind"] == "interaction_matrix"
        assert aggregate["declaration_manifest"]["expected_cells"] == 2
        assert aggregate["declaration_manifest"]["pending_cells"] == 2

        # 2. A changed source manifest is a fingerprint mismatch: SKIP, never PASS.
        (case / "source-gpui.txt").write_text("gpui-source-changed\n", encoding="utf-8")
        code, aggregate = run(
            parser_args(
                parser,
                base
                + [
                    "--inputs",
                    str(case / "segments"),
                    "--source-manifest",
                    f"gpui={case / 'source-gpui.txt'}",
                    "--source-manifest",
                    f"iced={case / 'source-iced.txt'}",
                    "--out",
                    str(fixture["out"]),
                    "--verify",
                ],
            )
        )
        assert aggregate["frontends"]["gpui"]["fingerprint"] == "mismatch"
        assert aggregate["frontends"]["gpui"]["overall_status"] == "skip"
        assert aggregate["overall_status"] == "skip"
        assert aggregate["overall_status"] != "pass"
        assert code == 1, code
        assert any(item["code"] == "fingerprint_mismatch" for item in aggregate["findings"])
        # Report-only mode (no --verify) stays non-zero only for objective fails,
        # but it must still report skip, not pass.
        code, report_only = run(
            parser_args(
                parser,
                base
                + [
                    "--inputs",
                    str(case / "segments"),
                    "--source-manifest",
                    f"gpui={case / 'source-gpui.txt'}",
                    "--source-manifest",
                    f"iced={case / 'source-iced.txt'}",
                    "--out",
                    str(fixture["out"]),
                ],
            )
        )
        assert code == 0 and report_only["overall_status"] == "skip"

        # 3. No current fingerprint supplied -> unverified -> SKIP.
        code, aggregate = run(
            parser_args(
                parser,
                base
                + [
                    "--inputs",
                    str(case / "segments"),
                    "--source-manifest",
                    f"gpui={case / 'source-gpui.txt'}",
                    "--out",
                    str(fixture["out"]),
                    "--verify",
                ],
            )
        )
        assert aggregate["frontends"]["iced"]["fingerprint"] == "unverified"
        assert aggregate["frontends"]["iced"]["overall_status"] == "skip"
        assert aggregate["overall_status"] == "skip"
        assert code == 1, code

        # 4. Segment declaring a null digest -> fingerprint missing -> SKIP.
        null_segment = segment("gpui", "run-gpui-null", None, [
            {
                "case_id": "mc00-gpui",
                "test_id": "ui::tests::mc00_case_page",
                "outcome": "ok",
            }
        ])
        write_json(case / "segments" / "gpui.json", null_segment)
        code, aggregate = run(
            parser_args(
                parser,
                base
                + [
                    "--inputs",
                    str(case / "segments"),
                    "--source-manifest",
                    f"gpui={case / 'source-gpui.txt'}",
                    "--source-manifest",
                    f"iced={case / 'source-iced.txt'}",
                    "--out",
                    str(fixture["out"]),
                    "--verify",
                ],
            )
        )
        assert aggregate["frontends"]["gpui"]["fingerprint"] == "missing"
        assert aggregate["frontends"]["gpui"]["overall_status"] == "skip"
        assert code == 1, code

        # 5. Rebuild the fixture for the coverage/duplicate probes.
        fixture = interaction_case(root)
        case = fixture["case"]
        base = ["--repo-root", str(root), "--manifest", str(case / "declaration.tsv")]
        report_args = base + [
            "--inputs",
            str(case / "segments"),
            "--source-manifest",
            f"gpui={case / 'source-gpui.txt'}",
            "--source-manifest",
            f"iced={case / 'source-iced.txt'}",
            "--out",
            str(fixture["out"]),
        ]
        verify_args = report_args + ["--verify"]
        code, aggregate = run(parser_args(parser, verify_args))
        assert code == 0 and aggregate["overall_status"] == "pass"

        # 6. A declared cell without a run cell is an objective FAIL, even
        #    without --verify.
        write_json(
            case / "segments" / "gpui.json",
            segment("gpui", "run-gpui-empty", fixture["gpui_digest"], []),
        )
        code, aggregate = run(parser_args(parser, report_args))
        assert any(item["code"] == "declared_cell_uncovered" for item in aggregate["findings"])
        assert aggregate["overall_status"] == "fail"
        assert code == 1, code

        # 7. Duplicate run cells are a FAIL.
        write_json(
            case / "segments" / "gpui.json",
            segment(
                "gpui",
                "run-gpui-duplicate",
                fixture["gpui_digest"],
                [
                    {"case_id": "mc00-gpui", "test_id": "ui::tests::mc00_case_page", "outcome": "ok"},
                    {"case_id": "mc00-gpui", "test_id": "ui::tests::mc00_case_page", "outcome": "ok"},
                ],
            ),
        )
        code, aggregate = run(parser_args(parser, verify_args))
        assert aggregate["frontends"]["gpui"]["counts"]["duplicate_cells"] == 1
        assert any(item["code"] == "duplicate_run_cell" for item in aggregate["findings"])
        assert code == 1, code

        # 8. An undeclared run cell is a FAIL (no evidence for undeclared tests).
        write_json(
            case / "segments" / "gpui.json",
            segment(
                "gpui",
                "run-gpui-extra",
                fixture["gpui_digest"],
                [
                    {"case_id": "mc00-gpui", "test_id": "ui::tests::mc00_case_page", "outcome": "ok"},
                    {"case_id": "mc99-invented", "test_id": "ui::tests::invented", "outcome": "ok"},
                ],
            ),
        )
        code, aggregate = run(parser_args(parser, verify_args))
        assert any(item["code"] == "undeclared_run_cell" for item in aggregate["findings"])
        assert code == 1, code

        # 9. A pending declaration that suddenly has an outcome is drift: FAIL.
        write_json(
            case / "segments" / "gpui.json",
            segment(
                "gpui",
                "run-gpui-pending",
                fixture["gpui_digest"],
                [
                    {"case_id": "mc00-gpui", "test_id": "ui::tests::mc00_case_page", "outcome": "ok"},
                    {"case_id": "mc03-gpui", "test_id": "ui::tests::mc03_case_page", "outcome": "ok"},
                ],
            ),
        )
        code, aggregate = run(parser_args(parser, verify_args))
        assert any(item["code"] == "pending_declared_with_outcome" for item in aggregate["findings"])
        assert code == 1, code

        # 10. A run cell for a different test than the declared anchor is a FAIL.
        write_json(
            case / "segments" / "gpui.json",
            segment(
                "gpui",
                "run-gpui-swapped",
                fixture["gpui_digest"],
                [{"case_id": "mc00-gpui", "test_id": "ui::tests::other", "outcome": "ok"}],
            ),
        )
        code, aggregate = run(parser_args(parser, verify_args))
        assert any(item["code"] == "anchor_test_id_mismatch" for item in aggregate["findings"])
        assert code == 1, code

        # 11. --expect-frontend turns a missing segment into a FAIL.
        write_json(
            case / "segments" / "gpui.json",
            segment(
                "gpui",
                "run-gpui-restored",
                fixture["gpui_digest"],
                [{"case_id": "mc00-gpui", "test_id": "ui::tests::mc00_case_page", "outcome": "ok"}],
            ),
        )
        code, aggregate = run(parser_args(parser, verify_args + ["--expect-frontend", "tui"]))
        assert aggregate["coverage"]["expected_frontends"] == ["tui"]
        assert any(item["code"] == "expected_frontend_absent" for item in aggregate["findings"])
        assert code == 1, code

        # 12. Schema violations are structural errors (exit 2 semantics).
        broken = segment("gpui", "run-gpui-broken", fixture["gpui_digest"], [])
        del broken["run"]
        write_json(case / "segments" / "gpui.json", broken)
        try:
            run(parser_args(parser, verify_args))
            raise AssertionError("missing field must raise AggregateError")
        except AggregateError as error:
            assert "run" in str(error)

        broken = segment(
            "gpui",
            "run-gpui-broken-outcome",
            fixture["gpui_digest"],
            [{"case_id": "mc00-gpui", "outcome": "maybe"}],
        )
        write_json(case / "segments" / "gpui.json", broken)
        try:
            run(parser_args(parser, verify_args))
            raise AssertionError("unknown outcome must raise AggregateError")
        except AggregateError as error:
            assert "outcome" in str(error)

        (case / "segments" / "gpui.json").write_text("{not json", encoding="utf-8")
        try:
            run(parser_args(parser, verify_args))
            raise AssertionError("invalid JSON must raise AggregateError")
        except AggregateError as error:
            assert "JSON" in str(error)

        # 13. Two segments for one frontend are rejected.
        write_json(
            case / "segments" / "gpui.json",
            segment(
                "gpui",
                "run-gpui",
                fixture["gpui_digest"],
                [{"case_id": "mc00-gpui", "test_id": "ui::tests::mc00_case_page", "outcome": "ok"}],
            ),
        )
        write_json(
            case / "segments" / "gpui-again.json",
            segment(
                "gpui",
                "run-gpui-again",
                fixture["gpui_digest"],
                [{"case_id": "mc00-gpui", "test_id": "ui::tests::mc00_case_page", "outcome": "ok"}],
            ),
        )
        try:
            run(parser_args(parser, verify_args))
            raise AssertionError("duplicate frontend segment must raise AggregateError")
        except AggregateError as error:
            assert "duplicate frontend segment" in str(error)
        (case / "segments" / "gpui-again.json").unlink()

    # 14. Facet manifest: behavior + visual expectations, `unsupported` excluded.
    with tempfile.TemporaryDirectory(prefix="cross-frontend-manifest-facet-") as directory:
        root = Path(directory)
        (root / "facet.tsv").write_text(facet_declaration(), encoding="utf-8")
        (root / "source-gpui.txt").write_text("gpui-source\n", encoding="utf-8")
        digest = sha256_file(root / "source-gpui.txt")
        write_json(
            root / "segments" / "gpui.json",
            segment(
                "gpui",
                "run-gpui-facet",
                digest,
                [
                    {
                        "subject_kind": "facet",
                        "subject_id": "keyboard-shortcuts",
                        "test_id": "ui::tests::shortcuts",
                        "outcome": "ok",
                    },
                    {"scenario": "settings", "outcome": "pass", "receipt": "target/settings.png"},
                ],
            ),
        )
        code, aggregate = run(
            parser_args(
                build_parser(),
                [
                    "--repo-root",
                    str(root),
                    "--manifest",
                    "facet.tsv",
                    "--inputs",
                    str(root / "segments"),
                    "--source-manifest",
                    f"gpui={root / 'source-gpui.txt'}",
                    "--out",
                    str(root / "out" / "facet.json"),
                    "--verify",
                ],
            )
        )
        assert code == 0, (code, aggregate["findings"])
        assert aggregate["declaration_manifest"]["kind"] == "facet_manifest"
        assert aggregate["declaration_manifest"]["declared_cells"] == 3
        assert aggregate["declaration_manifest"]["expected_cells"] == 2
        assert aggregate["coverage"]["covered_cells"] == 2
        assert aggregate["frontends"]["gpui"]["counts"]["covered_cells"] == 2
        cells = aggregate["frontends"]["gpui"]["cells"]
        assert cells[0]["cell_key"] == "facet:keyboard-shortcuts"
        assert any(cell["cell_key"] == "visual:settings" for cell in cells)
        assert_aggregate_shape(json.loads(SCHEMA_PATH.read_text(encoding="utf-8")), aggregate)

    print("cross-frontend manifest self-test: PASS")


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Aggregate per-frontend run segments into one cross-frontend run manifest."
    )
    parser.add_argument("--repo-root", type=Path, default=Path.cwd())
    parser.add_argument(
        "--manifest",
        default=DEFAULT_MANIFEST,
        help=(
            "committed declaration list; auto-detects the unified interaction matrix "
            "(default: %(default)s) or the facet manifest"
        ),
    )
    parser.add_argument(
        "--inputs",
        action="append",
        default=[],
        metavar="PATH",
        help=(
            "per-frontend run segment (JSON), a directory of *.json segments, a glob, "
            "or FRONTEND=PATH; repeat for several frontends"
        ),
    )
    parser.add_argument(
        "--source-manifest",
        action="append",
        default=[],
        metavar="FRONTEND=PATH",
        help=(
            "current frontend-scoped source manifest from scripts/frontend_source_manifest.py; "
            "its sha256 is the fingerprint a segment must match (missing -> SKIP, never PASS)"
        ),
    )
    parser.add_argument(
        "--out",
        default=None,
        metavar="PATH",
        help="aggregate output path (wired flow: target/cross-frontend-evidence/<run_id>/manifest.json)",
    )
    parser.add_argument(
        "--verify",
        action="store_true",
        help="fail (exit 1) unless the aggregate status is pass",
    )
    parser.add_argument("--run-id", default=None, help="aggregation run id (default: UTC timestamp)")
    parser.add_argument(
        "--expect-frontend",
        action="append",
        default=[],
        choices=FRONTENDS,
        help="require a segment for this frontend (repeatable); an absent one is a FAIL",
    )
    parser.add_argument("--json", action="store_true", help="print the aggregate JSON to stdout")
    parser.add_argument("--self-test", action="store_true")
    return parser


def main(argv: list[str]) -> int:
    args = build_parser().parse_args(argv)
    if args.self_test:
        self_test()
        return 0
    try:
        code, aggregate = run(args)
    except AggregateError as error:
        print(f"cross-frontend manifest: ERROR {error}", file=sys.stderr)
        return 2
    out_path = resolve_path(Path(args.repo_root).resolve(), args.out)
    if args.json:
        print(json.dumps(aggregate, indent=2, sort_keys=True))
    else:
        print(format_summary(aggregate, out_path))
    return code


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
