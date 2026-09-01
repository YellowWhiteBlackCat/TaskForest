#!/usr/bin/env python3
"""Prove that two private background capture runs cannot see each other."""

from __future__ import annotations

import argparse
import json
import os
import signal
import shutil
import subprocess
import sys
import time
import uuid
from dataclasses import dataclass
from pathlib import Path


class IsolationError(RuntimeError):
    """A fail-closed cross-run isolation failure."""


class IsolationUnavailable(IsolationError):
    """The host cannot provide the private capture prerequisites."""


@dataclass(frozen=True)
class HostBaseline:
    capture_bus_names: tuple[str, ...]
    wayland_socket_identity: tuple[int, int]
    kwin_state: tuple[str, ...]


@dataclass
class RunReceipt:
    process: subprocess.Popen[str]
    run_root: Path
    metadata: dict[str, str]


def command_output(command: list[str], env: dict[str, str]) -> str:
    result = subprocess.run(
        command,
        check=True,
        text=True,
        capture_output=True,
        env=env,
        timeout=10,
    )
    return result.stdout.strip()


def cgroup_v2_parent(pid: int) -> Path:
    """Return the current process's cgroup-v2 parent without creating state."""

    try:
        lines = Path(f"/proc/{pid}/cgroup").read_text(encoding="utf-8").splitlines()
    except (FileNotFoundError, PermissionError, OSError) as error:
        raise IsolationUnavailable(f"cannot inspect cgroup membership: {error}") from error
    for line in lines:
        hierarchy, separator, relative = line.partition(":")
        if separator:
            _controllers, separator, relative = relative.partition(":")
        if hierarchy == "0" and separator:
            return Path("/sys/fs/cgroup") / relative.lstrip("/")
    raise IsolationUnavailable("Linux cgroup v2 is unavailable")


def isolation_preflight() -> str | None:
    """Return an environment reason, or None when A/B capture can run."""

    if sys.platform != "linux":
        return "private capture supervision requires Linux"
    runtime_text = os.environ.get("XDG_RUNTIME_DIR")
    display = os.environ.get("WAYLAND_DISPLAY")
    if not runtime_text or not display:
        return "XDG_RUNTIME_DIR and WAYLAND_DISPLAY are required"
    socket = Path(runtime_text) / display
    if not socket.is_socket():
        return f"Wayland socket is unavailable: {socket}"
    missing = [
        command
        for command in ("dbus-run-session", "busctl", "qdbus6", "kwin_wayland", "niri")
        if shutil.which(command) is None
    ]
    if missing:
        return f"capture commands are unavailable: {', '.join(missing)}"

    try:
        parent = cgroup_v2_parent(os.getpid())
        if not parent.is_dir():
            return f"capture cgroup parent is unavailable: {parent}"
        if parent.stat().st_uid != os.getuid():
            return f"capture cgroup parent is not user-owned: {parent}"
        probe = parent / f"taskforest-capture-preflight-{uuid.uuid4().hex}"
        probe.mkdir(mode=0o700)
        probe.rmdir()
    except IsolationUnavailable as error:
        return str(error)
    except OSError as error:
        return f"capture cgroup parent is not writable: {parent}: {error}"
    return None


def capture_bus_names(env: dict[str, str]) -> tuple[str, ...]:
    try:
        output = command_output(["busctl", "--user", "list"], env)
    except (OSError, subprocess.SubprocessError):
        return ()
    names: list[str] = []
    for line in output.splitlines():
        fields = line.split()
        if not fields:
            continue
        name = fields[0]
        if any(token in name.lower() for token in ("taskforest", "yellowwhite", "capture")):
            names.append(name)
    return tuple(sorted(names))


def wayland_socket_identity(env: dict[str, str]) -> tuple[int, int]:
    runtime = Path(env.get("XDG_RUNTIME_DIR", f"/run/user/{os.getuid()}"))
    display = env.get("WAYLAND_DISPLAY", "wayland-0")
    socket = runtime / display
    stat = socket.stat()
    return stat.st_dev, stat.st_ino


def host_kwin_state(env: dict[str, str]) -> tuple[str, ...]:
    state: list[str] = []
    for method in ("activeOutputName", "currentDesktop"):
        try:
            state.append(command_output(["qdbus6", "org.kde.KWin", "/KWin", method], env))
        except (OSError, subprocess.SubprocessError):
            state.append("unavailable")
    try:
        state.append(
            command_output(
                [
                    "qdbus6",
                    "org.kde.KWin",
                    "/KWin",
                    "org.freedesktop.DBus.Properties.Get",
                    "org.kde.KWin",
                    "showingDesktop",
                ],
                env,
            )
        )
    except (OSError, subprocess.SubprocessError):
        state.append("unavailable")
    return tuple(state)


