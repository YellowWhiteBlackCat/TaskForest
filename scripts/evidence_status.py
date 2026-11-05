#!/usr/bin/env python3
"""Answer one question about capture evidence: do these pixels still match the
live source tree?

Every capture run records the digest of a frontend-scoped source manifest.
That digest is the only honest link between a frame and the source that
produced it. This tool recomputes the manifest for the live tree and compares
the two digests, so "is this screenshot the latest state?" is a mechanical
fact instead of a recollection.

The frontends disagree about where a frame lives on disk: GPUI writes
`screenshots/<name>.png`, Bevy writes `<scenario>/image.png`, and TUI writes
`<name>.png` in the run root. This tool normalizes every native manifest into
one `evidence-index.tsv` shape so a reader never has to learn four layouts.

Usage:
    python3 scripts/evidence_status.py
    python3 scripts/evidence_status.py --frontend bevy --require-fresh
    python3 scripts/evidence_status.py --frontend gpui --json
"""

from __future__ import annotations

import argparse
import contextlib
import hashlib
import io
import json
import sys
import tempfile
from dataclasses import dataclass, field
from pathlib import Path

from capture_publish import EVIDENCE_ROOTS
from frontend_source_manifest import write_manifest

INDEX_NAME = "evidence-index.tsv"
REFERENCE_DIRNAME = "reference"

INDEX_COLUMNS = (
    "frame",
    "scenario",
    "page",
    "window",
    "png",
    "sha256",
    "bytes",
    "width",
    "height",
    "status",
    "appearance",
)

COLUMN_ALIASES: dict[str, tuple[str, ...]] = {
    "scenario": ("scenario", "name"),
    "page": ("page",),
    "window": ("requested_window", "window_size"),
    "png": ("image", "screenshot"),
    "sha256": ("sha256",),
    "bytes": ("bytes",),
    "width": ("width",),
    "height": ("height",),
    "status": ("status", "markers"),
    "appearance": ("appearance", "skin"),
}


class EvidenceError(RuntimeError):
    """A fail-closed evidence lookup error."""


@dataclass(frozen=True)
class Frame:
    """One captured frame, normalized across the frontend manifests."""

    frame: str
    scenario: str
    page: str
    window: str
    png: str
    sha256: str
    bytes: str
    width: str
    height: str
    status: str
    appearance: str


@dataclass
class RunStatus:
    """The measurable state of one capture run."""

    frontend: str
    run_uuid: str
    run_root: str
    selected_by: str
    captured_at: str = ""
    recorded_digest: str = ""
    live_digest: str = ""
    freshness: str = "UNANCHORED"
    changed_files: int = 0
    changed_sample: list[str] = field(default_factory=list)
    validation: str = "absent"
    frames: list[Frame] = field(default_factory=list)

    @property
    def current(self) -> bool:
        return self.freshness == "CURRENT"


def evidence_root(
    repo: Path, frontend: str, root_name: str | None = None
) -> Path:
    """Return the per-frontend evidence root.

    One frontend can own two data regimes — the live host and the compiled
    fixture — and the fixture regime keeps its own root. `root_name` names
    that root explicitly; without it the frontend's default root answers.
    """
    if root_name:
        return repo / "target" / root_name
    try:
        return repo / "target" / EVIDENCE_ROOTS[frontend]
    except KeyError as error:
        raise EvidenceError(f"unsupported evidence frontend: {frontend}") from error


def _first_file(run_root: Path, patterns: tuple[str, ...]) -> Path | None:
    for pattern in patterns:
        matches = sorted(run_root.glob(pattern))
        if matches:
            return matches[0]
    return None


def source_manifest_path(run_root: Path) -> Path | None:
    """Return the stored frontend-scoped source manifest of a run."""
    matches = sorted(run_root.glob("*source-manifest*.sha256"))
    return matches[0] if matches else None


def read_metadata(run_root: Path) -> dict[str, str]:
    """Read the run's key/value metadata, whichever name the frontend used."""
    path = _first_file(run_root, ("*metadata*.txt",))
    if path is None:
        return {}
    values: dict[str, str] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        if "=" in line:
            key, _, value = line.partition("=")
            values.setdefault(key.strip(), value.strip())
    return values


def validation_state(run_root: Path) -> str:
    """Report whether the run carries a passing validation receipt."""
    path = _first_file(
        run_root,
        (
            "capture-validation.json",
            "bevy-capture-validation.json",
            "iced-capture-validation.json",
            "tui-capture-validation.json",
        ),
    )
    if path is None:
        return "absent"
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return "unreadable"
    status = payload.get("status")
    return str(status) if status else "unknown"


def frame_key(relative_png: str) -> str:
    """Return the stable frame key of a manifest image path.

    Bevy names every frame `image.png` inside a scenario directory, so the
    directory carries the identity. Every other frontend names the frame
    directly.
    """
    path = Path(relative_png)
    if path.stem == "image" and path.parent.name:
        return path.parent.name
    return path.stem


