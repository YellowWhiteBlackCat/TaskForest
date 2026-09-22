#!/usr/bin/env python3
"""Unified cross-frontend interaction acceptance driver (S5 step 1).

What this is
------------
The evidence-closure plan's S5 "one driver + per-frontend thin wrapper" entry
point.  It is **additive**: it retires no asset and changes no existing gate.
Per selected frontend it

1. records the frontend-scoped source fingerprint through
   ``scripts/frontend_source_manifest.py`` (accept mode only);
2. invokes the existing ``scripts/accept-<frontend>-interactions.sh`` gate under
   an external ``timeout --kill-after=...`` (accept mode).  The only cargo
   invocations remain the ones those gates already own;
3. reads the fresh native evidence directory and writes **exactly one**
   ``frontend_run_segment`` JSON per frontend under
   ``target/cross-frontend-evidence/<run_id>/segments/<frontend>.json`` (the
   segment contract is ``scripts/parity/run_manifest.schema.json``);
4. re-resolves Layer B (R4/R6) with
   ``scripts/parity/resolve_frontend_evidence.py`` against the same discovery
   artifacts;
5. folds the segments with ``scripts/quality/cross_frontend_manifest.py`` and,
   unless ``--report-only`` is passed, verifies the aggregate.

Discipline
----------
* The committed declaration list (``--declaration``, default
  ``scripts/parity/cross_frontend_matrix.tsv``) stays the single authority.  This
  driver declares nothing, defines no vocabulary, and never derives an anchor by
  scanning Rust source: an anchor is a hand-declared test id and the resolver
  checks discovery membership.
* The declaration parser, the discovery parser, the pending marker and the
  stable case-prefix channel come from ``resolve_frontend_evidence.py`` (one
  address), never a second copy.
* Every external command is launched under ``timeout --kill-after=30s`` with a
  finite Python backstop; there are no background children and no pipelines.  A
  frontend is only ``ok`` when its native gate actually ran and the run events
  covered the declared cell.
* Fingerprints are fail-closed: a segment declares ``source_manifest_sha256 =
  null`` when no fingerprint was recorded, which the aggregator folds to SKIP --
  never PASS.  ``--from-existing`` (alias ``--dry-run``) therefore rehearses the
  full segment -> aggregate -> verify chain from replayed artifacts without
  running cargo and without manufacturing a fresh-evidence PASS: a replayed run
  id carries the ``rehearsal`` marker and the default ``--fingerprint null``
  makes the aggregate SKIP.

Modes
-----
* default (accept): run the selected per-frontend gates, one at a time.
* ``--from-existing`` / ``--dry-run``: no cargo at all.  Segments are built from
  caller-supplied discovery artifacts, native evidence directories and run logs
  (``--discovery``, ``--native-evidence``, ``--run-events``).  This is the
  no-GUI end-to-end rehearsal path.

Exit codes
----------
0  every selected frontend verified; the aggregate is ``pass``
1  verification not ``pass`` (fail, or skip under ``--verify``), resolver
   dangling, or a native gate failed
2  usage / IO / structural error

Run
---
``bash scripts/parity/accept-frontend-interactions.sh [FRONTEND ...] [options]``
or ``python3 scripts/parity/accept_frontend_interactions.py --self-test``.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
import re
import shutil
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path

HERE = Path(__file__).resolve().parent
DEFAULT_REPO_ROOT = HERE.parents[1]
RESOLVER_PATH = HERE / "resolve_frontend_evidence.py"
AGGREGATOR_PATH = HERE.parent / "quality" / "cross_frontend_manifest.py"
SOURCE_MANIFEST_PATH = HERE.parent / "frontend_source_manifest.py"

DEFAULT_DECLARATION = "scripts/parity/cross_frontend_matrix.tsv"
DEFAULT_EVIDENCE_ROOT = "target/cross-frontend-evidence"
DEFAULT_FACET_MANIFEST = "scripts/parity/cross_frontend_manifest.tsv"
DEFAULT_FEATURE_EVIDENCE = "scripts/parity/feature_evidence.tsv"
DEFAULT_REQUIREMENTS = "scripts/interaction_requirements.tsv"
DEFAULT_SEGMENTS_DIR = "segments"
DEFAULT_DISCOVERY_DIR = "discovery"
DEFAULT_NATIVE_DIR = "native"

SCHEMA_VERSION = 1
SEGMENT_RECORD_KIND = "frontend_run_segment"
DRIVER_RECORD_KIND = "accept_frontend_interactions_driver"
REHEARSAL_MARKER = "rehearsal"
FRONTENDS = ("gpui", "iced", "tui", "bevy")

TIMEOUT_KILL_AFTER = "30s"
TIMEOUT_BACKSTOP_SLOP_S = 30.0
DEFAULT_ACCEPT_DEADLINE = "90m"
DEFAULT_SOURCE_MANIFEST_DEADLINE = "5m"
DEFAULT_DISCOVERY_DEADLINE = "20m"
DEFAULT_RESOLVER_DEADLINE = "5m"
DEFAULT_AGGREGATOR_DEADLINE = "2m"

INTERACTION_PENDING = "pending"
INTERACTION_PREFIX_CHANNEL = "case-prefix"
INTERACTION_ANCHOR_CHANNEL = "test-name"


class DriverError(RuntimeError):
    """Fatal usage, IO, or structural error; reported with exit code 2."""


def load_module(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise DriverError(f"cannot load {name} from {path}")
    module = importlib.util.module_from_spec(spec)
    # Register before exec: dataclass processing looks the module up by name.
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


# Single authority for declaration parsing, discovery parsing, the pending
# marker and the stable case-prefix channel.  Importing the resolver instead of
# re-implementing them keeps this driver free of a second vocabulary.
resolver = load_module("resolve_frontend_evidence", RESOLVER_PATH)


# ---------------------------------------------------------------------------
# Per-frontend native adapters (read-only transcription of the accept scripts)
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class Adapter:
    """How one frontend's existing accept gate reports discovery and runs."""

    frontend: str
    accept_script: str
    native_root: str
    discovery_files: tuple[str, ...]
    run_log_files: tuple[str, ...]
    discovery_command: str
    run_command: str
    discovery_from_accept: bool


ADAPTERS: dict[str, Adapter] = {
    "gpui": Adapter(
        frontend="gpui",
        accept_script="scripts/accept-gpui-interactions.sh",
        native_root="target/gpui-interaction-evidence",
        discovery_files=("gui-list.json", "lib-list.json"),
        run_log_files=("gui-run.log", "lib-run.log"),
        discovery_command=(
            "cargo nextest list --locked --profile ci -p taskmanager-gpui "
            "--test gui --features test-support --message-format json (plus the "
            "--lib list; the segment folds both gui and lib targets)"
        ),
        run_command=(
            "NEXTEST_EXPERIMENTAL_LIBTEST_JSON=1 cargo nextest run --locked "
            "--profile ci --message-format libtest-json-plus --cargo-quiet "
            "--no-fail-fast -j 4 -p taskmanager-gpui --test gui --features test-support "
            "(plus the --lib run; the segment folds both gui and lib targets)"
        ),
        discovery_from_accept=True,
    ),
    "iced": Adapter(
        frontend="iced",
        accept_script="scripts/accept-iced-interactions.sh",
        native_root="target/iced-interaction-evidence",
        discovery_files=("lib-list.json",),
        run_log_files=("lib-run.log",),
        discovery_command=(
            "cargo nextest list --locked --profile ci -p taskmanager-iced --lib "
            "--message-format json"
        ),
        run_command=(
            "NEXTEST_EXPERIMENTAL_LIBTEST_JSON=1 cargo nextest run --locked "
            "--profile ci --message-format libtest-json-plus --cargo-quiet -j 4 "
            "-p taskmanager-iced --lib"
        ),
        discovery_from_accept=True,
    ),
    "tui": Adapter(
        frontend="tui",
        accept_script="scripts/accept-tui-interactions.sh",
        native_root="target/tui-interaction-evidence",
        discovery_files=(),
        run_log_files=("nextest.log",),
        discovery_command=(
            "cargo nextest list --locked -p taskmanager-tui --message-format json"
        ),
        run_command="cargo nextest run --locked -p taskmanager-tui -j 4",
        # The TUI accept gate runs the package without emitting a discovery
        # artifact; the driver owns the one bounded list (documented cost delta,
        # retired with the S5 wrapper wave).
        discovery_from_accept=False,
    ),
    "bevy": Adapter(
        frontend="bevy",
        accept_script="scripts/accept-bevy-interactions.sh",
        native_root="target/bevy-interaction-evidence",
        discovery_files=("discovery.txt",),
        run_log_files=("nextest.log",),
        discovery_command="cargo nextest list --locked -p taskmanager-bevy-ui --lib",
        run_command=(
            "cargo nextest run --locked -p taskmanager-bevy-ui --lib -j 4 --no-fail-fast"
        ),
        discovery_from_accept=True,
    ),
}