def host_baseline(env: dict[str, str]) -> HostBaseline:
    return HostBaseline(
        capture_bus_names=capture_bus_names(env),
        wayland_socket_identity=wayland_socket_identity(env),
        kwin_state=host_kwin_state(env),
    )


def read_metadata(path: Path) -> dict[str, str]:
    values: dict[str, str] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        key, separator, value = line.partition("=")
        if not separator or not key or key in values:
            raise IsolationError(f"invalid metadata line: {line!r}")
        values[key] = value
    return values


def process_tree(root_pid: int) -> set[int]:
    """Return the bounded descendant tree of one public capture launcher."""

    seen = {root_pid}
    frontier = [root_pid]
    for _ in range(32):
        if not frontier:
            break
        next_frontier: list[int] = []
        for pid in frontier:
            children_path = Path(f"/proc/{pid}/task/{pid}/children")
            try:
                children = children_path.read_text(encoding="ascii").split()
            except (FileNotFoundError, PermissionError, OSError):
                continue
            for token in children:
                try:
                    child_pid = int(token)
                except ValueError:
                    continue
                if child_pid not in seen:
                    seen.add(child_pid)
                    next_frontier.append(child_pid)
        frontier = next_frontier
    return seen


def find_run_root(repo_root: Path, launcher_pids: set[int]) -> Path | None:
    base = repo_root / "target" / "host-wayland-diagnostic" / "runs"
    if not base.is_dir():
        return None
    for metadata_path in sorted(base.glob("*/supervisor.json")):
        try:
            metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            continue
        if metadata.get("supervisor_pid") in launcher_pids:
            return metadata_path.parent
    return None


def wait_for_receipt(repo_root: Path, process: subprocess.Popen[str]) -> RunReceipt:
    run_root: Path | None = None
    for _ in range(900):
        run_root = find_run_root(repo_root, process_tree(process.pid))
        if run_root is not None:
            break
        if process.poll() is not None:
            break
        time.sleep(0.1)
    if run_root is None:
        raise IsolationError(f"supervisor {process.pid} did not publish its run identity")
    try:
        supervisor_payload = json.loads(
            (run_root / "supervisor.json").read_text(encoding="utf-8")
        )
    except (OSError, json.JSONDecodeError) as error:
        raise IsolationError(f"invalid supervisor receipt: {run_root}") from error
    metadata = {key: str(value) for key, value in supervisor_payload.items()}
    validation = run_root / "validation.json"
    for _ in range(1200):
        if validation.is_file():
            try:
                payload = json.loads(validation.read_text(encoding="utf-8"))
            except json.JSONDecodeError as error:
                raise IsolationError(f"invalid validation receipt: {validation}") from error
            if payload.get("status") == "pass":
                metadata.update(read_metadata(run_root / "metadata.txt"))
                return RunReceipt(process=process, run_root=run_root, metadata=metadata)
        if process.poll() is not None:
            break
        time.sleep(0.1)
    log = (run_root / "stdout.log").read_text(encoding="utf-8", errors="replace") if (run_root / "stdout.log").is_file() else ""
    raise IsolationError(f"run {run_root} did not reach validation pass\n{log[-2000:]}")


def launch(repo_root: Path, test_root: Path, label: str) -> subprocess.Popen[str]:
    log = test_root / f"{label}.log"
    environment = os.environ.copy()
    for key in (
        "TM_CAPTURE_SUPERVISED",
        "TM_CAPTURE_SUPERVISOR_TOKEN",
        "TM_CAPTURE_PRIVATE_DBUS",
        "TM_PRIVATE_KWIN_BUS",
        "TM_CAPTURE_RUN_UUID",
        "TM_CAPTURE_RUN_ROOT",
        "TM_CAPTURE_RUNTIME_ROOT",
    ):
        environment.pop(key, None)
    environment.update(
        {
            "TM_CAPTURE_NIRI_BACKGROUND": "1",
            "TM_HOST_WAYLAND_PRIVATE_KWIN": "1",
            "TM_HOST_WAYLAND_SCENARIO": "standard",
            "TM_CAPTURE_ISOLATION_HOLD_SECONDS": "30",
        }
    )
    handle = log.open("w", encoding="utf-8")
    process = subprocess.Popen(
        ["bash", "scripts/capture-host-wayland-diagnostic.sh"],
        cwd=repo_root,
        env=environment,
        stdin=subprocess.DEVNULL,
        stdout=handle,
        stderr=subprocess.STDOUT,
        text=True,
        start_new_session=True,
    )
    handle.close()
    return process


