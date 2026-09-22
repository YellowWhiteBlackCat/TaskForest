#!/usr/bin/env python3
"""Resolve cross-frontend evidence anchors against `cargo nextest list`.

This is the P4 ("evidence closure") Layer B resolver for the W3-C pilot.  It
reads the committed declaration manifest and checks that every declared
`behavior` anchor names a test that the environment actually discovers.

Contract
--------
* Reads committed declaration data (the manifest and, with
  `--interaction-matrix`, the unified interaction matrix) and environment
  discovery (`cargo nextest list --message-format json`, or the plain `--list`
  text).
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
* `platform` is the reserved P5 axis (empty field = frontend axis).  Like
  `contract_tag` it is an opaque field here: the resolver requires the column
  and accepts an empty value, it does not define the vocabulary.

Interaction matrix (`--interaction-matrix PATH`)
-----------------------------------------------
The unified `cross_frontend_matrix.tsv` carries interaction-case declarations
that used to live in three per-frontend matrices.  This resolver consumes it as
a second declaration source: one row per `(frontend, case_id)`:

* `subject_kind` must be `interaction` (new subject type; the resolver reports
  interaction entries with `subject_kind="interaction"` and never mixes them
  into the manifest's `facet` grid counts).
* `case_id` / `frontend` / `p0_id` / `target` (gui|lib) / `paths` /
  `capture_scenarios` keep their matrix semantics.
* `contract_tag` must equal the first `paths` token (the row's primary contract
  tag).  The vocabulary authority stays the Rust `ContractTag` enum; this
  resolver only checks the matrix-internal equality and never defines tags.
* `platform` stays the same reserved, opaque column as in the manifest.
* Anchor source recognition: a row with an explicit `test_name` resolves by
  exact nextest membership (iced/bevy rows).  A row whose `test_name` is `-`
  uses the owning frontend's stable case-prefix channel instead (GPUI:
  `<case_id>` with `-` normalized to `_`, plus `_case_`); the resolver accepts
  that channel only for the frontends in `CASE_PREFIX_CHANNELS`.  Either way a
  missing anchor is `dangling` (R4) and fails the run.

Scopes
------
* `--scope all` (default) evaluates every declared anchor and keeps the
  original semantics.
* `--scope auto` is the local-gate entry: it inspects the git diff since
  `--base` (default: merge-base with `origin/main`, else `HEAD~1`) plus
  untracked files, and short-circuits with `status=pass`, `"skipped": true`
  and no `cargo nextest list` when no evidence-relevant path changed.  A
  failed diff probe is evaluated fail-closed (never skipped) and reported in
  `scope.reason`.
* `--report-json PATH` is the canonical machine-readable report path for gate
  callers; `--report` is the original spelling of the same option.

Exit codes
----------
0  every declared anchor resolved, or the run was out of scope (skipped)
1  at least one dangling or invalid anchor
2  usage / IO / discovery error

The script is standard library only and read-only with respect to the repo
except for the optional machine-readable report it writes under `target/`.
"""

from __future__ import annotations

import argparse
import csv
import fnmatch
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
    # Reserved P5 axis: empty in the committed manifest (empty = frontend
    # axis).  The column exists so the three-dimensional ledger can land
    # without a schema break; no vocabulary is defined here.
    "platform",
)

# Fields that may legitimately be empty.  `reason` is required only for the
# non-ready statuses (a rule owned by the structural gate), and `platform` is
# the reserved axis above.
OPTIONAL_FIELDS = ("reason", "platform")

# Unified interaction matrix (`--interaction-matrix`).  One row per
# `(frontend, case_id)`.  The column set is the matrix schema plus the
# manifest's `contract_tag`/`platform` semantics; the resolver treats
# `contract_tag` as an opaque id and only checks the declared
# `contract_tag == first paths token` equality.
INTERACTION_FIELDS = (
    "subject_kind",
    "case_id",
    "frontend",
    "p0_id",
    "target",
    "test_name",
    "paths",
    "capture_scenarios",
    "contract_tag",
    "platform",
)

# The only subject_kind the unified matrix may declare today.  Reserved future
# kinds (for example `requirement`) must be added deliberately, together with
# their resolution rule, not silently accepted.
INTERACTION_SUBJECT_KIND = "interaction"

