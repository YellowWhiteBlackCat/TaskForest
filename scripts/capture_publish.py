#!/usr/bin/env python3
"""Publish one validated capture run through an atomic, locked pointer."""

from __future__ import annotations

import argparse
import fcntl
import os
import json
import sys
import tempfile
import uuid
from pathlib import Path


EVIDENCE_ROOTS = {
    "gpui": "screenshot-evidence",
    "iced": "iced-evidence",
    "tui": "tui-evidence",
    "bevy": "bevy-evidence",
    "host-wayland": "host-wayland-diagnostic",
}
VALIDATION_FILES = {
    "gpui": "capture-validation.json",
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