def _pick(row: dict[str, str], field_name: str) -> str:
    for alias in COLUMN_ALIASES[field_name]:
        value = row.get(alias)
        if value:
            return value
    return ""


def read_frames(run_root: Path) -> list[Frame]:
    """Read the run's native manifest into normalized frames."""
    candidates = [
        path
        for path in sorted(run_root.glob("*.tsv"))
        if "manifest" in path.name and "validator" not in path.name
    ]
    frames: list[Frame] = []
    for path in candidates:
        lines = path.read_text(encoding="utf-8").splitlines()
        if len(lines) < 2:
            continue
        header = lines[0].split("\t")
        for line in lines[1:]:
            cells = line.split("\t")
            if len(cells) != len(header):
                continue
            row = dict(zip(header, cells, strict=True))
            png = _pick(row, "png")
            if not png:
                continue
            png = resolve_png(run_root, png)
            frames.append(
                Frame(
                    frame=frame_key(png),
                    scenario=_pick(row, "scenario"),
                    page=_pick(row, "page"),
                    window=_pick(row, "window"),
                    png=png,
                    sha256=_pick(row, "sha256"),
                    bytes=_pick(row, "bytes"),
                    width=_pick(row, "width"),
                    height=_pick(row, "height"),
                    status=_pick(row, "status"),
                    appearance=_pick(row, "appearance"),
                )
            )
        if frames:
            break
    return frames


def resolve_png(run_root: Path, recorded: str) -> str:
    """Return the manifest's image path relative to the run root.

    GPUI records a bare file name and keeps the frames in `screenshots/`;
    Bevy records the scenario-relative path. Both must resolve against the
    run root so one index shape serves every frontend.
    """
    if (run_root / recorded).is_file():
        return recorded
    nested = run_root / "screenshots" / recorded
    if nested.is_file():
        return nested.relative_to(run_root).as_posix()
    return recorded


def write_index(run_root: Path, frames: list[Frame]) -> Path:
    """Write the normalized evidence index next to the run's own receipts."""
    index = run_root / INDEX_NAME
    lines = ["\t".join(INDEX_COLUMNS)]
    for frame in frames:
        lines.append(
            "\t".join(
                (
                    frame.frame,
                    frame.scenario,
                    frame.page,
                    frame.window,
                    frame.png,
                    frame.sha256,
                    frame.bytes,
                    frame.width,
                    frame.height,
                    frame.status,
                    frame.appearance,
                )
            )
        )
    index.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return index


def sha256_file(path: Path) -> str:
    """Return the SHA-256 digest of a file."""
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def live_digest(repo: Path, frontend: str) -> str:
    """Return the digest of the live tree's frontend-scoped source manifest."""
    with tempfile.TemporaryDirectory(prefix="taskforest-evidence-status-") as temporary:
        output = Path(temporary) / "source-manifest.sha256"
        with contextlib.redirect_stdout(io.StringIO()):
            write_manifest(repo, frontend, output)
        return sha256_file(output)


def manifest_entries(path: Path) -> dict[str, str]:
    """Return the `path -> digest` map of a stored source manifest."""
    entries: dict[str, str] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        digest, _, name = line.partition("  ")
        if name:
            entries[name] = digest
    return entries


def changed_paths(run_root: Path, repo: Path, frontend: str) -> list[str]:
    """Return the source paths that differ between the run and the live tree."""
    stored = source_manifest_path(run_root)
    if stored is None:
        return []
    with tempfile.TemporaryDirectory(prefix="taskforest-evidence-status-") as temporary:
        output = Path(temporary) / "source-manifest.sha256"
        with contextlib.redirect_stdout(io.StringIO()):
            write_manifest(repo, frontend, output)
        old_entries = manifest_entries(stored)
        new_entries = manifest_entries(output)
    return [
        name
        for name in sorted(set(old_entries) | set(new_entries))
        if old_entries.get(name) != new_entries.get(name)
    ]


def select_run(
    repo: Path, frontend: str, run_uuid: str | None, root_name: str | None = None
) -> tuple[Path, str]:
    """Resolve the run to report on and how it was selected."""
    root = evidence_root(repo, frontend, root_name)
    runs = root / "runs"
    if run_uuid:
        candidate = runs / run_uuid
        if not candidate.is_dir():
            raise EvidenceError(f"no such {frontend} run: {run_uuid}")
        return candidate, "explicit"
    pointer = root / "latest"
    if pointer.is_symlink() and pointer.resolve().is_dir():
        return pointer.resolve(), "latest"
    if not runs.is_dir():
        raise EvidenceError(f"{frontend} has no evidence root")
    candidates = [path for path in runs.iterdir() if path.is_dir()]
    if not candidates:
        raise EvidenceError(f"{frontend} has no capture runs")
    newest = max(candidates, key=lambda path: path.stat().st_mtime)
    return newest, "newest"


