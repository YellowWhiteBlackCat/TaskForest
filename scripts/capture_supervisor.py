#!/usr/bin/env python3
"""Own one background capture run and its complete Linux process tree.

The capture scripts deliberately run inside this supervisor instead of relying
on a shell EXIT trap.  A run receives a random UUID, a private runtime tree,
and a user-owned cgroup v2 leaf.  A detached watchdog reaps that leaf when the
supervisor itself disappears, which covers the interruption that previously
left a private KWin/Niri pair behind.

This module has no Wayland or D-Bus dependencies.  The watchdog never inherits
those session variables and therefore cannot create a desktop surface or bus
name of its own.
"""

from __future__ import annotations

import argparse
import ctypes
import json
import os
import platform
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import uuid
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable, Sequence


CAPTURE_CGROUP_PREFIX = "taskforest-capture-"
RUNTIME_PREFIXES = {
    "gpui": "taskforest-capture-gpui-",
    "iced": "taskforest-capture-iced-",
    "tui": "taskforest-capture-tui-",
    "bevy": "taskforest-capture-bevy-",
    "host-wayland": "taskforest-capture-host-wayland-",
    "isolation": "taskforest-capture-isolation-",
}
EVIDENCE_ROOTS = {
    "gpui": "screenshot-evidence",
    "iced": "iced-evidence",
    "tui": "tui-evidence",
    "bevy": "bevy-evidence",
    "host-wayland": "host-wayland-diagnostic",
    "isolation": "capture-isolation",
}
PRIVATE_SESSION_KEYS = (
    "DBUS_SESSION_BUS_ADDRESS",
    "DBUS_STARTER_BUS_TYPE",
    "DISPLAY",
    "WAYLAND_DISPLAY",
)


class SupervisorError(RuntimeError):
    """A fail-closed capture supervision error."""