def terminate(receipt: RunReceipt) -> None:
    terminate_process(receipt.process)


def terminate_process(process: subprocess.Popen[str]) -> None:
    if process.poll() is None:
        process.send_signal(signal.SIGTERM)
        try:
            process.wait(timeout=20)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)


def assert_same_host_state(baseline: HostBaseline, env: dict[str, str]) -> None:
    current = host_baseline(env)
    if current != baseline:
        raise IsolationError(f"host state changed: before={baseline!r} after={current!r}")


def assert_run_identity(run: RunReceipt, other: RunReceipt) -> None:
    own = run.metadata
    foreign = other.metadata
    for field in ("run_uuid", "run_root", "runtime_root", "cgroup_path"):
        if own.get(field) == foreign.get(field):
            raise IsolationError(f"A/B share {field}: {own.get(field)!r}")
    window_info = (run.run_root / "window-info.txt").read_text(encoding="utf-8")
    if own.get("app_pid") not in window_info:
        raise IsolationError(f"{run.run_root}: KWin receipt lacks its own app PID")
    if foreign.get("app_pid") in window_info:
        raise IsolationError(f"{run.run_root}: KWin receipt sees the foreign app PID")


def assert_cleaned(run: RunReceipt) -> None:
    if run.process.poll() is None:
        raise IsolationError(f"supervisor remains alive: {run.process.pid}")
    status = run.run_root / "cleanup-status.txt"
    if not status.is_file() or "residue=none" not in status.read_text(encoding="utf-8"):
        raise IsolationError(f"cleanup receipt is not clean: {status}")
    runtime_root = Path(run.metadata["runtime_root"])
    cgroup_path = Path(run.metadata["cgroup_path"])
    if runtime_root.exists():
        raise IsolationError(f"runtime residue remains: {runtime_root}")
    if cgroup_path.exists():
        raise IsolationError(f"cgroup residue remains: {cgroup_path}")


def run(repo_root: Path) -> Path:
    if (reason := isolation_preflight()) is not None:
        raise IsolationUnavailable(reason)
    test_uuid = uuid.uuid4()
    test_root = repo_root / "target" / "capture-isolation" / "tests" / str(test_uuid)
    test_root.mkdir(parents=True, mode=0o700)
    environment = os.environ.copy()
    baseline = host_baseline(environment)
    first: RunReceipt | None = None
    second: RunReceipt | None = None
    first_process = launch(repo_root, test_root, "a")
    second_process: subprocess.Popen[str] | None = None
    try:
        second_process = launch(repo_root, test_root, "b")
        first = wait_for_receipt(repo_root, first_process)
        second = wait_for_receipt(repo_root, second_process)
        assert_run_identity(first, second)
        assert_run_identity(second, first)
        assert_same_host_state(baseline, environment)

        terminate(first)
        if second.process.poll() is not None:
            raise IsolationError("B exited when only A was terminated")
        assert_same_host_state(baseline, environment)
    finally:
        if first is not None:
            terminate(first)
        else:
            terminate_process(first_process)
        if second is not None:
            terminate(second)
        elif second_process is not None:
            terminate_process(second_process)
    if first is None or second is None:
        raise IsolationError("both runs were not established")
    assert_cleaned(first)
    assert_cleaned(second)
    assert_same_host_state(baseline, environment)
    (test_root / "result.txt").write_text("status=pass\n", encoding="utf-8")
    return test_root


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, required=True)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--preflight", action="store_true")
    args = parser.parse_args(argv)
    if args.self_test:
        assert uuid.UUID(str(uuid.uuid4()))
        print("capture isolation self-test: PASS")
        return 0
    if args.preflight:
        reason = isolation_preflight()
        if reason is None:
            print("capture isolation preflight: READY")
            return 0
        print(f"capture isolation preflight: UNAVAILABLE: {reason}")
        return 3
    try:
        output = run(args.repo_root.resolve())
    except (IsolationError, OSError, subprocess.SubprocessError) as error:
        print(f"capture isolation: FAIL: {error}", file=sys.stderr)
        return 1
    print(f"capture isolation: PASS -> {output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