# Matrix `target` vocabulary.  It selects which nextest binary the anchor
# belongs to in the owning frontend, mirroring the per-frontend validators.
INTERACTION_TARGETS = {"gui", "lib"}

# Frontends whose matrix rows are allowed to leave `test_name` empty and declare
# the stable case-prefix channel instead of an explicit test id.  Any other
# frontend must name its test: a silent prefix convention there would be
# unverifiable.
CASE_PREFIX_CHANNELS = {"gpui"}

# The unified matrix keeps `p0_id` and `capture_scenarios` as declared data.
# `-` is the committed "no value" marker; `platform` may be empty (reserved).
INTERACTION_DASH_FIELDS = ("p0_id", "capture_scenarios")

EVIDENCE_KINDS = {"behavior", "visual", "none", "pending"}
STATUSES = {"ready", "partial", "missing", "unsupported", "pending"}
SCOPES = ("all", "auto")

# Paths that can move a declared anchor or the manifest's own contract.  A diff
# that touches none of them cannot change the resolution outcome, so the local
# gate may skip the (compiling) discovery walk.  `*` crosses directory
# separators here, matching the committed layout at any depth.
EVIDENCE_SCOPE_PATTERNS = (
    # The declaration manifest, this resolver, its self-test, and any route
    # artifact that lives in this directory.
    "scripts/parity/*",
    # Contract-tag authority and the Rust conformance reader of the manifest.
    "crates/taskmanager-ui-contract/src/conformance.rs",
    "crates/taskmanager-ui-contract/tests/*/ui_conformance.rs",
    # Feature-coverage declarations (shared contract registry plus every
    # per-frontend adapter) and their tests.
    "crates/*/feature_coverage*",
)

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


class ScopeDecision:
    """Whether the run must evaluate anchors, and why.

    A plain class rather than a dataclass: the self-test loads this module by
    path (without registering it in `sys.modules`), and dataclass annotation
    resolution requires it to be importable by name.
    """

    __slots__ = ("mode", "base", "relevant", "matched_paths", "reason")

    def __init__(
        self,
        mode: str,
        base: str | None,
        relevant: bool,
        matched_paths: tuple[str, ...],
        reason: str,
    ) -> None:
        self.mode = mode
        self.base = base
        self.relevant = relevant
        self.matched_paths = matched_paths
        self.reason = reason

    def as_dict(self) -> dict:
        return {
            "mode": self.mode,
            "base": self.base,
            "relevant": self.relevant,
            "matched_paths": list(self.matched_paths),
            "reason": self.reason,
        }


def evidence_relevant(path: str) -> bool:
    """True when a repo-relative path can move a declared evidence anchor."""
    normalised = path.strip().replace("\\", "/")
    if normalised.startswith("./"):
        normalised = normalised[2:]
    if not normalised:
        return False
    return any(
        fnmatch.fnmatchcase(normalised, pattern)
        for pattern in EVIDENCE_SCOPE_PATTERNS
    )


def evaluate_scope(
    paths: list[str] | tuple[str, ...],
    mode: str,
    base: str | None,
    git_error: str | None = None,
) -> ScopeDecision:
    """Pure scope decision: which changed paths matter, and must we run?

    Fail-closed: a failed diff probe (`git_error`) evaluates the anchors.
    """
    if mode == "all":
        return ScopeDecision(
            mode="all",
            base=None,
            relevant=True,
            matched_paths=(),
            reason="every declared anchor is evaluated",
        )
    matched = tuple(path for path in paths if evidence_relevant(path))
    if git_error:
        return ScopeDecision(
            mode="auto",
            base=base,
            relevant=True,
            matched_paths=matched,
            reason=f"changed-path discovery failed ({git_error}); evaluating fail-closed",
        )
    if not matched:
        return ScopeDecision(
            mode="auto",
            base=base,
            relevant=False,
            matched_paths=(),
            reason=f"no evidence-relevant change since {base}",
        )
    return ScopeDecision(
        mode="auto",
        base=base,
        relevant=True,
        matched_paths=matched,
        reason=f"{len(matched)} evidence-relevant path(s) since {base}",
    )