# ---------------------------------------------------------------------------
# Bounded external commands
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class CommandResult:
    command: tuple[str, ...]
    exit_code: int | None
    timed_out: bool
    log: str | None


DEADLINE_PATTERN = re.compile(r"^\s*(?P<value>\d+)\s*(?P<unit>s|m|h)?\s*$")


def parse_deadline(text: str) -> float:
    """Parse `30s` / `20m` / `2h` into seconds; reject anything else."""

    match = DEADLINE_PATTERN.match(text)
    if match is None:
        raise DriverError(f"malformed deadline {text!r} (expected e.g. 90m, 20m, 30s)")
    value = int(match.group("value"))
    unit = match.group("unit") or "s"
    return float(value) * {"s": 1.0, "m": 60.0, "h": 3600.0}[unit]


def with_timeout(deadline: str, command: list[str]) -> list[str]:
    """Wrap an external command in coreutils `timeout --kill-after=...`."""

    return ["timeout", f"--kill-after={TIMEOUT_KILL_AFTER}", deadline, *command]


def run_command(
    command: list[str],
    *,
    cwd: Path,
    timeout_s: float,
    env: dict[str, str] | None = None,
    log_path: Path | None = None,
) -> CommandResult:
    """Run one bounded external command; never background, never a pipeline."""

    handle = None
    if log_path is not None:
        log_path.parent.mkdir(parents=True, exist_ok=True)
        handle = log_path.open("w", encoding="utf-8")
    try:
        completed = subprocess.run(
            list(command),
            cwd=str(cwd),
            env=env,
            stdout=handle if handle is not None else subprocess.PIPE,
            stderr=subprocess.STDOUT if handle is not None else subprocess.PIPE,
            text=True,
            timeout=timeout_s,
            check=False,
        )
    except subprocess.TimeoutExpired:
        return CommandResult(tuple(command), None, True, str(log_path) if log_path else None)
    finally:
        if handle is not None:
            handle.close()
    return CommandResult(
        tuple(command),
        completed.returncode,
        False,
        str(log_path) if log_path else None,
    )


def run_capture_stdout(
    command: list[str],
    *,
    cwd: Path,
    timeout_s: float,
    env: dict[str, str] | None,
    stdout_path: Path,
    stderr_path: Path,
) -> CommandResult:
    """Run a command whose stdout is a machine-readable artifact."""

    stdout_path.parent.mkdir(parents=True, exist_ok=True)
    stderr_path.parent.mkdir(parents=True, exist_ok=True)
    with stdout_path.open("w", encoding="utf-8") as out_handle, stderr_path.open(
        "w", encoding="utf-8"
    ) as err_handle:
        try:
            completed = subprocess.run(
                list(command),
                cwd=str(cwd),
                env=env,
                stdout=out_handle,
                stderr=err_handle,
                text=True,
                timeout=timeout_s,
                check=False,
            )
        except subprocess.TimeoutExpired:
            return CommandResult(tuple(command), None, True, str(stderr_path))
    return CommandResult(tuple(command), completed.returncode, False, str(stderr_path))


# ---------------------------------------------------------------------------
# Small helpers
# ---------------------------------------------------------------------------


def sha256_file(path: Path) -> str:
    hasher = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            hasher.update(chunk)
    return hasher.hexdigest()


def utc_now() -> str:
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())


def resolve_path(repo: Path, value: str | Path) -> Path:
    path = Path(value)
    return path if path.is_absolute() else repo / path


def display_path(path: Path, repo: Path) -> str:
    try:
        return path.resolve().relative_to(repo.resolve()).as_posix()
    except ValueError:
        return path.as_posix()


def parse_pairs(values: list[str], label: str) -> dict[str, Path]:
    pairs: dict[str, Path] = {}
    for raw in values:
        key, separator, value = raw.partition("=")
        if not separator or key not in FRONTENDS or not value:
            raise DriverError(f"--{label} expects FRONTEND=PATH, got: {raw}")
        if key in pairs:
            raise DriverError(f"--{label} repeats frontend {key}: {raw}")
        pairs[key] = Path(value)
    return pairs


def git_capture(repo: Path, arguments: list[str]) -> tuple[int, str]:
    result = subprocess.run(
        ["git", "-C", str(repo), *arguments],
        cwd=str(repo),
        capture_output=True,
        text=True,
        timeout=60.0,
        check=False,
    )
    return result.returncode, result.stdout


def git_state(repo: Path) -> tuple[str | None, str]:
    """Best-effort git head + worktree state; never fatal, never guessed."""

    head: str | None = None
    worktree = "unknown"
    head_code, head_text = git_capture(repo, ["rev-parse", "--short=12", "HEAD"])
    if head_code == 0 and head_text.strip():
        head = head_text.strip()
    status_code, status_text = git_capture(repo, ["status", "--porcelain"])
    if status_code == 0:
        worktree = "dirty" if status_text.strip() else "clean"
    return head, worktree


def rustc_version(repo: Path) -> str | None:
    result = subprocess.run(
        ["rustc", "-V"],
        cwd=str(repo),
        capture_output=True,
        text=True,
        timeout=60.0,
        check=False,
    )
    if result.returncode != 0:
        return None
    version = result.stdout.strip()
    return version or None


