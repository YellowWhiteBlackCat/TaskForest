#!/usr/bin/env python3
"""Publish one validated capture run through an atomic, locked pointer.

Publication is the only path that may claim a run is the current one. The
published pointer carries a structured receipt next to the symlink so a
reader can answer "is this the latest state?" without re-running a capture
and without opening the run's private bookkeeping.
"""

from __future__ import annotations

import argparse
import fcntl
import hashlib
import os
import json
import sys
import tempfile
import uuid
from datetime import datetime, timezone
from pathlib import Path
from importlib.util import module_from_spec, spec_from_file_location


def presentation_resolution_contract() -> dict[str, object]:
    """The shared presentation resolution contract (ADR-064).

    The Rust owner is welded by ``tests/logic/presentation_resolution_test``;
    this loader keeps the published receipt on the same single source as the
    gate instead of spelling a second role census in the publisher.
    """
    path = Path(__file__).parent / "quality" / "presentation_resolution_guard.py"
    spec = spec_from_file_location("presentation_resolution_guard", path)
    if spec is None or spec.loader is None:
        raise PublishError("cannot load the presentation resolution guard")
    module = module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.emit_contract()


def resolution_digest(declared: dict[str, object]) -> str:
    """Hash the same declared values the capture gate verifies."""
    path = Path(__file__).parent / "quality" / "presentation_resolution_guard.py"
    spec = spec_from_file_location("presentation_resolution_guard", path)
    if spec is None or spec.loader is None:
        raise PublishError("cannot load the presentation resolution guard")
    module = module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.digest(declared)


def validate_presentation_resolution(receipt: dict[str, object]) -> None:
    """Assert one published receipt satisfies the ADR-064 capture gate."""
    path = Path(__file__).parent / "quality" / "presentation_resolution_guard.py"
    spec = spec_from_file_location("presentation_resolution_guard", path)
    if spec is None or spec.loader is None:
        raise PublishError("cannot load the presentation resolution guard")
    module = module_from_spec(spec)
    spec.loader.exec_module(module)
    expected = module.emit_contract()
    resolution = receipt.get("presentation_resolution")
    if not isinstance(resolution, dict):
        raise PublishError("published receipt has no presentation resolution")
    contract = expected["contract"]
    for field in ("control_roles", "motion_roles", "hit_target_floor_px", "bottom_safety_band_px"):
        if resolution.get(field) != contract[field]:
            raise PublishError(f"published presentation resolution disagrees on {field}")
    declared = {key: resolution[key] for key in (
        "ui_size", "density", "motion_policy", "control_roles", "motion_roles",
        "hit_target_floor_px", "bottom_safety_band_px",
    )}
    if resolution.get("resolution_digest") != module.digest(declared):
        raise PublishError("published presentation resolution digest is not self-consistent")


EVIDENCE_ROOTS = {
    "gpui": "screenshot-evidence",
    "iced": "iced-evidence",
    "tui": "tui-evidence",
    "bevy": "bevy-evidence",
    "host-wayland": "host-wayland-diagnostic",
}
VALIDATION_FILES = {
    "gpui": "capture-validation.json",
    "bevy": "bevy-capture-validation.json",
    "iced": "iced-capture-validation.json",
    "tui": "tui-capture-validation.json",
}


class PublishError(RuntimeError):
    """A fail-closed publication error."""


def atomic_write(path: Path, content: str) -> None:
    descriptor, temporary_name = tempfile.mkstemp(
        prefix=f".{path.name}.", dir=path.parent
    )
    temporary = Path(temporary_name)
    try:
        os.fchmod(descriptor, 0o600)
        with os.fdopen(descriptor, "w", encoding="utf-8") as handle:
            handle.write(content)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)


def read_run_metadata(run_root: Path) -> dict[str, str]:
    """Read a run's key/value metadata, whichever name the frontend used."""
    candidates = sorted(run_root.glob("*metadata*.txt"))
    if not candidates:
        return {}
    values: dict[str, str] = {}
    for line in candidates[0].read_text(encoding="utf-8").splitlines():
        if "=" in line:
            key, _, value = line.partition("=")
            values.setdefault(key.strip(), value.strip())
    return values