def _git(repo: Path, command: list[str]) -> subprocess.CompletedProcess:
    return subprocess.run(
        command,
        cwd=repo,
        check=False,
        capture_output=True,
        text=True,
        timeout=60,
    )


def default_scope_base(repo: Path) -> str:
    """Mirror the ui-route default: merge-base with origin/main, else HEAD~1."""
    try:
        probe = _git(repo, ["git", "rev-parse", "--verify", "origin/main"])
        if probe.returncode == 0:
            merge = _git(repo, ["git", "merge-base", "HEAD", "origin/main"])
            if merge.returncode == 0 and merge.stdout.strip():
                return merge.stdout.strip()
    except (OSError, subprocess.TimeoutExpired):
        pass
    return "HEAD~1"


def git_changed_paths(repo: Path, base: str) -> tuple[list[str], str | None]:
    """Repo-relative paths changed since `base`, including untracked files.

    Returns `(paths, error)`; a non-empty error means the probe failed and the
    caller must evaluate fail-closed.
    """
    paths: set[str] = set()
    commands = (
        ["git", "diff", "--name-only", base, "--"],
        ["git", "ls-files", "--others", "--exclude-standard", "--"],
    )
    for command in commands:
        try:
            proc = _git(repo, command)
        except (OSError, subprocess.TimeoutExpired) as exc:
            return sorted(paths), f"{' '.join(command)}: {exc}"
        if proc.returncode != 0:
            detail = (proc.stderr or proc.stdout or "").strip().splitlines()
            tail = detail[-1] if detail else f"exit {proc.returncode}"
            return sorted(paths), f"{' '.join(command)}: {tail}"
        paths.update(line.strip() for line in proc.stdout.splitlines() if line.strip())
    return sorted(paths), None


def scope_decision_for_run(args: argparse.Namespace, repo: Path) -> ScopeDecision:
    if args.scope == "all":
        return evaluate_scope((), "all", None)
    base = args.base or default_scope_base(repo)
    paths, error = git_changed_paths(repo, base)
    return evaluate_scope(paths, "auto", base, error)


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
            if row[field].strip() == "" and field not in OPTIONAL_FIELDS
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


def read_interaction_matrix(path: Path) -> list[dict[str, str]]:
    """Read the unified interaction matrix (`--interaction-matrix`).

    Structural rules only: the schema, the `interaction` subject kind, the
    frontend/target vocabulary the resolver can actually resolve against, the
    `contract_tag == first paths token` matrix-internal equality, and the
    per-frontend `(frontend, case_id)` key.  No contract-tag vocabulary and no
    capture-scenario resolution happen here; those stay with the Rust authority
    and the per-frontend capture validators.
    """
    if not path.is_file():
        raise ResolveError(f"interaction matrix not found: {path}")
    with path.open("r", encoding="utf-8", newline="") as handle:
        data_lines = [
            line for line in handle
            if line.strip() and not line.lstrip().startswith("#")
        ]
    reader = csv.DictReader(data_lines, delimiter="\t")
    if tuple(reader.fieldnames or ()) != INTERACTION_FIELDS:
        raise ResolveError(
            f"{path}: expected fields {INTERACTION_FIELDS}, got {reader.fieldnames}"
        )
    rows: list[dict[str, str]] = []
    seen: set[tuple[str, str]] = set()
    for lineno, row in enumerate(reader, start=2):
        if any(value is None for value in row.values()):
            raise ResolveError(f"{path}:{lineno}: malformed row (wrong field count)")
        missing = [
            field for field in INTERACTION_FIELDS
            if row[field].strip() == ""
            and field not in INTERACTION_DASH_FIELDS
            and field != "platform"
        ]
        if missing:
            raise ResolveError(f"{path}:{lineno}: empty field(s): {', '.join(missing)}")
        if row["subject_kind"].strip() != INTERACTION_SUBJECT_KIND:
            raise ResolveError(
                f"{path}:{lineno}: unknown subject_kind {row['subject_kind']!r} "
                f"(the unified matrix only declares {INTERACTION_SUBJECT_KIND!r})"
            )
        if row["frontend"].strip() not in FRONTEND_PACKAGES:
            raise ResolveError(
                f"{path}:{lineno}: unknown frontend {row['frontend']!r}"
            )
        if row["target"].strip() not in INTERACTION_TARGETS:
            raise ResolveError(f"{path}:{lineno}: unknown target {row['target']!r}")
        test_name = row["test_name"].strip()
        if test_name == "-" and row["frontend"].strip() not in CASE_PREFIX_CHANNELS:
            raise ResolveError(
                f"{path}:{lineno}: frontend {row['frontend']!r} has no stable "
                "case-prefix channel; test_name must name its anchor"
            )
        paths = [token for token in row["paths"].split("|") if token]
        if not paths:
            raise ResolveError(f"{path}:{lineno}: empty paths declaration")
        if row["contract_tag"].strip() != paths[0]:
            raise ResolveError(
                f"{path}:{lineno}: contract_tag {row['contract_tag']!r} is not the "
                f"first paths token {paths[0]!r}"
            )
        key = (row["frontend"].strip(), row["case_id"].strip())
        if key in seen:
            raise ResolveError(
                f"{path}:{lineno}: duplicate (frontend, case_id) cell: {key[0]}/{key[1]}"
            )
        seen.add(key)
        rows.append(row)
    if not rows:
        raise ResolveError(f"{path}: interaction matrix has no data rows")
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