def write_json(path: Path, payload: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")


# ---------------------------------------------------------------------------
# Run-event extraction: libtest-json-plus, the committed event<TAB>name shape,
# and nextest's human log.  Outcomes are never invented: a test id without an
# event is `skipped`, which the aggregator can never read as a pass.
# ---------------------------------------------------------------------------

OK_EVENTS = frozenset({"ok", "pass", "PASS", "SLOW"})
FAIL_EVENTS = frozenset(
    {"failed", "fail", "FAIL", "TIMEOUT", "SIGSEGV", "SIGABRT", "ABORT", "LEAK", "error"}
)
SKIP_EVENTS = frozenset({"ignored", "skipped", "SKIP", "IGNORED"})


def event_outcome(kind: object) -> str | None:
    if not isinstance(kind, str):
        return None
    if kind in OK_EVENTS:
        return "ok"
    if kind in FAIL_EVENTS:
        return "fail"
    if kind in SKIP_EVENTS:
        return "skipped"
    return None


def parse_jsonl_events(text: str) -> dict[str, str]:
    """`libtest-json-plus` lines: the last event per test id wins (retries)."""

    outcomes: dict[str, str] = {}
    for raw in text.splitlines():
        line = raw.strip()
        if not line.startswith("{"):
            continue
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            continue
        if not isinstance(event, dict) or event.get("type") != "test":
            continue
        name = event.get("name")
        if not isinstance(name, str) or "$" not in name:
            continue
        outcome = event_outcome(event.get("event"))
        if outcome is not None:
            outcomes[name.split("$", 1)[1]] = outcome
    return outcomes


NEXTEST_STATUS_LINE = re.compile(
    r"^\s*(?P<status>[A-Z][A-Z]+)"
    r"(?:\s+(?P<attempt>\d+))?"
    r"(?:\s+(?P<final>[A-Z][A-Z]+))?"
    r"\s+\[\s*>?\s*(?P<seconds>[0-9]+(?:\.[0-9]+)?)s\s*\]"
    r"\s+(?P<payload>.+?)\s*$"
)
NEXTEST_COUNTER_PREFIX = re.compile(r"^\(\s*\d+\s*/\s*\d+\s*\)\s*")


def parse_nextest_human_log(text: str) -> dict[str, str]:
    """Nextest's human status lines (`PASS [ 0.004s] (1/2) bin test::name`)."""

    outcomes: dict[str, str] = {}
    for raw in text.splitlines():
        match = NEXTEST_STATUS_LINE.match(raw)
        if match is None:
            continue
        final = match.group("final")
        if final is not None:
            word = final
        elif match.group("attempt") is not None:
            # `TRY n` without a verdict carries no outcome.
            continue
        else:
            word = match.group("status")
        outcome = event_outcome(word)
        if outcome is None:
            continue
        payload = NEXTEST_COUNTER_PREFIX.sub("", match.group("payload"))
        tokens = payload.split()
        if not tokens:
            continue
        test_id = tokens[-1]
        if "::" not in test_id:
            continue
        outcomes[test_id] = outcome
    return outcomes


def parse_event_table(text: str) -> dict[str, str]:
    """The committed `accept-iced-interactions.sh` shape: `event<TAB>test-name`."""

    outcomes: dict[str, str] = {}
    for raw in text.splitlines():
        line = raw.strip()
        if not line or "\t" not in line:
            continue
        word, _, test_id = line.partition("\t")
        test_id = test_id.strip()
        if not test_id:
            continue
        outcome = event_outcome(word.strip())
        if outcome is not None:
            outcomes[test_id] = outcome
    return outcomes


# ---------------------------------------------------------------------------
# Declaration reading
# ---------------------------------------------------------------------------


def declaration_kind(path: Path) -> str:
    if not path.is_file():
        raise DriverError(f"declaration list not found: {path}")
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        fields = [field.strip() for field in line.split("\t")]
        if "case_id" in fields:
            return "interaction_matrix"
        if "evidence_kind" in fields:
            return "facet_manifest"
        break
    raise DriverError(
        f"declaration {path} is neither an interaction matrix (needs case_id) "
        "nor a facet manifest (needs evidence_kind)"
    )


def read_declaration_rows(path: Path, kind: str) -> list[dict[str, str]]:
    try:
        if kind == "interaction_matrix":
            return resolver.read_interaction_matrix(path)
        return resolver.read_manifest(path)
    except resolver.ResolveError as error:
        raise DriverError(f"declaration rejected by the resolver parser: {error}") from error


# ---------------------------------------------------------------------------
# Segment cells
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class CellResult:
    cell: dict
    note: str | None = None


def interaction_cell(
    row: dict[str, str], discovered: set[str], outcomes: dict[str, str]
) -> CellResult | None:
    """One declared interaction row -> one run cell (None for `pending`)."""

    test_name = row["test_name"].strip()
    if test_name == INTERACTION_PENDING:
        return None
    case_id = row["case_id"].strip()
    paths = [token for token in row["paths"].split("|") if token]
    if test_name == "-":
        channel = INTERACTION_PREFIX_CHANNEL
        prefix = f"{case_id.replace('-', '_')}_case_"
        candidates = sorted(
            name for name in discovered if name.rsplit("::", 1)[-1].startswith(prefix)
        )
        hint = prefix
    else:
        channel = INTERACTION_ANCHOR_CHANNEL
        candidates = [test_name] if test_name in discovered else []
        hint = test_name
    if not candidates:
        return CellResult(
            {
                "subject_kind": row["subject_kind"],
                "case_id": case_id,
                "contract_tag": row["contract_tag"],
                "test_id": hint,
                "paths": paths,
                "outcome": "error",
            },
            note=f"{case_id}: no discovered test advertises the declared anchor ({channel})",
        )
    statuses = [outcomes.get(name, "skipped") for name in candidates]
    if all(status == "ok" for status in statuses):
        outcome = "ok"
    elif any(status == "fail" for status in statuses):
        outcome = "fail"
    else:
        outcome = "skipped"
    note = None
    if len(candidates) > 1:
        note = (
            f"{case_id}: {len(candidates)} tests match the {channel} anchor; "
            "folded into one cell"
        )
    return CellResult(
        {
            "subject_kind": row["subject_kind"],
            "case_id": case_id,
            "contract_tag": row["contract_tag"],
            "test_id": candidates[0],
            "paths": paths,
            "outcome": outcome,
        },
        note=note,
    )


def facet_cell(
    row: dict[str, str], discovered: set[str], outcomes: dict[str, str]
) -> CellResult | None:
    """One declared facet row -> one run cell (None for `pending`/`none`)."""

    evidence_kind = row["evidence_kind"].strip()
    if evidence_kind in ("pending", "none"):
        return None
    contract_tag = row["contract_tag"].strip()
    if evidence_kind == "visual":
        # Visual cells belong to the retained capture validators; the run
        # manifest records them as an honest, non-coverable gap here.
        return CellResult(
            {
                "subject_kind": row["subject_kind"],
                "subject_id": row["subject_id"],
                "scenario": row["test_id_or_scenario"],
                "contract_tag": contract_tag,
                "outcome": "skipped",
            },
            note=f"{row['subject_id']}: visual capture cell is owned by the capture validators",
        )
    anchor = row["test_id_or_scenario"].strip()
    if anchor not in discovered:
        return CellResult(
            {
                "subject_kind": row["subject_kind"],
                "subject_id": row["subject_id"],
                "contract_tag": contract_tag,
                "test_id": anchor,
                "outcome": "error",
            },
            note=f"{row['subject_id']}: anchor {anchor} is not in discovery",
        )
    return CellResult(
        {
            "subject_kind": row["subject_kind"],
            "subject_id": row["subject_id"],
            "contract_tag": contract_tag,
            "test_id": anchor,
            "outcome": outcomes.get(anchor, "skipped"),
        }
    )


def build_cells(
    kind: str,
    frontend: str,
    rows: list[dict[str, str]],
    discovered: set[str],
    outcomes: dict[str, str],
) -> tuple[list[dict], list[str]]:
    cells: list[dict] = []
    notes: list[str] = []
    for row in rows:
        if row["frontend"].strip() != frontend:
            continue
        if kind == "interaction_matrix":
            result = interaction_cell(row, discovered, outcomes)
        else:
            result = facet_cell(row, discovered, outcomes)
        if result is None:
            continue
        cells.append(result.cell)
        if result.note:
            notes.append(result.note)
    return cells, notes


def segment_status(cells: list[dict], discovery_result: str, run_result: str) -> str:
    if discovery_result in ("fail", "error") or run_result in ("fail", "error"):
        return "fail"
    if any(cell["outcome"] in ("fail", "error") for cell in cells):
        return "fail"
    if run_result == "skipped" or any(cell["outcome"] == "skipped" for cell in cells):
        return "skip"
    return "pass"


def build_segment(
    *,
    frontend: str,
    run_id: str,
    cells: list[dict],
    discovery_record: dict,
    run_record: dict,
    digest: str | None,
    source_manifest_path: str | None,
    git_head: str | None,
    worktree: str,
    rust: str | None,
    receipts: list[str],
) -> dict:
    segment: dict = {
        "schema_version": SCHEMA_VERSION,
        "record_kind": SEGMENT_RECORD_KIND,
        "frontend": frontend,
        "run_id": run_id,
        "generated_at": utc_now(),
        "source_manifest_sha256": digest,
        "discovery": discovery_record,
        "run": run_record,
        "cells": cells,
        "overall_status": segment_status(cells, discovery_record["result"], run_record["result"]),
    }
    if git_head is not None:
        segment["git_head"] = git_head
    segment["worktree"] = worktree
    if rust is not None:
        segment["rust"] = rust
    if source_manifest_path is not None:
        segment["source_manifest_path"] = source_manifest_path
    if receipts:
        segment["receipts"] = receipts
    return segment


def event_counts(outcomes: dict[str, str]) -> dict[str, int]:
    return {
        "events": len(outcomes),
        "ok_events": sum(1 for value in outcomes.values() if value == "ok"),
        "failed_events": sum(1 for value in outcomes.values() if value == "fail"),
    }


# ---------------------------------------------------------------------------
# Driver
# ---------------------------------------------------------------------------


@dataclass
class FrontendOutcome:
    frontend: str
    segment_path: Path
    status: str
    warnings: list[str]


def preflight() -> None:
    required = ("timeout", "bash", "git", "cargo", "rustc")
    missing = [name for name in required if shutil.which(name) is None]
    if missing:
        raise DriverError("required command is unavailable: " + ", ".join(missing))


def default_run_id(repo: Path, mode: str) -> str:
    stamp = time.strftime("%Y%m%dT%H%M%SZ", time.gmtime())
    head, worktree = git_state(repo)
    marker = "" if mode == "accept" else f"_{REHEARSAL_MARKER}"
    return f"{stamp}_{head or 'no-git'}_{worktree}{marker}_{os.getpid()}"


def load_discovery_names(path: Path) -> set[str]:
    try:
        return resolver.load_discovery(path)
    except resolver.DiscoveryError as error:
        raise DriverError(str(error)) from error


def load_run_events(path: Path) -> dict[str, str]:
    """Auto-detect a run-event artifact: JSON lines, event table, or human log."""

    if not path.is_file():
        raise DriverError(f"run-event artifact not found: {path}")
    text = path.read_text(encoding="utf-8", errors="replace")
    if any(line.lstrip().startswith("{") for line in text.splitlines()):
        events = parse_jsonl_events(text)
        if events:
            return events
    if any("\t" in line for line in text.splitlines()):
        events = parse_event_table(text)
        if events:
            return events
    events = parse_nextest_human_log(text)
    if events:
        return events
    raise DriverError(f"no run events could be extracted from {path}")


@dataclass
class NativeEvidence:
    directory: Path
    discovery: set[str]
    discovery_files: list[Path]
    run_events: dict[str, str]
    run_files: list[Path]


def collect_native_evidence(
    adapter: Adapter,
    directory: Path,
    *,
    external_discovery: set[str] | None,
    external_events: dict[str, str] | None,
) -> NativeEvidence:
    discovery = set(external_discovery or ())
    discovery_files: list[Path] = []
    if not discovery:
        for name in adapter.discovery_files:
            path = directory / name
            if not path.is_file():
                continue
            discovery |= load_discovery_names(path)
            discovery_files.append(path)
    run_events = dict(external_events or {})
    run_files: list[Path] = []
    if not run_events:
        for name in adapter.run_log_files:
            path = directory / name
            if not path.is_file():
                continue
            run_events |= load_run_events(path)
            run_files.append(path)
    return NativeEvidence(
        directory=directory,
        discovery=discovery,
        discovery_files=discovery_files,
        run_events=run_events,
        run_files=run_files,
    )


def fresh_native_directory(root: Path, before: set[str]) -> Path | None:
    if not root.is_dir():
        return None
    after = {entry.name for entry in root.iterdir() if entry.is_dir()}
    created = sorted(after - before)
    if not created:
        return None
    return root / created[-1]


def accept_frontend(
    *,
    repo: Path,
    adapter: Adapter,
    kind: str,
    rows: list[dict[str, str]],
    run_dir: Path,
    run_id: str,
    scope: str,
    accept_deadline: str,
    source_manifest_deadline: str,
    discovery_deadline: str,
    supplied_discovery: Path | None,
    git_head: str | None,
    worktree: str,
    rust: str | None,
) -> FrontendOutcome:
    """Run one existing accept gate and fold its fresh receipts into a segment."""

    warnings: list[str] = []
    native_snapshot_root = resolve_path(repo, adapter.native_root)
    forced_root: Path | None = None
    if adapter.frontend == "gpui":
        # The GPUI gate accepts an evidence-root override; pointing it inside
        # this run keeps the fresh directory unambiguous.
        forced_root = run_dir / DEFAULT_NATIVE_DIR / adapter.frontend
        before: set[str] = set()
    else:
        before = (
            {entry.name for entry in native_snapshot_root.iterdir() if entry.is_dir()}
            if native_snapshot_root.is_dir()
            else set()
        )

    digest: str | None = None
    recorded_manifest: str | None = None
    manifest_target = repo / "target" / "frontend-source-manifests" / f"{adapter.frontend}.txt"
    manifest_command = with_timeout(
        source_manifest_deadline,
        [
            sys.executable,
            str(SOURCE_MANIFEST_PATH),
            "--frontend",
            adapter.frontend,
            "--output",
            str(manifest_target),
        ],
    )
    manifest_result = run_command(
        manifest_command,
        cwd=repo,
        timeout_s=parse_deadline(source_manifest_deadline) + TIMEOUT_BACKSTOP_SLOP_S,
        log_path=run_dir / DEFAULT_NATIVE_DIR / f"{adapter.frontend}-source-manifest.log",
    )
    if manifest_result.exit_code == 0 and manifest_target.is_file():
        digest = sha256_file(manifest_target)
        recorded_manifest = display_path(manifest_target, repo)
    else:
        warnings.append(
            f"{adapter.frontend}: source manifest generation failed "
            f"(exit {manifest_result.exit_code}); the segment declares no fingerprint "
            "and aggregates to SKIP, never PASS"
        )

    environment = dict(os.environ)
    environment["CARGO_BUILD_JOBS"] = "4"
    if adapter.frontend == "gpui":
        environment["GPUI_INTERACTION_SCOPE"] = scope
        environment["GPUI_INTERACTION_EVIDENCE_ROOT"] = str(forced_root)
    accept_log = run_dir / DEFAULT_NATIVE_DIR / f"{adapter.frontend}-accept.log"
    accept_command = with_timeout(accept_deadline, ["bash", str(repo / adapter.accept_script)])
    accept_result = run_command(
        accept_command,
        cwd=repo,
        timeout_s=parse_deadline(accept_deadline) + TIMEOUT_BACKSTOP_SLOP_S,
        env=environment,
        log_path=accept_log,
    )
    accept_exit = accept_result.exit_code
    if accept_result.timed_out:
        warnings.append(
            f"{adapter.frontend}: accept gate exceeded {accept_deadline} and was killed"
        )
    receipts = [display_path(accept_log, repo)]
    if recorded_manifest is not None:
        receipts.append(recorded_manifest)

    native_dir = fresh_native_directory(forced_root, set()) if forced_root is not None else fresh_native_directory(native_snapshot_root, before)
    discovery: set[str] = set()
    outcomes: dict[str, str] = {}
    discovery_files: list[Path] = []
    run_files: list[Path] = []
    if native_dir is not None:
        evidence = collect_native_evidence(
            adapter, native_dir, external_discovery=None, external_events=None
        )
        discovery = evidence.discovery
        outcomes = dict(evidence.run_events)
        discovery_files = evidence.discovery_files
        run_files = evidence.run_files
        receipts.extend(display_path(path, repo) for path in (*discovery_files, *run_files))
    else:
        warnings.append(
            f"{adapter.frontend}: no fresh native evidence directory under "
            f"{display_path(native_snapshot_root, repo)}; the gate produced no artifact"
        )

    if supplied_discovery is not None:
        discovery = load_discovery_names(resolve_path(repo, supplied_discovery))
        discovery_files = [resolve_path(repo, supplied_discovery)]
        receipts.append(display_path(discovery_files[0], repo))
    elif not discovery and not adapter.discovery_from_accept and accept_exit == 0:
        # Documented cost delta: the TUI gate emits no discovery artifact, so the
        # driver owns the one bounded list inside this run directory.
        list_target = run_dir / DEFAULT_DISCOVERY_DIR / f"{adapter.frontend}-list.json"
        list_command = with_timeout(
            discovery_deadline,
            [
                "cargo",
                "nextest",
                "list",
                "--locked",
                "-p",
                resolver.FRONTEND_PACKAGES[adapter.frontend][0],
                "--message-format",
                "json",
            ],
        )
        list_result = run_capture_stdout(
            list_command,
            cwd=repo,
            timeout_s=parse_deadline(discovery_deadline) + TIMEOUT_BACKSTOP_SLOP_S,
            env=environment,
            stdout_path=list_target,
            stderr_path=run_dir / DEFAULT_NATIVE_DIR / f"{adapter.frontend}-discovery.log",
        )
        if list_result.exit_code == 0 and list_target.is_file():
            discovery = load_discovery_names(list_target)
            discovery_files = [list_target]
            receipts.append(display_path(list_target, repo))
        else:
            warnings.append(
                f"{adapter.frontend}: driver-owned discovery list failed "
                f"(exit {list_result.exit_code})"
            )

    if discovery:
        merged = run_dir / DEFAULT_DISCOVERY_DIR / f"{adapter.frontend}.txt"
        merged.parent.mkdir(parents=True, exist_ok=True)
        merged.write_text("\n".join(sorted(discovery)) + "\n", encoding="utf-8")

    cells, notes = build_cells(kind, adapter.frontend, rows, discovery, outcomes)
    warnings.extend(notes)

    if discovery:
        discovery_result = "ok"
    elif accept_exit is None:
        discovery_result = "skipped"
    else:
        discovery_result = "fail"
    if accept_exit is None:
        run_result = "error"
    elif accept_exit != 0:
        run_result = "fail"
    elif any(value == "fail" for value in outcomes.values()):
        run_result = "fail"
    else:
        run_result = "ok"

    discovery_record = {
        "command": adapter.discovery_command,
        "result": discovery_result,
        "counts": {"discovered_tests": len(discovery)},
    }
    run_record: dict = {
        "command": adapter.run_command,
        "result": run_result,
        "counts": {"cells": len(cells), **event_counts(outcomes)},
    }
    if accept_exit is not None:
        run_record["counts"]["accept_exit_code"] = accept_exit
    segment = build_segment(
        frontend=adapter.frontend,
        run_id=run_id,
        cells=cells,
        discovery_record=discovery_record,
        run_record=run_record,
        digest=digest,
        source_manifest_path=recorded_manifest,
        git_head=git_head,
        worktree=worktree,
        rust=rust,
        receipts=receipts,
    )
    segment_path = run_dir / DEFAULT_SEGMENTS_DIR / f"{adapter.frontend}.json"
    write_json(segment_path, segment)
    return FrontendOutcome(adapter.frontend, segment_path, segment["overall_status"], warnings)


def replay_frontend(
    *,
    repo: Path,
    adapter: Adapter,
    kind: str,
    rows: list[dict[str, str]],
    run_dir: Path,
    run_id: str,
    fingerprint_mode: str,
    source_manifest_inputs: dict[str, Path],
    discovery_inputs: dict[str, Path],
    native_inputs: dict[str, Path],
    event_inputs: dict[str, Path],
    git_head: str | None,
    worktree: str,
    rust: str | None,
) -> FrontendOutcome:
    """Build one segment from pre-existing artifacts; no cargo is invoked."""

    warnings: list[str] = []
    frontend = adapter.frontend
    external_discovery: set[str] | None = None
    discovery_files: list[Path] = []
    if frontend in discovery_inputs:
        resolved_discovery = resolve_path(repo, discovery_inputs[frontend])
        external_discovery = load_discovery_names(resolved_discovery)
        discovery_files.append(resolved_discovery)
    external_events: dict[str, str] | None = None
    run_files: list[Path] = []
    if frontend in event_inputs:
        resolved_events = resolve_path(repo, event_inputs[frontend])
        external_events = load_run_events(resolved_events)
        run_files.append(resolved_events)

    discovery: set[str] = set(external_discovery or ())
    outcomes: dict[str, str] = dict(external_events or {})
    if frontend in native_inputs:
        directory = resolve_path(repo, native_inputs[frontend])
        if not directory.is_dir():
            raise DriverError(f"--native-evidence {frontend} is not a directory: {directory}")
        evidence = collect_native_evidence(
            adapter,
            directory,
            external_discovery=external_discovery,
            external_events=external_events,
        )
        discovery = evidence.discovery
        outcomes = dict(evidence.run_events)
        discovery_files = [*discovery_files, *evidence.discovery_files]
        run_files = [*run_files, *evidence.run_files]

    if not discovery:
        raise DriverError(
            f"{frontend}: no discovery artifact was supplied; pass "
            f"--discovery {frontend}=PATH or --native-evidence {frontend}=DIR "
            "in --from-existing mode (no cargo is run in this mode)"
        )
    if not outcomes:
        warnings.append(
            f"{frontend}: no run events were supplied; declared cells stay skipped "
            "(the aggregate reports SKIP, never PASS)"
        )

    digest: str | None = None
    recorded_manifest: str | None = None
    manifest_input = source_manifest_inputs.get(frontend)
    if fingerprint_mode == "null":
        if manifest_input is not None:
            warnings.append(
                f"{frontend}: --source-manifest was supplied but --fingerprint null "
                "keeps the segment fingerprint empty (rehearsal)"
            )
    else:
        if manifest_input is None:
            raise DriverError(
                f"{frontend}: --fingerprint current in --from-existing mode requires "
                f"--source-manifest {frontend}=PATH (no cargo is run in this mode)"
            )
        resolved_manifest = resolve_path(repo, manifest_input)
        digest = sha256_file(resolved_manifest)
        recorded_manifest = resolved_manifest.as_posix()
        warnings.append(
            f"{frontend}: --fingerprint current replays pre-existing artifacts against "
            f"the current source manifest {recorded_manifest}; this is a rehearsal, "
            "not acceptance evidence"
        )

    cells, notes = build_cells(kind, frontend, rows, discovery, outcomes)
    warnings.extend(notes)
    merged = run_dir / DEFAULT_DISCOVERY_DIR / f"{frontend}.txt"
    merged.parent.mkdir(parents=True, exist_ok=True)
    merged.write_text("\n".join(sorted(discovery)) + "\n", encoding="utf-8")

    discovery_record = {
        "command": f"[{REHEARSAL_MARKER}] " + adapter.discovery_command,
        "result": "ok",
        "counts": {"discovered_tests": len(discovery)},
    }
    if not run_files:
        run_result = "skipped"
    elif any(value == "fail" for value in outcomes.values()):
        run_result = "fail"
    else:
        run_result = "ok"
    run_command = f"[{REHEARSAL_MARKER}] " + adapter.run_command
    if run_files:
        run_command += " (outcomes replayed from " + ", ".join(
            display_path(path, repo) for path in run_files
        ) + ")"
    run_record = {
        "command": run_command,
        "result": run_result,
        "counts": {"cells": len(cells), **event_counts(outcomes)},
    }
    receipts = [display_path(path, repo) for path in (*discovery_files, *run_files)]
    segment = build_segment(
        frontend=frontend,
        run_id=run_id,
        cells=cells,
        discovery_record=discovery_record,
        run_record=run_record,
        digest=digest,
        source_manifest_path=recorded_manifest,
        git_head=git_head,
        worktree=worktree,
        rust=rust,
        receipts=receipts,
    )
    segment_path = run_dir / DEFAULT_SEGMENTS_DIR / f"{frontend}.json"
    write_json(segment_path, segment)
    return FrontendOutcome(frontend, segment_path, segment["overall_status"], warnings)


def drive(args: argparse.Namespace) -> int:
    repo = Path(args.repo_root).resolve()
    selected = list(args.frontends) if args.frontends else list(FRONTENDS)
    if len(set(selected)) != len(selected):
        raise DriverError("frontends must not repeat: " + ", ".join(selected))
    frontends = sorted(selected, key=FRONTENDS.index)
    from_existing = bool(args.from_existing)
    mode = "from-existing" if from_existing else "accept"

    declaration = resolve_path(repo, args.declaration)
    kind = declaration_kind(declaration)
    rows = read_declaration_rows(declaration, kind)

    expected = tuple(args.expect_frontend) if args.expect_frontend else tuple(frontends)
    fingerprint_mode = args.fingerprint or ("null" if from_existing else "current")
    if fingerprint_mode == "null" and not from_existing:
        raise DriverError("--fingerprint null is only meaningful with --from-existing")

    discovery_inputs = parse_pairs(args.discovery, "discovery")
    native_inputs = parse_pairs(args.native_evidence, "native-evidence")
    event_inputs = parse_pairs(args.run_events, "run-events")
    source_manifest_inputs = parse_pairs(args.source_manifest, "source-manifest")
    for label, pairs in (
        ("--discovery", discovery_inputs),
        ("--native-evidence", native_inputs),
        ("--run-events", event_inputs),
        ("--source-manifest", source_manifest_inputs),
    ):
        unknown = sorted(set(pairs) - set(frontends))
        if unknown:
            raise DriverError(f"{label} names frontend(s) outside this run: {', '.join(unknown)}")
    if not from_existing and source_manifest_inputs:
        raise DriverError(
            "--source-manifest is a --from-existing input; accept mode generates the "
            "frontend source manifest itself"
        )
    if not from_existing:
        preflight()

    run_id = args.run_id or default_run_id(repo, mode)
    run_dir = resolve_path(repo, args.evidence_root) / run_id
    if run_dir.exists() and any(run_dir.iterdir()):
        raise DriverError(f"evidence directory already exists and is not empty: {run_dir}")
    (run_dir / DEFAULT_SEGMENTS_DIR).mkdir(parents=True, exist_ok=True)

    git_head, worktree = git_state(repo)
    rust = rustc_version(repo)

    outcomes: list[FrontendOutcome] = []
    warnings: list[str] = []
    for frontend in frontends:
        adapter = ADAPTERS[frontend]
        if from_existing:
            outcome = replay_frontend(
                repo=repo,
                adapter=adapter,
                kind=kind,
                rows=rows,
                run_dir=run_dir,
                run_id=run_id,
                fingerprint_mode=fingerprint_mode,
                source_manifest_inputs=source_manifest_inputs,
                discovery_inputs=discovery_inputs,
                native_inputs=native_inputs,
                event_inputs=event_inputs,
                git_head=git_head,
                worktree=worktree,
                rust=rust,
            )
        else:
            outcome = accept_frontend(
                repo=repo,
                adapter=adapter,
                kind=kind,
                rows=rows,
                run_dir=run_dir,
                run_id=run_id,
                scope=args.scope,
                accept_deadline=args.accept_deadline,
                source_manifest_deadline=args.source_manifest_deadline,
                discovery_deadline=args.discovery_deadline,
                supplied_discovery=discovery_inputs.get(frontend),
                git_head=git_head,
                worktree=worktree,
                rust=rust,
            )
        outcomes.append(outcome)
        warnings.extend(outcome.warnings)
        print(
            f"segment {outcome.frontend}: {outcome.status} -> "
            f"{display_path(outcome.segment_path, repo)}"
        )

    resolver_report = run_dir / "manifest-validation.json"
    resolver_exit: int | None = None
    if args.resolver == "off":
        warnings.append("resolver stage skipped (--resolver off)")
    elif args.resolver == "auto" and set(frontends) != set(FRONTENDS):
        warnings.append(
            "resolver stage skipped: the run scope covers "
            f"{len(frontends)}/{len(FRONTENDS)} frontends and the resolver resolves the "
            "whole declaration; pass --resolver on if discovery is available for every "
            "declared frontend"
        )
    else:
        resolver_command = with_timeout(
            args.resolver_deadline,
            [
                sys.executable,
                str(RESOLVER_PATH),
                "--repo",
                str(repo),
                "--manifest",
                args.resolver_facet_manifest,
                "--feature-evidence",
                args.resolver_feature_evidence,
                "--interaction-matrix",
                str(declaration),
                "--requirements",
                args.resolver_requirements,
                "--report-json",
                str(resolver_report),
                *[
                    item
                    for frontend in frontends
                    for item in (
                        "--discovery",
                        f"{frontend}={run_dir / DEFAULT_DISCOVERY_DIR / f'{frontend}.txt'}",
                    )
                ],
            ],
        )
        resolver_result = run_command(
            resolver_command,
            cwd=repo,
            timeout_s=parse_deadline(args.resolver_deadline) + TIMEOUT_BACKSTOP_SLOP_S,
            log_path=run_dir / "resolver.log",
        )
        resolver_exit = resolver_result.exit_code
        if resolver_result.timed_out:
            warnings.append(f"resolver stage exceeded {args.resolver_deadline} and was killed")
        elif resolver_exit == 1:
            warnings.append("resolver reported dangling or invalid anchors (exit 1)")
        elif resolver_exit not in (0, 1):
            warnings.append(f"resolver stage failed with exit {resolver_exit}")

    aggregate_manifest = run_dir / "manifest.json"
    aggregate_exit: int | None = None
    if args.no_aggregate:
        warnings.append("aggregation skipped (--no-aggregate)")
    else:
        aggregate_arguments = [
            "--repo-root",
            str(repo),
            "--manifest",
            str(declaration),
            "--inputs",
            str(run_dir / DEFAULT_SEGMENTS_DIR),
            "--out",
            str(aggregate_manifest),
            "--run-id",
            run_id,
            *[item for frontend in expected for item in ("--expect-frontend", frontend)],
        ]
        for frontend in frontends:
            if fingerprint_mode != "current":
                continue
            manifest_path = source_manifest_inputs.get(frontend)
            if manifest_path is None and not from_existing:
                generated = repo / "target" / "frontend-source-manifests" / f"{frontend}.txt"
                manifest_path = generated if generated.is_file() else None
            if manifest_path is not None:
                aggregate_arguments.extend(
                    ["--source-manifest", f"{frontend}={resolve_path(repo, manifest_path).as_posix()}"]
                )
        if not args.report_only:
            aggregate_arguments.append("--verify")
        aggregate_command = with_timeout(
            args.aggregator_deadline,
            [sys.executable, str(AGGREGATOR_PATH), *aggregate_arguments],
        )
        aggregate_result = run_command(
            aggregate_command,
            cwd=repo,
            timeout_s=parse_deadline(args.aggregator_deadline) + TIMEOUT_BACKSTOP_SLOP_S,
            log_path=run_dir / "aggregate.log",
        )
        aggregate_exit = aggregate_result.exit_code
        if aggregate_result.timed_out:
            warnings.append(f"aggregator exceeded {args.aggregator_deadline} and was killed")
        if aggregate_manifest.is_file():
            aggregate = json.loads(aggregate_manifest.read_text(encoding="utf-8"))
            print(
                f"aggregate: {aggregate['overall_status'].upper()} "
                f"(verified {', '.join(aggregate['coverage']['verified_frontends']) or '-'}; "
                f"skipped {', '.join(aggregate['coverage']['skipped_frontends']) or '-'}; "
                f"absent {', '.join(aggregate['coverage']['absent_frontends']) or '-'})"
            )
        else:
            warnings.append(f"aggregator wrote no manifest (exit {aggregate_exit})")

    exit_code = 0
    if resolver_exit not in (0, 1, None) or aggregate_exit not in (0, 1, None):
        exit_code = 2
    elif resolver_exit == 1:
        exit_code = 1
    elif aggregate_exit == 1:
        exit_code = 1
    elif args.no_aggregate and any(outcome.status != "pass" for outcome in outcomes):
        exit_code = 1

    driver_receipt = {
        "schema_version": SCHEMA_VERSION,
        "record_kind": DRIVER_RECORD_KIND,
        "run_id": run_id,
        "mode": mode,
        "generated_at": utc_now(),
        "git_head": git_head,
        "worktree": worktree,
        "rust": rust,
        "declaration": {"path": display_path(declaration, repo), "kind": kind},
        "fingerprint_mode": fingerprint_mode,
        "frontends": {
            outcome.frontend: {
                "status": outcome.status,
                "segment": display_path(outcome.segment_path, repo),
            }
            for outcome in outcomes
        },
        "resolver": {
            "mode": args.resolver,
            "report": display_path(resolver_report, repo),
            "exit_code": resolver_exit,
        },
        "aggregate": {
            "manifest": display_path(aggregate_manifest, repo),
            "exit_code": aggregate_exit,
        },
        "warnings": warnings,
        "exit_code": exit_code,
    }
    write_json(run_dir / "accept-driver.json", driver_receipt)

    for warning in warnings:
        print(f"warning: {warning}", file=sys.stderr)
    print(f"driver receipt: {display_path(run_dir / 'accept-driver.json', repo)}")
    return exit_code


# ---------------------------------------------------------------------------
# Self-test: a synthetic fixture that exercises the real resolver and the real
# aggregator CLI end to end.  No cargo, no accept gate, temp dir only.
# ---------------------------------------------------------------------------


def self_test() -> int:
    import tempfile

    aggregator = load_module("cross_frontend_manifest", AGGREGATOR_PATH)
    checks = 0

    def check(condition: bool, label: str) -> None:
        nonlocal checks
        checks += 1
        if not condition:
            raise AssertionError(label)

    jsonl = "\n".join(
        [
            '{"type":"suite","event":"started","test_count":1}',
            '{"type":"test","event":"started","name":"taskmanager-iced::taskmanager_iced$ui::tests::mc01_case_page"}',
            '{"type":"test","event":"ignored","name":"taskmanager-iced::taskmanager_iced$ui::tests::mc01_case_page"}',
            '{"type":"test","event":"ok","name":"taskmanager-iced::taskmanager_iced$ui::tests::mc01_case_page"}',
        ]
    )
    events = parse_jsonl_events(jsonl)
    check(events == {"ui::tests::mc01_case_page": "ok"}, "jsonl events: last event wins")
    check(
        parse_event_table("ok\tui::tests::one\nfailed\tui::tests::two")["ui::tests::two"] == "fail",
        "event table extraction",
    )
    human = "\n".join(
        [
            "        PASS [   0.006s] (   1/2879) taskmanager-application alert_center::tests::x",
            "        PASS [   0.006s] taskmanager-tui ui::tests::y",
            "   TRY 2 FAIL [   0.004s] taskmanager-bevy-ui app::tests::z",
            "        FAIL [   0.010s] taskmanager-bevy-ui app::tests::z",
            "        SLOW [> 60.000s] taskmanager-tui ui::tests::slow_one",
            "error: build failed",
        ]
    )
    parsed_human = parse_nextest_human_log(human)
    check(parsed_human["alert_center::tests::x"] == "ok", "human log: counter and binary id")
    check(parsed_human["ui::tests::y"] == "ok", "human log: single-binary payload")
    check(parsed_human["app::tests::z"] == "fail", "human log: failed status")
    check(parsed_human["ui::tests::slow_one"] == "ok", "human log: SLOW is a pass")

    with tempfile.TemporaryDirectory(prefix="accept-frontend-driver-") as directory:
        root = Path(directory)

        def write(relative: str, text: str) -> Path:
            path = root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text, encoding="utf-8")
            return path

        interaction_header = "\t".join(resolver.INTERACTION_FIELDS)
        declaration = write(
            "declaration.tsv",
            "\n".join(
                [
                    "# synthetic self-test declaration (temp dir only)",
                    interaction_header,
                    "\t".join(["interaction", "mc00-gpui-demo", "gpui", "P0-MC-00", "gui", "-", "success", "-", "success", ""]),
                    "\t".join(["interaction", "mc01-iced-demo", "iced", "P0-MC-01", "lib", "ui::tests::mc01_case_page", "success", "-", "success", ""]),
                    "\t".join(["interaction", "mc02-tui-gap", "tui", "P0-MC-02", "lib", "pending", "success", "-", "success", ""]),
                    "\t".join(["interaction", "mc03-bevy-demo", "bevy", "P0-MC-03", "lib", "app::tests::mc03_case_page", "success", "-", "success", ""]),
                ]
            )
            + "\n",
        )
        gpui_case = "gpui_app::mc00_gpui_demo_case_page_behavior"
        write(
            "native/gpui/gui-list.json",
            json.dumps(
                {
                    "rust-suites": {
                        "taskmanager-gpui": {
                            "kind": "lib",
                            "testcases": {gpui_case: {"ignored": False}},
                        }
                    }
                }
            )
            + "\n",
        )
        write(
            "native/gpui/gui-run.log",
            '{"type":"test","event":"ok","name":"taskmanager-gpui::taskmanager_gpui$'
            + gpui_case
            + '"}\n',
        )
        write(
            "native/iced/lib-list.json",
            json.dumps(
                {
                    "rust-suites": {
                        "taskmanager-iced": {
                            "kind": "lib",
                            "testcases": {"ui::tests::mc01_case_page": {"ignored": False}},
                        }
                    }
                }
            )
            + "\n",
        )
        write(
            "native/iced/lib-run.log",
            '{"type":"test","event":"ok","name":"taskmanager-iced::taskmanager_iced$ui::tests::mc01_case_page"}\n',
        )
        write("native/bevy/discovery.txt", "app::tests::mc03_case_page\n")
        write("native/bevy/nextest.log", "        PASS [   0.004s] app::tests::mc03_case_page\n")
        write("native/tui/nextest.log", "        PASS [   0.002s] ui::tests::tui_ok\n")
        tui_discovery = write("discovery/tui.txt", "ui::tests::tui_ok\n")
        facet_manifest = write(
            "facet.tsv",
            "\t".join(resolver.MANIFEST_FIELDS)
            + "\n"
            + "\t".join(["facet", "demo-facet", "bevy", "unsupported", "no source", "honesty", "none", "-", "bevy", ""])
            + "\n",
        )
        feature_manifest = write(
            "feature.tsv",
            "\t".join(resolver.FEATURE_EVIDENCE_FIELDS)
            + "\n"
            + "\t".join(["demo-feature", "bevy", "-", "pending", "no discoverable anchor yet"])
            + "\n",
        )
        requirements = write("requirements.tsv", "requirement_id\nP0-MC-00\nP0-MC-01\nP0-MC-02\nP0-MC-03\n")
        source_manifests = {
            frontend: write(f"source/{frontend}.txt", f"{frontend}-source\n")
            for frontend in FRONTENDS
        }
        evidence_root = root / "evidence"
        common = [
            "--repo-root",
            str(root),
            "--declaration",
            str(declaration),
            "--resolver-facet-manifest",
            str(facet_manifest),
            "--resolver-feature-evidence",
            str(feature_manifest),
            "--resolver-requirements",
            str(requirements),
            "--evidence-root",
            str(evidence_root),
        ]
        native_arguments = [
            item
            for frontend in ("gpui", "iced", "tui", "bevy")
            for item in ("--native-evidence", f"{frontend}={root / 'native' / frontend}")
        ]
        fingerprint_arguments = [
            item
            for frontend in FRONTENDS
            for item in ("--source-manifest", f"{frontend}={source_manifests[frontend]}")
        ]
        three_front_fingerprints = [
            item
            for frontend in ("gpui", "iced", "tui")
            for item in ("--source-manifest", f"{frontend}={source_manifests[frontend]}")
        ]
        two_front_fingerprints = [
            item
            for frontend in ("gpui", "iced")
            for item in ("--source-manifest", f"{frontend}={source_manifests[frontend]}")
        ]

        # Arm 1: four fronts, real run events replayed, current fingerprints -> PASS.
        code = main(
            [
                *FRONTENDS,
                "--from-existing",
                "--run-id",
                "selftest-pass",
                "--discovery",
                f"tui={tui_discovery}",
                *native_arguments,
                "--fingerprint",
                "current",
                *fingerprint_arguments,
                *common,
            ]
        )
        check(code == 0, "end-to-end pass arm exits 0")
        manifest = json.loads((evidence_root / "selftest-pass" / "manifest.json").read_text())
        check(manifest["overall_status"] == "pass", "end-to-end aggregate is pass")
        check(manifest["coverage"]["verified_frontends"] == list(FRONTENDS), "four frontends verified")
        check(manifest["coverage"]["covered_cells"] == 3, "three anchored cells covered")
        check(manifest["coverage"]["expected_cells"] == 3, "pending row is not expected")
        for frontend in FRONTENDS:
            segment = json.loads(
                (evidence_root / "selftest-pass" / "segments" / f"{frontend}.json").read_text()
            )
            aggregator.validate_segment(segment, f"selftest/{frontend}")
            check(segment["run_id"] == "selftest-pass", f"{frontend} segment carries the run id")
            check(
                segment["overall_status"] == "pass" and segment["source_manifest_sha256"] is not None,
                f"{frontend} segment is pass with a fingerprint",
            )
        gpui_segment = json.loads(
            (evidence_root / "selftest-pass" / "segments" / "gpui.json").read_text()
        )
        check(
            gpui_segment["cells"][0]["test_id"] == gpui_case,
            "case-prefix channel resolves the run cell test id",
        )

        # Arm 2: discovery only, no run events -> SKIP (never PASS); --report-only
        # still exits 0 but the aggregate stays SKIP.
        three_front_discovery = [
            "--discovery",
            f"gpui={root / 'native' / 'gpui' / 'gui-list.json'}",
            "--discovery",
            f"iced={root / 'native' / 'iced' / 'lib-list.json'}",
            "--discovery",
            f"tui={tui_discovery}",
        ]
        discovery_arguments = [
            *three_front_discovery,
            "--discovery",
            f"bevy={root / 'native' / 'bevy' / 'discovery.txt'}",
        ]
        code = main(
            [
                *FRONTENDS,
                "--from-existing",
                "--run-id",
                "selftest-skip",
                *discovery_arguments,
                *fingerprint_arguments,
                "--fingerprint",
                "current",
                *common,
            ]
        )
        check(code == 1, "an eventless replay verifies as non-pass")
        skipped = json.loads((evidence_root / "selftest-skip" / "manifest.json").read_text())
        check(skipped["overall_status"] == "skip", "eventless replay is SKIP, never PASS")
        code = main(
            [
                *FRONTENDS,
                "--from-existing",
                "--run-id",
                "selftest-skip-report",
                *discovery_arguments,
                *fingerprint_arguments,
                "--fingerprint",
                "current",
                "--report-only",
                *common,
            ]
        )
        check(code == 0, "--report-only keeps an objective skip at exit 0")
        reported = json.loads((evidence_root / "selftest-skip-report" / "manifest.json").read_text())
        check(reported["overall_status"] == "skip", "--report-only never turns SKIP into PASS")

        # Arm 3: an expected frontend with no segment is a FAIL.
        code = main(
            [
                "gpui",
                "iced",
                "tui",
                "--from-existing",
                "--run-id",
                "selftest-absent",
                "--expect-frontend",
                "bevy",
                *three_front_discovery,
                *three_front_fingerprints,
                "--fingerprint",
                "current",
                *common,
            ]
        )
        check(code == 1, "expected frontend without a segment fails the run")
        absent = json.loads((evidence_root / "selftest-absent" / "manifest.json").read_text())
        check(
            any(item["code"] == "expected_frontend_absent" for item in absent["findings"]),
            "absent expectation is reported as expected_frontend_absent",
        )

        # Arm 4: a deleted anchor is a collection-time (R4) failure.  This arm
        # uses its own two-front declaration set so the resolver's required
        # frontends match the run scope (the shared fixtures name all four).
        renamed_discovery = write("discovery/iced-renamed.txt", "ui::tests::renamed_away\n")
        dangling_declaration = write(
            "dangling-declaration.tsv",
            "\n".join(
                [
                    interaction_header,
                    "\t".join(["interaction", "mc00-gpui-demo", "gpui", "P0-MC-00", "gui", "-", "success", "-", "success", ""]),
                    "\t".join(["interaction", "mc01-iced-demo", "iced", "P0-MC-01", "lib", "ui::tests::mc01_case_page", "success", "-", "success", ""]),
                ]
            )
            + "\n",
        )
        dangling_facet = write(
            "dangling-facet.tsv",
            "\t".join(resolver.MANIFEST_FIELDS)
            + "\n"
            + "\t".join(["facet", "demo-facet", "iced", "unsupported", "no source", "honesty", "none", "-", "iced", ""])
            + "\n",
        )
        dangling_feature = write(
            "dangling-feature.tsv",
            "\t".join(resolver.FEATURE_EVIDENCE_FIELDS)
            + "\n"
            + "\t".join(["demo-feature", "gpui", "-", "pending", "no discoverable anchor yet"])
            + "\n",
        )
        dangling_requirements = write("dangling-requirements.tsv", "requirement_id\nP0-MC-00\nP0-MC-01\n")
        code = main(
            [
                "gpui",
                "iced",
                "--from-existing",
                "--run-id",
                "selftest-dangling",
                "--resolver",
                "on",
                "--discovery",
                f"gpui={root / 'native' / 'gpui' / 'gui-list.json'}",
                "--discovery",
                f"iced={renamed_discovery}",
                *two_front_fingerprints,
                "--fingerprint",
                "current",
                "--repo-root",
                str(root),
                "--declaration",
                str(dangling_declaration),
                "--resolver-facet-manifest",
                str(dangling_facet),
                "--resolver-feature-evidence",
                str(dangling_feature),
                "--resolver-requirements",
                str(dangling_requirements),
                "--evidence-root",
                str(evidence_root),
            ]
        )
        check(code == 1, "a deleted anchor fails the resolver stage")
        resolver_report = json.loads(
            (evidence_root / "selftest-dangling" / "manifest-validation.json").read_text()
        )
        check(
            any(
                item.get("subject_id") == "mc01-iced-demo"
                and item.get("reason", "").startswith("anchor not discovered")
                for item in resolver_report["dangling"]
            ),
            "the resolver reports the deleted anchor as dangling",
        )

        # Arm 5: accept-mode plumbing with a stub gate.  The stub writes the same
        # native artifacts the real GPUI gate writes, so the driver's real-mode
        # path (source-manifest step -> gate invocation -> fresh native directory
        # -> segment) is exercised without running cargo tests.  The stub repo
        # has no Cargo workspace, so the fingerprint step fails on purpose and
        # the segment declares no fingerprint: the aggregate must stay SKIP.
        if shutil.which("cargo") is None:
            print("self-test: accept-mode stub arm skipped (cargo unavailable)", file=sys.stderr)
        else:
            stub_repo = root / "accept-repo"
            stub_script = stub_repo / "scripts" / "accept-gpui-interactions.sh"
            stub_script.parent.mkdir(parents=True, exist_ok=True)
            stub_script.write_text(
                "\n".join(
                    [
                        "#!/usr/bin/env bash",
                        "set -euo pipefail",
                        'run="${GPUI_INTERACTION_EVIDENCE_ROOT:?}/stub-run"',
                        'mkdir -p "$run"',
                        "printf '%s\\n' "
                        "'{\"rust-suites\":{\"taskmanager-gpui\":{\"kind\":\"lib\","
                        "\"testcases\":{\""
                        + gpui_case
                        + "\":{\"ignored\":false}}}}}' > \"$run/gui-list.json\"",
                        "printf '%s\\n' "
                        "'{\"type\":\"test\",\"event\":\"ok\",\"name\":"
                        "\"taskmanager-gpui::taskmanager_gpui$"
                        + gpui_case
                        + "\"}' > \"$run/gui-run.log\"",
                        "printf 'GPUI interaction acceptance: PASS (self-test stub)\\n'",
                    ]
                )
                + "\n",
                encoding="utf-8",
            )
            code = main(
                [
                    "gpui",
                    "--repo-root",
                    str(stub_repo),
                    "--run-id",
                    "selftest-accept",
                    "--declaration",
                    str(declaration),
                    "--evidence-root",
                    str(root / "evidence-accept"),
                    "--expect-frontend",
                    "gpui",
                    "--resolver",
                    "off",
                ]
            )
            check(code == 1, "accept mode without a fingerprint verifies as non-pass")
            accept_segment = json.loads(
                (root / "evidence-accept" / "selftest-accept" / "segments" / "gpui.json").read_text()
            )
            aggregator.validate_segment(accept_segment, "selftest/accept-gpui")
            check(accept_segment["overall_status"] == "pass", "the stub gate's cell covers its case")
            check(accept_segment["source_manifest_sha256"] is None, "the failed fingerprint step is not faked")
            check(accept_segment["cells"][0]["outcome"] == "ok", "the stub gate's ok event covers the cell")
            check(
                accept_segment["discovery"]["counts"]["discovered_tests"] == 1,
                "the stub gate's fresh native discovery was read",
            )
            check(
                accept_segment["run"]["counts"].get("accept_exit_code") == 0,
                "the stub gate's exit code is recorded",
            )
            accept_receipt = json.loads(
                (root / "evidence-accept" / "selftest-accept" / "accept-driver.json").read_text()
            )
            check(accept_receipt["mode"] == "accept", "the receipt records accept mode")
            check(
                any("fingerprint" in warning for warning in accept_receipt["warnings"]),
                "the failed fingerprint step is reported as a warning",
            )

        # The driver receipt records the replay mode, and a replay that does not
        # pass --run-id carries the rehearsal marker in its generated run id.
        receipt = json.loads((evidence_root / "selftest-pass" / "accept-driver.json").read_text())
        check(receipt["mode"] == "from-existing", "driver receipt records the replay mode")
        check(receipt["fingerprint_mode"] == "current", "driver receipt records the fingerprint policy")
        code = main(
            [
                "gpui",
                "--from-existing",
                "--evidence-root",
                str(root / "evidence-default"),
                "--discovery",
                f"gpui={root / 'native' / 'gpui' / 'gui-list.json'}",
                *common[:-2],
            ]
        )
        check(code == 1, "the default replay fingerprint is null, so the run verifies as non-pass")
        default_directories = sorted((root / "evidence-default").iterdir())
        check(len(default_directories) == 1, "the default run id created exactly one run directory")
        check(
            REHEARSAL_MARKER in default_directories[0].name,
            "the default replay run id carries the rehearsal marker",
        )
        default_receipt = json.loads((default_directories[0] / "accept-driver.json").read_text())
        check(
            default_receipt["fingerprint_mode"] == "null",
            "the default replay fingerprint policy is null (SKIP, never PASS)",
        )

    print(f"accept-frontend-interactions self-test: PASS ({checks} checks)")
    return 0


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="accept-frontend-interactions",
        description=(
            "Unified cross-frontend interaction acceptance driver (S5 step 1): run the "
            "per-frontend accept gates, write one run segment per frontend, and fold "
            "them with the cross-frontend manifest aggregator."
        ),
    )
    parser.add_argument(
        "frontends",
        nargs="*",
        choices=FRONTENDS,
        metavar="FRONTEND",
        help="frontend(s) to accept; default: all four",
    )
    parser.add_argument("--repo-root", type=Path, default=DEFAULT_REPO_ROOT)
    parser.add_argument(
        "--declaration",
        default=DEFAULT_DECLARATION,
        help=(
            "committed declaration list the segments are keyed against "
            "(default: %(default)s, the unified interaction matrix; a facet manifest is "
            "also accepted)"
        ),
    )
    parser.add_argument("--run-id", default=None, help="run id (default: UTC stamp + git state)")
    parser.add_argument(
        "--evidence-root",
        default=DEFAULT_EVIDENCE_ROOT,
        help="run evidence root; segments land in <root>/<run_id>/segments/ (default: %(default)s)",
    )
    parser.add_argument(
        "--from-existing",
        "--dry-run",
        dest="from_existing",
        action="store_true",
        help=(
            "rehearse the segment -> aggregate -> verify chain from pre-existing "
            "artifacts; runs no cargo and invokes no accept gate"
        ),
    )
    parser.add_argument(
        "--discovery",
        action="append",
        default=[],
        metavar="FRONTEND=PATH",
        help="pre-generated nextest discovery artifact (JSON list or plain --list text)",
    )
    parser.add_argument(
        "--native-evidence",
        action="append",
        default=[],
        metavar="FRONTEND=DIR",
        help="native accept evidence directory to read discovery and run logs from",
    )
    parser.add_argument(
        "--run-events",
        action="append",
        default=[],
        metavar="FRONTEND=PATH",
        help="run-event artifact: libtest-json-plus lines, event<TAB>name table, or a nextest log",
    )
    parser.add_argument(
        "--source-manifest",
        action="append",
        default=[],
        metavar="FRONTEND=PATH",
        help="current frontend source manifest used as the fingerprint (--from-existing only)",
    )
    parser.add_argument(
        "--fingerprint",
        choices=("current", "null"),
        default=None,
        help=(
            "segment fingerprint policy; default: current in accept mode, null in "
            "--from-existing mode (null aggregates to SKIP, never PASS)"
        ),
    )
    parser.add_argument(
        "--scope",
        default="linux",
        help="GPUI accept scope dispatched through GPUI_INTERACTION_SCOPE (default: %(default)s)",
    )
    parser.add_argument("--accept-deadline", default=DEFAULT_ACCEPT_DEADLINE)
    parser.add_argument("--source-manifest-deadline", default=DEFAULT_SOURCE_MANIFEST_DEADLINE)
    parser.add_argument("--discovery-deadline", default=DEFAULT_DISCOVERY_DEADLINE)
    parser.add_argument("--resolver-deadline", default=DEFAULT_RESOLVER_DEADLINE)
    parser.add_argument("--aggregator-deadline", default=DEFAULT_AGGREGATOR_DEADLINE)
    parser.add_argument(
        "--expect-frontend",
        action="append",
        default=[],
        choices=FRONTENDS,
        help="require a segment for this frontend (repeatable); defaults to the selected frontends",
    )
    parser.add_argument("--resolver-facet-manifest", default=DEFAULT_FACET_MANIFEST)
    parser.add_argument("--resolver-feature-evidence", default=DEFAULT_FEATURE_EVIDENCE)
    parser.add_argument("--resolver-requirements", default=DEFAULT_REQUIREMENTS)
    parser.add_argument(
        "--resolver",
        choices=("auto", "on", "off"),
        default="auto",
        help=(
            "Layer B resolver stage: auto runs it only for a full four-frontend scope "
            "(the resolver resolves the whole declaration), on always, off never"
        ),
    )
    parser.add_argument("--no-aggregate", action="store_true", help="write segments only")
    parser.add_argument(
        "--report-only",
        action="store_true",
        help="run the aggregator without --verify (a SKIP aggregate then exits 0 but stays SKIP)",
    )
    parser.add_argument("--self-test", action="store_true")
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    if args.self_test:
        try:
            return self_test()
        except AssertionError as error:
            print(f"accept-frontend-interactions self-test: FAIL: {error}", file=sys.stderr)
            return 1
    try:
        return drive(args)
    except DriverError as error:
        print(f"accept-frontend-interactions: ERROR {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
