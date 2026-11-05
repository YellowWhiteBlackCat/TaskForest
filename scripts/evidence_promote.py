#!/usr/bin/env python3
"""Promote captured frames into a frontend's terminal reference set.

`latest` answers "which run did we capture last?". A reference answers
"which pixels are the terminal target for this product?". Those are different
questions: a targeted debugging capture moves `latest` but must never change
the target, so the reference set is written explicitly and refuses to be
silently overwritten.

The promoted receipt records the run, the capture time and the source
manifest digest, so the reference can be measured against the live tree the
same way a candidate run is.

Usage:
    python3 scripts/evidence_promote.py --frontend gpui --run <uuid>
    python3 scripts/evidence_promote.py --frontend gpui --run <uuid> \
        --frame perf-memory-normal --frame perf-cpu-normal
    python3 scripts/evidence_promote.py --frontend gpui --run <uuid> --force
"""

from __future__ import annotations

import argparse
import json
import shutil
import sys
from datetime import datetime, timezone
from pathlib import Path

from evidence_status import (
    INDEX_NAME,
    REFERENCE_DIRNAME,
    EvidenceError,
    evidence_root,
    read_frames,
    read_metadata,
    sha256_file,
    source_manifest_path,
    write_index,
)


class PromoteError(RuntimeError):
    """A fail-closed promotion error."""


def run_root_for(
    repo: Path, frontend: str, run_uuid: str, root_name: str | None = None
) -> Path:
    """Resolve the run that owns the frames to promote."""
    candidate = evidence_root(repo, frontend, root_name) / "runs" / run_uuid
    if not candidate.is_dir():
        raise PromoteError(f"no such {frontend} run: {run_uuid}")
    return candidate


def select_frames(run_root: Path, wanted: list[str] | None) -> list[tuple[str, Path]]:
    """Return the `(frame, png)` pairs to promote."""
    frames = read_frames(run_root)
    if not frames:
        raise PromoteError(f"run has no readable capture manifest: {run_root.name}")
    chosen = frames
    if wanted:
        known = {frame.frame for frame in frames}
        missing = [name for name in wanted if name not in known]
        if missing:
            raise PromoteError(
                f"run does not contain frame(s): {', '.join(sorted(missing))}"
            )
        wanted_set = set(wanted)
        chosen = [frame for frame in frames if frame.frame in wanted_set]
    pairs: list[tuple[str, Path]] = []
    for frame in chosen:
        png = run_root / frame.png
        if not png.is_file():
            raise PromoteError(f"frame file is missing: {frame.png}")
        pairs.append((frame.frame, png))
    return pairs


def promote(
    repo: Path,
    frontend: str,
    run_uuid: str,
    wanted: list[str] | None,
    force: bool,
    root_name: str | None = None,
) -> Path:
    """Copy the selected frames into the frontend's reference set."""
    run_root = run_root_for(repo, frontend, run_uuid, root_name)
    reference = evidence_root(repo, frontend, root_name) / REFERENCE_DIRNAME
    receipt_path = reference / "reference.json"
    if receipt_path.exists() and not force:
        raise PromoteError(
            f"{frontend} already has a promoted reference; pass --force to replace it"
        )
    pairs = select_frames(run_root, wanted)
    metadata = read_metadata(run_root)
    stored_manifest = source_manifest_path(run_root)
    write_index(run_root, read_frames(run_root))

    reference.mkdir(parents=True, exist_ok=True)
    entries: dict[str, dict[str, str]] = {}
    for frame_name, png in pairs:
        target = reference / f"{frame_name}.png"
        shutil.copy2(png, target)
        entries[frame_name] = {
            "png": target.name,
            "sha256": sha256_file(target),
            "bytes": str(target.stat().st_size),
        }
    receipt = {
        "frontend": frontend,
        "run_uuid": run_uuid,
        "captured_at": metadata.get("captured_at", ""),
        "git_head": metadata.get("git_head", ""),
        "source_scope": metadata.get("source_scope", frontend),
        "source_manifest_sha256": metadata.get("source_manifest_sha256", ""),
        "source_manifest_file": stored_manifest.name if stored_manifest else "",
        "promoted_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "index": f"{run_root.relative_to(repo).as_posix()}/{INDEX_NAME}",
        "frames": entries,
    }
    receipt_path.write_text(
        json.dumps(receipt, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    return receipt_path


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--repo-root", type=Path, default=Path(__file__).resolve().parents[1]
    )
    parser.add_argument("--frontend", required=True)
    parser.add_argument("--run", required=True, help="the run uuid to promote from")
    parser.add_argument(
        "--frame", action="append", help="promote only this frame (repeatable)"
    )
    parser.add_argument("--force", action="store_true")
    parser.add_argument(
        "--evidence-root", help="explicit evidence root folder name under target/"
    )
    args = parser.parse_args(argv)

    repo = args.repo_root.resolve()
    try:
        receipt = promote(
            repo, args.frontend, args.run, args.frame, args.force, args.evidence_root
        )
    except (EvidenceError, PromoteError, OSError) as error:
        print(f"evidence promote: FAIL: {error}", file=sys.stderr)
        return 2
    print(f"evidence promote: {receipt}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