def atomic_write(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
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


def atomic_json(path: Path, payload: dict[str, object]) -> None:
    atomic_write(path, json.dumps(payload, indent=2, sort_keys=True) + "\n")


def proc_start_time(pid: int) -> str | None:
    try:
        text = Path(f"/proc/{pid}/stat").read_text(encoding="utf-8")
    except (FileNotFoundError, PermissionError, OSError):
        return None
    try:
        after_command = text.rsplit(")", 1)[1].split()
        # /proc/<pid>/stat field 22, indexed from field 3 after the command.
        return after_command[19]
    except (IndexError, ValueError):
        return None


def process_matches(pid: int, expected_start_time: str) -> bool:
    actual = proc_start_time(pid)
    return actual is not None and actual == expected_start_time


def unified_cgroup_for_pid(pid: int) -> Path:
    for line in Path(f"/proc/{pid}/cgroup").read_text(encoding="utf-8").splitlines():
        hierarchy, _controllers, relative = line.split(":", 2)
        if hierarchy == "0":
            return Path("/sys/fs/cgroup") / relative.lstrip("/")
    raise SupervisorError("Linux cgroup v2 is unavailable")


def safe_runtime_remove(runtime_root: Path, frontend: str) -> None:
    expected_prefix = RUNTIME_PREFIXES[frontend]
    if runtime_root.is_symlink():
        raise SupervisorError(f"refusing symlink runtime path: {runtime_root}")
    resolved = runtime_root.resolve(strict=False)
    if resolved.parent != Path("/tmp") or not resolved.name.startswith(expected_prefix):
        raise SupervisorError(f"refusing unexpected runtime path: {resolved}")
    if resolved.exists():
        shutil.rmtree(resolved)


@dataclass
class CgroupLeaf:
    path: Path
    parent: Path

    @property
    def procs_file(self) -> Path:
        return self.path / "cgroup.procs"

    @property
    def kill_file(self) -> Path:
        return self.path / "cgroup.kill"

    @classmethod
    def create(cls, run_uuid: uuid.UUID) -> "CgroupLeaf":
        parent = unified_cgroup_for_pid(os.getpid())
        if not parent.is_dir() or parent.stat().st_uid != os.getuid():
            raise SupervisorError(f"capture cgroup parent is not user-owned: {parent}")
        path = parent / f"{CAPTURE_CGROUP_PREFIX}{run_uuid.hex}"
        if path.exists():
            raise SupervisorError(f"capture cgroup already exists: {path}")
        try:
            path.mkdir(mode=0o700)
        except OSError as error:
            raise SupervisorError(f"cannot create capture cgroup {path}: {error}") from error
        if path.stat().st_uid != os.getuid():
            raise SupervisorError(f"capture cgroup is not user-owned: {path}")
        return cls(path=path, parent=parent)

    @classmethod
    def from_metadata(cls, path_text: str, run_uuid: uuid.UUID) -> "CgroupLeaf | None":
        path = Path(path_text)
        expected_name = f"{CAPTURE_CGROUP_PREFIX}{run_uuid.hex}"
        if path.name != expected_name or not path.is_absolute():
            return None
        parent = path.parent
        if not path.exists() or path.stat().st_uid != os.getuid():
            return None
        return cls(path=path, parent=parent)

    def attach(self, pid: int) -> None:
        try:
            self.procs_file.write_text(f"{pid}\n", encoding="ascii")
        except OSError as error:
            raise SupervisorError(f"cannot attach PID {pid} to {self.path}: {error}") from error

    def pids(self) -> set[int]:
        try:
            text = self.procs_file.read_text(encoding="ascii")
        except (FileNotFoundError, PermissionError, OSError):
            return set()
        result: set[int] = set()
        for token in text.split():
            try:
                result.add(int(token))
            except ValueError:
                continue
        return result

    def send_signal(self, signum: signal.Signals, excluded: Iterable[int] = ()) -> None:
        excluded_set = set(excluded)
        for pid in self.pids() - excluded_set:
            try:
                os.kill(pid, signum)
            except ProcessLookupError:
                continue
            except PermissionError as error:
                raise SupervisorError(f"cannot signal capture PID {pid}: {error}") from error

    def kill_all(self) -> None:
        if not self.path.exists():
            return
        if self.kill_file.exists():
            try:
                self.kill_file.write_text("1\n", encoding="ascii")
                return
            except OSError:
                pass
        self.send_signal(signal.SIGKILL)

    def move_to_parent(self, pid: int) -> None:
        try:
            (self.parent / "cgroup.procs").write_text(f"{pid}\n", encoding="ascii")
        except OSError as error:
            raise SupervisorError(f"cannot move supervisor out of {self.path}: {error}") from error

    def remove(self) -> bool:
        if not self.path.exists():
            return True
        if self.pids():
            return False
        try:
            self.path.rmdir()
        except OSError:
            return False
        return True


def drain_cgroup(leaf: CgroupLeaf, excluded: Iterable[int] = ()) -> None:
    excluded_set = set(excluded)
    leaf.send_signal(signal.SIGTERM, excluded_set)
    for _ in range(60):
        if not (leaf.pids() - excluded_set):
            return
        time.sleep(0.05)
    leaf.send_signal(signal.SIGKILL, excluded_set)
    for _ in range(40):
        if not (leaf.pids() - excluded_set):
            return
        time.sleep(0.05)


def arm_parent_death_signal() -> None:
    if platform.system() != "Linux":
        raise SupervisorError("capture supervisor requires Linux")
    libc = ctypes.CDLL(None, use_errno=True)
    pr_set_pdeathsig = 1
    if libc.prctl(pr_set_pdeathsig, signal.SIGTERM, 0, 0, 0) != 0:
        error = ctypes.get_errno()
        raise SupervisorError(f"PR_SET_PDEATHSIG failed: errno={error}")


def scrub_watchdog_environment() -> dict[str, str]:
    environment = os.environ.copy()
    for key in PRIVATE_SESSION_KEYS:
        environment.pop(key, None)
    environment.pop("TM_CAPTURE_SUPERVISED", None)
    environment.pop("TM_CAPTURE_SUPERVISOR_TOKEN", None)
    return environment


def make_layout(repo_root: Path, frontend: str, run_uuid: uuid.UUID) -> tuple[Path, Path]:
    run_root = (
        repo_root
        / "target"
        / EVIDENCE_ROOTS[frontend]
        / "runs"
        / str(run_uuid)
    )
    if run_root.exists():
        raise SupervisorError(f"run UUID collision: {run_root}")
    run_root.mkdir(mode=0o700, parents=True)
    runtime_root = Path(
        tempfile.mkdtemp(prefix=RUNTIME_PREFIXES[frontend], dir="/tmp")
    )
    os.chmod(runtime_root, 0o700)
    return run_root, runtime_root


def write_cleanup_status(run_root: Path, status: str) -> None:
    atomic_write(run_root / "cleanup-status.txt", f"residue=none\nstatus={status}\n")


def watchdog_main(
    run_root: Path,
    runtime_root: Path,
    frontend: str,
    run_uuid: uuid.UUID,
    cgroup_path: Path,
    supervisor_pid: int,
    supervisor_start_time: str,
) -> int:
    ready = run_root / "supervisor-ready"
    done = run_root / "supervisor-done"
    # Give the supervisor enough time to create and attach its cgroup before
    # treating a very early process disappearance as a crash.
    startup_deadline = time.monotonic() + 10.0
    for _ in range(18_000):
        if done.exists():
            return 0
        if ready.exists() and not process_matches(supervisor_pid, supervisor_start_time):
            leaf = CgroupLeaf.from_metadata(str(cgroup_path), run_uuid)
            if leaf is not None:
                leaf.kill_all()
                for _ in range(40):
                    if not leaf.pids():
                        break
                    time.sleep(0.05)
                leaf.remove()
            safe_runtime_remove(runtime_root, frontend)
            write_cleanup_status(run_root, "watchdog-reaped-supervisor")
            atomic_write(run_root / "watchdog-reaped", "supervisor=gone\n")
            return 0
        if not ready.exists() and time.monotonic() >= startup_deadline:
            if not process_matches(supervisor_pid, supervisor_start_time):
                safe_runtime_remove(runtime_root, frontend)
            write_cleanup_status(run_root, "watchdog-reaped-before-ready")
            return 0
        time.sleep(0.1)
    return 1


def reclaim_stale(repo_root: Path) -> int:
    reclaimed = 0
    for frontend, evidence_root_name in EVIDENCE_ROOTS.items():
        frontend_root = repo_root / "target" / evidence_root_name / "runs"
        if not frontend_root.is_dir():
            continue
        for run_root in frontend_root.iterdir():
            if not run_root.is_dir() or (run_root / "supervisor-done").exists():
                continue
            metadata_path = run_root / "supervisor.json"
            if not metadata_path.is_file():
                continue
            try:
                metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
                run_uuid = uuid.UUID(str(metadata["run_uuid"]))
                supervisor_pid = int(metadata["supervisor_pid"])
                supervisor_start_time = str(metadata["supervisor_start_time"])
                cgroup_path = Path(str(metadata["cgroup_path"]))
                runtime_root = Path(str(metadata["runtime_root"]))
            except (KeyError, TypeError, ValueError, OSError, json.JSONDecodeError):
                continue
            if process_matches(supervisor_pid, supervisor_start_time):
                continue
            leaf = CgroupLeaf.from_metadata(str(cgroup_path), run_uuid)
            if leaf is not None:
                leaf.kill_all()
                for _ in range(40):
                    if not leaf.pids():
                        break
                    time.sleep(0.05)
                leaf.remove()
            try:
                safe_runtime_remove(runtime_root, frontend)
            except SupervisorError:
                continue
            write_cleanup_status(run_root, "reclaim-stale-run")
            atomic_write(run_root / "supervisor-done", "reclaimed=1\n")
            reclaimed += 1
    return reclaimed


def validate_supervised_metadata(
    metadata: dict[str, str],
    repo_root: Path,
    metadata_path: Path,
    frontend: str,
) -> uuid.UUID:
    """Validate the cross-frontend ownership fields in a capture receipt."""

    required = {
        "run_id",
        "run_uuid",
        "frontend",
        "run_root",
        "runtime_root",
        "supervisor_pid",
        "cgroup_path",
        "dbus_address_sha256",
    }
    missing = required - metadata.keys()
    if missing:
        raise SupervisorError(f"metadata missing supervisor fields: {sorted(missing)}")
    try:
        run_uuid = uuid.UUID(metadata["run_uuid"])
    except ValueError as error:
        raise SupervisorError("metadata run_uuid is not a UUID") from error
    if metadata["run_id"] != metadata["run_uuid"]:
        raise SupervisorError("run_id and run_uuid differ")
    if metadata["frontend"] != frontend:
        raise SupervisorError(
            f"metadata frontend {metadata['frontend']!r} differs from {frontend!r}"
        )
    run_root = (repo_root / metadata["run_root"]).resolve()
    if run_root != metadata_path.resolve().parent:
        raise SupervisorError("metadata run_root does not own the metadata file")
    if not metadata["supervisor_pid"].isdigit() or int(metadata["supervisor_pid"]) < 1:
        raise SupervisorError("metadata supervisor_pid is invalid")
    if not re.fullmatch(r"[0-9a-f]{64}", metadata["dbus_address_sha256"]):
        raise SupervisorError("metadata dbus_address_sha256 is invalid")
    runtime_root = Path(metadata["runtime_root"])
    expected_prefix = RUNTIME_PREFIXES.get(frontend)
    if (
        expected_prefix is None
        or not runtime_root.is_absolute()
        or not runtime_root.name.startswith(expected_prefix)
    ):
        raise SupervisorError(f"metadata runtime_root is not {frontend}-private")
    cgroup_path = Path(metadata["cgroup_path"])
    if cgroup_path.name != f"{CAPTURE_CGROUP_PREFIX}{run_uuid.hex}":
        raise SupervisorError("metadata cgroup_path is not owned by this run UUID")
    if not str(cgroup_path).startswith("/sys/fs/cgroup/"):
        raise SupervisorError("metadata cgroup_path is outside cgroup v2")
    return run_uuid


def run_capture(repo_root: Path, frontend: str, command: Sequence[str]) -> int:
    if not command:
        raise SupervisorError("capture command is empty")
    if frontend not in RUNTIME_PREFIXES:
        raise SupervisorError(f"unsupported capture frontend: {frontend}")

    reclaim_stale(repo_root)
    run_uuid = uuid.uuid4()
    run_root, runtime_root = make_layout(repo_root, frontend, run_uuid)
    supervisor_pid = os.getpid()
    supervisor_start_time = proc_start_time(supervisor_pid)
    if supervisor_start_time is None:
        raise SupervisorError("supervisor has no /proc start-time receipt")

    try:
        leaf = CgroupLeaf.create(run_uuid)
    except SupervisorError as error:
        atomic_write(run_root / "supervisor-error.txt", f"{error}\n")
        safe_runtime_remove(runtime_root, frontend)
        raise
    metadata = {
        "schema_version": 1,
        "run_uuid": str(run_uuid),
        "frontend": frontend,
        "repo_root": str(repo_root),
        "run_root": str(run_root),
        "runtime_root": str(runtime_root),
        "supervisor_pid": supervisor_pid,
        "supervisor_start_time": supervisor_start_time,
        "cgroup_path": str(leaf.path),
        "command": list(command),
        "state": "starting",
    }
    atomic_json(run_root / "supervisor.json", metadata)
    watchdog_command = [
        sys.executable,
        str(Path(__file__).resolve()),
        "--watchdog",
        "--frontend",
        frontend,
        "--run-uuid",
        str(run_uuid),
        "--run-root",
        str(run_root),
        "--runtime-root",
        str(runtime_root),
        "--cgroup-path",
        str(leaf.path),
        "--supervisor-pid",
        str(supervisor_pid),
        "--supervisor-start-time",
        supervisor_start_time,
    ]
    watchdog = subprocess.Popen(
        watchdog_command,
        cwd=repo_root,
        env=scrub_watchdog_environment(),
        start_new_session=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    child: subprocess.Popen[bytes] | None = None
    stop_signal: int | None = None
    stop_requested = False

    def request_stop(signum: int, _frame: object) -> None:
        nonlocal stop_signal, stop_requested
        stop_signal = signum
        stop_requested = True

    signal.signal(signal.SIGINT, request_stop)
    signal.signal(signal.SIGTERM, request_stop)
    try:
        arm_parent_death_signal()
        if os.getppid() == 1:
            raise SupervisorError("capture supervisor lost its parent before startup")
        leaf.attach(supervisor_pid)
        (run_root / "supervisor-ready").touch(mode=0o600)
        metadata["state"] = "running"
        metadata["watchdog_pid"] = watchdog.pid
        atomic_json(run_root / "supervisor.json", metadata)

        child_environment = os.environ.copy()
        child_environment.update(
            {
                "TM_CAPTURE_SUPERVISED": "1",
                "TM_CAPTURE_SUPERVISOR_TOKEN": str(run_uuid),
                "TM_CAPTURE_FRONTEND": frontend,
                "TM_CAPTURE_RUN_UUID": str(run_uuid),
                "TM_CAPTURE_RUN_ROOT": str(run_root),
                "TM_CAPTURE_RUNTIME_ROOT": str(runtime_root),
                "TM_CAPTURE_CGROUP_PATH": str(leaf.path),
                "TM_CAPTURE_SUPERVISOR_PID": str(supervisor_pid),
                "TM_CAPTURE_WATCHDOG_PID": str(watchdog.pid),
            }
        )
        child = subprocess.Popen(
            list(command),
            cwd=repo_root,
            env=child_environment,
            start_new_session=True,
        )
        metadata["child_pid"] = child.pid
        metadata["child_start_time"] = proc_start_time(child.pid)
        atomic_json(run_root / "supervisor.json", metadata)

        for _ in range(18_000):
            if child.poll() is not None:
                break
            if stop_requested:
                try:
                    os.killpg(child.pid, stop_signal or signal.SIGTERM)
                except (ProcessLookupError, PermissionError):
                    pass
                break
            time.sleep(0.1)
        if child.poll() is None:
            child.wait(timeout=3.0)
        exit_code = child.returncode if child.returncode is not None else 143
        if stop_signal is not None:
            exit_code = 128 + stop_signal
    except (OSError, subprocess.SubprocessError, SupervisorError) as error:
        atomic_write(run_root / "supervisor-error.txt", f"{error}\n")
        exit_code = 2
    finally:
        try:
            if child is not None and child.poll() is None:
                try:
                    os.killpg(child.pid, signal.SIGTERM)
                except (ProcessLookupError, PermissionError):
                    pass
            drain_cgroup(leaf, excluded={supervisor_pid})
            if supervisor_pid in leaf.pids():
                leaf.move_to_parent(supervisor_pid)
            leaf.kill_all()
            for _ in range(40):
                if not leaf.pids():
                    break
                time.sleep(0.05)
            removed = leaf.remove()
            safe_runtime_remove(runtime_root, frontend)
            write_cleanup_status(
                run_root,
                "supervisor-cleanup" if removed else "supervisor-cleanup-cgroup-retained",
            )
        except (OSError, SupervisorError) as error:
            atomic_write(run_root / "cleanup-failure.txt", f"{error}\n")
            exit_code = max(exit_code, 2)
        metadata["state"] = "complete"
        metadata["exit_code"] = exit_code
        atomic_json(run_root / "supervisor.json", metadata)
        (run_root / "supervisor-done").touch(mode=0o600)
        try:
            watchdog.wait(timeout=3.0)
        except subprocess.TimeoutExpired:
            watchdog.terminate()
            watchdog.wait(timeout=1.0)
    return exit_code


def parse_args(argv: Sequence[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, required=False)
    parser.add_argument("--frontend", default="isolation")
    parser.add_argument("--watchdog", action="store_true")
    parser.add_argument("--reclaim", action="store_true")
    parser.add_argument("--run-uuid")
    parser.add_argument("--run-root", type=Path)
    parser.add_argument("--runtime-root", type=Path)
    parser.add_argument("--cgroup-path", type=Path)
    parser.add_argument("--supervisor-pid", type=int)
    parser.add_argument("--supervisor-start-time")
    parser.add_argument("command", nargs=argparse.REMAINDER)
    return parser.parse_args(argv)


def main(argv: Sequence[str]) -> int:
    args = parse_args(argv)
    repo_root = (args.repo_root or Path.cwd()).resolve()
    if args.reclaim:
        print(f"capture reclaim: {reclaim_stale(repo_root)} run(s)")
        return 0
    if args.watchdog:
        required = (
            args.run_uuid,
            args.run_root,
            args.runtime_root,
            args.cgroup_path,
            args.supervisor_pid,
            args.supervisor_start_time,
        )
        if any(value is None for value in required):
            raise SupervisorError("watchdog arguments are incomplete")
        return watchdog_main(
            args.run_root,
            args.runtime_root,
            args.frontend,
            uuid.UUID(args.run_uuid),
            args.cgroup_path,
            args.supervisor_pid,
            args.supervisor_start_time,
        )
    command = list(args.command)
    if command and command[0] == "--":
        command = command[1:]
    return run_capture(repo_root, args.frontend, command)


if __name__ == "__main__":
    try:
        raise SystemExit(main(sys.argv[1:]))
    except SupervisorError as error:
        print(f"capture supervisor: FAIL: {error}", file=sys.stderr)
        raise SystemExit(2)