def skipped_report(
    manifest_path: Path,
    decision: ScopeDecision,
    interaction_path: Path | None = None,
) -> dict:
    """Report for an `--scope auto` run whose diff cannot move an anchor.

    Shaped like a resolved report so every consumer can read one schema; no
    frontend was discovered and no anchor was evaluated.
    """
    zero_counts = {
        "cells": 0,
        "anchored_behavior": 0,
        "anchored_visual": 0,
        "pending": 0,
        "none": 0,
        "dangling": 0,
        "invalid": 0,
        "visual_unverified": 0,
        "interaction_cells": 0,
        "interaction_anchored": 0,
        "interaction_dangling": 0,
    }
    return {
        "manifest": str(manifest_path),
        "interaction_matrix": {
            "path": str(interaction_path) if interaction_path else None,
            "cases": 0,
            "anchored": 0,
            "dangling": 0,
        },
        "generated_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "status": "pass",
        "skipped": True,
        "scope": decision.as_dict(),
        "counts": zero_counts,
        "frontends": {},
        "dangling": [],
        "invalid": [],
        "visual_unverified": [],
        "pending": [],
    }


def resolve(args: argparse.Namespace) -> dict:
    repo = Path(args.repo).resolve()
    manifest_path = Path(args.manifest)
    if not manifest_path.is_absolute():
        manifest_path = repo / manifest_path
    rows = read_manifest(manifest_path)

    interaction_path = getattr(args, "interaction_matrix", None)
    interaction_path = Path(interaction_path) if interaction_path else None
    if interaction_path is not None and not interaction_path.is_absolute():
        interaction_path = repo / interaction_path
    interaction_rows = (
        read_interaction_matrix(interaction_path) if interaction_path else []
    )

    decision = scope_decision_for_run(args, repo)
    if not decision.relevant:
        return skipped_report(manifest_path, decision, interaction_path)

    provided = split_pairs(args.discovery, "discovery")
    unknown = sorted(set(provided) - set(FRONTEND_PACKAGES))
    if unknown:
        raise ResolveError(f"unknown frontend(s) in --discovery: {', '.join(unknown)}")

    scenarios = split_pairs(args.capture_scenarios, "capture-scenarios")

    required_frontends = sorted(
        {row["frontend"] for row in rows}
        | {row["frontend"] for row in interaction_rows}
    )
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

    # Unified interaction matrix: each row is an `interaction` subject whose
    # anchor resolves against the owning frontend's discovery.  Rows with an
    # explicit test_name use exact membership; rows on a frontend with the
    # stable case-prefix channel resolve through `<case_id>_case_*`.
    interaction_cells = interaction_anchored = interaction_dangling = 0
    for row in interaction_rows:
        interaction_cells += 1
        interaction_anchored += 1
        frontend = row["frontend"]
        test_name = row["test_name"]
        if test_name == "-":
            prefix = f"{row['case_id'].replace('-', '_')}_case_"
            if not any(
                name.rsplit("::", 1)[-1].startswith(prefix)
                for name in discovered[frontend]
            ):
                interaction_dangling += 1
                dangling.append({
                    "reason": (
                        "no discovered test advertises the stable case prefix "
                        "(R4, case-prefix channel)"
                    ),
                    "subject_kind": INTERACTION_SUBJECT_KIND,
                    "subject_id": row["case_id"],
                    "frontend": frontend,
                    "test_id": f"{prefix}*",
                    "channel": "case-prefix",
                })
        elif test_name not in discovered[frontend]:
            interaction_dangling += 1
            dangling.append({
                "reason": "anchor not discovered by cargo nextest list (R4)",
                "subject_kind": INTERACTION_SUBJECT_KIND,
                "subject_id": row["case_id"],
                "frontend": frontend,
                "test_id": test_name,
                "channel": "test-name",
            })

    status = "pass" if not dangling and not invalid else "fail"
    return {
        "manifest": str(manifest_path),
        "interaction_matrix": {
            "path": str(interaction_path) if interaction_path else None,
            "cases": interaction_cells,
            "anchored": interaction_anchored,
            "dangling": interaction_dangling,
        },
        "generated_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "status": status,
        "skipped": False,
        "scope": decision.as_dict(),
        "counts": {
            "cells": cells,
            "anchored_behavior": anchored_behavior,
            "anchored_visual": anchored_visual,
            "pending": pending,
            "none": none,
            "dangling": len(dangling),
            "invalid": len(invalid),
            "visual_unverified": len(visual_unverified),
            "interaction_cells": interaction_cells,
            "interaction_anchored": interaction_anchored,
            "interaction_dangling": interaction_dangling,
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
        "--interaction-matrix",
        default=None,
        metavar="PATH",
        help=(
            "unified interaction matrix (S4) to resolve as a second declaration "
            "source; each row's anchor is checked against the owning frontend's "
            "discovery"
        ),
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
    parser.add_argument(
        "--scope",
        choices=SCOPES,
        default="all",
        help=(
            "all evaluates every declared anchor (default); auto short-circuits "
            "when the diff since --base carries no evidence-relevant path"
        ),
    )
    parser.add_argument(
        "--base",
        default=None,
        help=(
            "diff base for --scope auto (default: merge-base with origin/main, "
            "else HEAD~1)"
        ),
    )
    parser.add_argument("--repo", default=".", help="repository root (default: cwd)")
    parser.add_argument(
        "--report", "--report-json",
        dest="report",
        default=None,
        help="report JSON path (--report-json is the canonical gate spelling)",
    )
    parser.add_argument("--no-report", action="store_true", help="do not write a report file")
    parser.add_argument("--json", action="store_true", help="print the report JSON to stdout")
    return parser


def format_summary(report: dict) -> str:
    counts = report["counts"]
    scope = report.get("scope") or {}
    lines = [
        f"manifest: {report['manifest']}",
        f"status:   {report['status'].upper()}"
        + (" (skipped)" if report.get("skipped") else ""),
    ]
    if scope:
        base = scope.get("base") or "-"
        lines.append(
            "scope:    {mode} (base={base}): {reason}".format(
                mode=scope.get("mode", "?"), base=base, reason=scope.get("reason", "")
            )
        )
    if report.get("skipped"):
        lines.append("          no test discovery was run; nothing to resolve")
        return "\n".join(lines)
    interaction = report.get("interaction_matrix") or {}
    # `counts.dangling` is the shared total (manifest + interaction matrix); the
    # facet line shows the manifest share so the two sources stay readable.
    interaction_dangling = int(interaction.get("dangling") or 0)
    facet_dangling = counts.get("dangling", 0) - interaction_dangling
    lines.append(
        (
            "cells:    {cells} manifest cells | {anchored_behavior} behavior anchors | "
            "{pending} pending | {none} none | {facet_dangling} dangling | {invalid} invalid"
        ).format(facet_dangling=facet_dangling, **counts)
    )
    if interaction.get("cases"):
        lines.append(
            "matrix:   {cases} interaction cases | {anchored} anchored | "
            "{dangling} dangling  ({path})".format(**interaction)
        )
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