def status_of(
    repo: Path,
    frontend: str,
    run_uuid: str | None = None,
    root_name: str | None = None,
) -> RunStatus:
    """Measure one run's freshness and frame inventory."""
    run_root, selected_by = select_run(repo, frontend, run_uuid, root_name)
    metadata = read_metadata(run_root)
    result = RunStatus(
        frontend=frontend,
        run_uuid=run_root.name,
        run_root=str(run_root.relative_to(repo)),
        selected_by=selected_by,
        captured_at=metadata.get("captured_at", ""),
        recorded_digest=metadata.get("source_manifest_sha256", ""),
        validation=validation_state(run_root),
    )
    result.frames = read_frames(run_root)
    write_index(run_root, result.frames)
    result.live_digest = live_digest(repo, frontend)
    if not result.recorded_digest:
        result.freshness = "UNANCHORED"
        return result
    if result.recorded_digest == result.live_digest:
        result.freshness = "CURRENT"
        return result
    result.freshness = "STALE"
    changed = changed_paths(run_root, repo, frontend)
    result.changed_files = len(changed)
    result.changed_sample = changed[:12]
    return result


def reference_status(
    repo: Path, frontend: str, root_name: str | None = None
) -> dict[str, str] | None:
    """Return the promoted reference receipt of a frontend, if any."""
    path = (
        evidence_root(repo, frontend, root_name)
        / REFERENCE_DIRNAME
        / "reference.json"
    )
    if not path.is_file():
        return None
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return {"state": "unreadable"}
    receipt = {str(key): str(value) for key, value in payload.items()}
    frames = payload.get("frames")
    receipt["frame_count"] = str(len(frames)) if isinstance(frames, dict) else "0"
    recorded = receipt.get("source_manifest_sha256", "")
    if not recorded:
        receipt["freshness"] = "UNANCHORED"
        return receipt
    try:
        live = live_digest(repo, frontend)
    except (OSError, ValueError):
        receipt["freshness"] = "UNKNOWN"
        return receipt
    receipt["live_source"] = live
    receipt["freshness"] = "CURRENT" if recorded == live else "STALE"
    return receipt


def format_report(result: RunStatus, reference: dict[str, str] | None) -> str:
    """Render one run's status as a compact reader-facing block."""
    lines = [
        f"frontend={result.frontend}",
        f"  run={result.run_uuid}  selected_by={result.selected_by}",
        f"  path={result.run_root}",
        f"  captured_at={result.captured_at or 'unknown'}",
        f"  validation={result.validation}",
        f"  recorded_source={result.recorded_digest or 'none'}",
        f"  live_source={result.live_digest or 'none'}",
        f"  freshness={result.freshness}",
    ]
    if result.freshness == "STALE":
        lines.append(f"  changed_files={result.changed_files}")
        for name in result.changed_sample:
            lines.append(f"    changed: {name}")
        if result.changed_files > len(result.changed_sample):
            lines.append(f"    ... {result.changed_files - len(result.changed_sample)} more")
    lines.append(f"  frames={len(result.frames)}  index={result.run_root}/{INDEX_NAME}")
    if reference is None:
        lines.append("  reference=absent")
    else:
        lines.append(
            "  reference="
            f"{reference.get('run_uuid', 'unknown')} "
            f"source={reference.get('source_manifest_sha256', 'unknown')} "
            f"freshness={reference.get('freshness', 'unknown')} "
            f"frames={reference.get('frame_count', '0')}"
        )
    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--repo-root", type=Path, default=Path(__file__).resolve().parents[1]
    )
    parser.add_argument("--frontend", choices=sorted(EVIDENCE_ROOTS), default="all")
    parser.add_argument("--run", help="report one run uuid instead of the selected one")
    parser.add_argument(
        "--evidence-root",
        help="read a non-default regime root (for example screenshot-evidence-demo)",
    )
    parser.add_argument("--require-fresh", action="store_true")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args(argv)

    repo = args.repo_root.resolve()
    frontends = sorted(EVIDENCE_ROOTS) if args.frontend == "all" else [args.frontend]
    reports: list[RunStatus] = []
    failures: list[str] = []
    for frontend in frontends:
        try:
            result = status_of(repo, frontend, args.run, args.evidence_root)
        except EvidenceError as error:
            failures.append(f"{frontend}: {error}")
            continue
        reports.append(result)
        if args.require_fresh and not result.current:
            failures.append(f"{frontend}: evidence is {result.freshness}")

    if args.json:
        print(
            json.dumps(
                {
                    "reports": [
                        {
                            "frontend": report.frontend,
                            "run_uuid": report.run_uuid,
                            "selected_by": report.selected_by,
                            "captured_at": report.captured_at,
                            "freshness": report.freshness,
                            "recorded_source": report.recorded_digest,
                            "live_source": report.live_digest,
                            "changed_files": report.changed_files,
                            "validation": report.validation,
                            "frames": len(report.frames),
                        }
                        for report in reports
                    ],
                    "failures": failures,
                },
                indent=2,
            )
        )
    else:
        for report in reports:
            print(
                format_report(
                    report,
                    reference_status(repo, report.frontend, args.evidence_root),
                )
            )
    for failure in failures:
        print(f"evidence status: FAIL: {failure}", file=sys.stderr)
    return 2 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
