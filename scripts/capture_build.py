#!/usr/bin/env python3
"""Serialize capture builds and copy one immutable binary into a run root."""

from __future__ import annotations

import argparse
import fcntl
import hashlib
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


class BuildError(RuntimeError):
    """A fail-closed capture build error."""


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def copy_immutable(source: Path, destination: Path) -> None:
    if not source.is_file():
        raise BuildError(f"capture build did not produce {source}")
    destination.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary_name = tempfile.mkstemp(
        prefix=f".{destination.name}.", dir=destination.parent
    )
    temporary = Path(temporary_name)
    try:
        with os.fdopen(descriptor, "wb") as output, source.open("rb") as input_file:
            shutil.copyfileobj(input_file, output, length=1024 * 1024)
            output.flush()
            os.fsync(output.fileno())
        os.chmod(temporary, 0o755)
        os.replace(temporary, destination)
    finally:
        temporary.unlink(missing_ok=True)


def build(repo_root: Path, source: Path, destination: Path, command: list[str]) -> str:
    if not command:
        raise BuildError("capture build command is empty")
    target_root = (repo_root / "target").resolve()
    source = source.resolve()
    destination = destination.resolve()
    if not source.is_relative_to(target_root):
        raise BuildError(f"capture build source is outside target: {source}")
    run_root = destination.parent.parent
    if not destination.is_relative_to(run_root) or not run_root.name:
        raise BuildError(f"invalid run-owned binary destination: {destination}")
    lock_root = target_root / "capture-runs"
    lock_root.mkdir(parents=True, exist_ok=True)
    lock_path = lock_root / ".build.lock"
    with lock_path.open("a+", encoding="ascii") as lock:
        fcntl.flock(lock.fileno(), fcntl.LOCK_EX)
        result = subprocess.run(command, cwd=repo_root, check=False, timeout=1200)
        if result.returncode != 0:
            raise BuildError(f"cargo capture build failed with status {result.returncode}")
        copy_immutable(source, destination)
        fcntl.flock(lock.fileno(), fcntl.LOCK_UN)
    return sha256(destination)


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--repo-root", type=Path)
    parser.add_argument("--source", type=Path)
    parser.add_argument("--destination", type=Path)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args(argv)
    if args.self_test:
        import tempfile

        with tempfile.TemporaryDirectory(prefix="taskforest-build-test-") as temporary:
            root = Path(temporary)
            source = root / "target" / "debug" / "taskmanager"
            destination = root / "target" / "capture-runs" / "gpui" / "run" / "bin" / "taskforest-g"
            source.parent.mkdir(parents=True)
            source.write_bytes(b"capture-build-self-test")
            digest = build(root, source, destination, ["true"])
            if destination.read_bytes() != source.read_bytes() or digest != sha256(destination):
                raise BuildError("self-test immutable copy mismatch")
        print("capture build self-test: PASS")
        return 0
    required = (args.repo_root, args.source, args.destination)
    if any(value is None for value in required):
        parser.error("all build paths are required unless --self-test is used")
    command = list(args.command)
    if command and command[0] == "--":
        command = command[1:]
    try:
        digest = build(
            args.repo_root.resolve(),
            args.source,
            args.destination,
            command,
        )
    except (BuildError, OSError, subprocess.SubprocessError) as error:
        print(f"capture build: FAIL: {error}", file=sys.stderr)
        return 1
    print(f"capture build: binary_sha256={digest}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