def run_provenance(frontend: str, run_root: Path, run_uuid: uuid.UUID) -> dict[str, object]:
    """Build the published receipt of one run.

    The source manifest digest is the run's link to the pixels: a reader
    recomputes the same digest over the live tree and compares.
    """
    metadata = read_run_metadata(run_root)
    validation_name = VALIDATION_FILES[frontend]
    validation = run_root / validation_name
    resolution = presentation_resolution_contract()["contract"]
    receipt = {
        "frontend": frontend,
        "run_uuid": str(run_uuid),
        "run_root": run_root.as_posix(),
        "captured_at": metadata.get("captured_at", ""),
        "git_head": metadata.get("git_head", ""),
        "worktree": metadata.get("worktree", ""),
        "source_scope": metadata.get("source_scope", frontend),
        "source_manifest_sha256": metadata.get("source_manifest_sha256", ""),
        "scenario_count": metadata.get("scenario_count", ""),
        "validation_receipt": validation.name,
        "validation_sha256": sha256_file(validation),
        "presentation_resolution": {
            "ui_size": metadata.get("ui_size", "Standard"),
            "density": metadata.get("density", "Comfortable"),
            "motion_policy": metadata.get("motion_policy", "normal"),
            "control_roles": resolution["control_roles"],
            "motion_roles": resolution["motion_roles"],
            "hit_target_floor_px": resolution["hit_target_floor_px"],
            "bottom_safety_band_px": resolution["bottom_safety_band_px"],
            "resolution_digest": resolution_digest({
                "ui_size": metadata.get("ui_size", "Standard"),
                "density": metadata.get("density", "Comfortable"),
                "motion_policy": metadata.get("motion_policy", "normal"),
                **resolution,
            }),
        },
        "published_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    }
    return receipt


def sha256_file(path: Path) -> str:
    """Return the SHA-256 digest of a file."""
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def publish(repo_root: Path, frontend: str, run_root: Path, run_uuid: uuid.UUID) -> Path:
    try:
        evidence_root = repo_root / "target" / EVIDENCE_ROOTS[frontend]
    except KeyError as error:
        raise PublishError(f"unsupported publication frontend: {frontend}") from error
    expected_run_root = evidence_root / "runs" / str(run_uuid)
    resolved_run_root = run_root.resolve()
    if resolved_run_root != expected_run_root.resolve() or not resolved_run_root.is_dir():
        raise PublishError(f"run root is not the UUID-owned evidence directory: {run_root}")
    validation_name = VALIDATION_FILES.get(frontend)
    if validation_name is None:
        raise PublishError(f"frontend does not support latest publication: {frontend}")
    validation = resolved_run_root / validation_name
    if not validation.is_file():
        raise PublishError(f"run validation is not a passing receipt: {validation}")
    try:
        validation_payload = json.loads(validation.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise PublishError(f"run validation is not valid JSON: {validation}") from error
    if validation_payload.get("status") != "pass":
        raise PublishError(f"run validation is not a passing receipt: {validation}")

    evidence_root.mkdir(parents=True, exist_ok=True)
    lock_path = evidence_root / ".latest.lock"
    latest = evidence_root / "latest"
    temporary_link = evidence_root / f".latest.{run_uuid}.next"
    with lock_path.open("a+", encoding="ascii") as lock:
        fcntl.flock(lock.fileno(), fcntl.LOCK_EX)
        try:
            if latest.exists() or latest.is_symlink():
                if not latest.is_symlink():
                    legacy = evidence_root / f"latest-legacy-{run_uuid}"
                    os.replace(latest, legacy)
            relative_target = os.path.relpath(resolved_run_root, evidence_root)
            os.symlink(relative_target, temporary_link)
            os.replace(temporary_link, latest)
            atomic_write(evidence_root / "latest.txt", f"{run_uuid}\n")
            receipt = run_provenance(frontend, resolved_run_root, run_uuid)
            atomic_write(
                evidence_root / "latest.json",
                json.dumps(receipt, indent=2, sort_keys=True) + "\n",
            )
            directory_fd = os.open(evidence_root, os.O_RDONLY | getattr(os, "O_DIRECTORY", 0))
            try:
                os.fsync(directory_fd)
            finally:
                os.close(directory_fd)
        finally:
            temporary_link.unlink(missing_ok=True)
            fcntl.flock(lock.fileno(), fcntl.LOCK_UN)
    return latest


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--repo-root", type=Path)
    parser.add_argument("--frontend", choices=sorted(EVIDENCE_ROOTS))
    parser.add_argument("--run-root", type=Path)
    parser.add_argument("--run-uuid")
    args = parser.parse_args(argv)
    if args.self_test:
        from concurrent.futures import ThreadPoolExecutor
        import tempfile

        with tempfile.TemporaryDirectory(prefix="taskforest-publish-test-") as temporary:
            root = Path(temporary)
            run_uuid = uuid.uuid4()
            run_root = root / "target" / "screenshot-evidence" / "runs" / str(run_uuid)
            run_root.mkdir(parents=True)
            (run_root / "capture-validation.json").write_text(
                json.dumps({"status": "pass"}) + "\n", encoding="utf-8"
            )
            latest = publish(root, "gpui", run_root, run_uuid)
            if not latest.is_symlink() or latest.resolve() != run_root.resolve():
                raise PublishError("self-test first publication did not create the UUID pointer")
            next_uuid = uuid.uuid4()
            next_root = root / "target" / "screenshot-evidence" / "runs" / str(next_uuid)
            next_root.mkdir(parents=True)
            (next_root / "capture-validation.json").write_text(
                json.dumps({"status": "pass"}) + "\n", encoding="utf-8"
            )
            publish(root, "gpui", next_root, next_uuid)
            if (root / "target" / "screenshot-evidence" / "latest").resolve() != next_root.resolve():
                raise PublishError("self-test second publication did not replace the pointer")
            receipt = json.loads(
                (root / "target" / "screenshot-evidence" / "latest.json").read_text(
                    encoding="utf-8"
                )
            )
            if receipt.get("run_uuid") != str(next_uuid):
                raise PublishError("self-test receipt does not name the published run")
            if receipt.get("frontend") != "gpui":
                raise PublishError("self-test receipt does not name the publishing frontend")
            if not receipt.get("validation_sha256"):
                raise PublishError("self-test receipt does not bind the validation receipt")
            resolution = receipt.get("presentation_resolution")
            if not isinstance(resolution, dict) or not resolution.get("resolution_digest"):
                raise PublishError("self-test receipt does not bind the presentation resolution")
            if resolution.get("hit_target_floor_px") != 44.0:
                raise PublishError("self-test receipt does not carry the shared hit-target floor")
            validate_presentation_resolution(receipt)
            bevy_uuid = uuid.uuid4()
            bevy_root = root / "target" / "bevy-evidence" / "runs" / str(bevy_uuid)
            bevy_root.mkdir(parents=True)
            (bevy_root / "bevy-capture-validation.json").write_text(
                json.dumps({"status": "pass"}) + "\n", encoding="utf-8"
            )
            bevy_latest = publish(root, "bevy", bevy_root, bevy_uuid)
            if bevy_latest.resolve() != bevy_root.resolve():
                raise PublishError("self-test Bevy publication did not create the pointer")
            concurrent_runs: list[tuple[uuid.UUID, Path]] = []
            for _ in range(8):
                concurrent_uuid = uuid.uuid4()
                concurrent_root = (
                    root
                    / "target"
                    / "screenshot-evidence"
                    / "runs"
                    / str(concurrent_uuid)
                )
                concurrent_root.mkdir(parents=True)
                (concurrent_root / "capture-validation.json").write_text(
                    json.dumps({"status": "pass"}) + "\n", encoding="utf-8"
                )
                concurrent_runs.append((concurrent_uuid, concurrent_root))
            with ThreadPoolExecutor(max_workers=8) as executor:
                list(
                    executor.map(
                        lambda item: publish(root, "gpui", item[1], item[0]),
                        concurrent_runs,
                    )
                )
            final_target = (root / "target" / "screenshot-evidence" / "latest").resolve()
            if final_target not in {run_root.resolve() for _, run_root in concurrent_runs}:
                raise PublishError("self-test concurrent publication selected an unknown run")
            if list((root / "target" / "screenshot-evidence").glob(".latest.*.next")):
                raise PublishError("self-test left a temporary latest pointer")
        print("capture publish self-test: PASS")
        return 0
    required = (args.repo_root, args.frontend, args.run_root, args.run_uuid)
    if any(value is None for value in required):
        parser.error("all publication paths are required unless --self-test is used")
    try:
        latest = publish(
            args.repo_root.resolve(),
            args.frontend,
            args.run_root,
            uuid.UUID(args.run_uuid),
        )
    except (OSError, ValueError, PublishError) as error:
        print(f"capture publish: FAIL: {error}", file=sys.stderr)
        return 2
    print(f"capture publish: {latest}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
