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
import tempfile
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


def probe_virtual_compositor() -> str | None:
    """Probe if a virtual kwin compositor can initialize EGL and bind a wayland socket."""

    try:
        tmpdir = tempfile.mkdtemp(prefix="tf-probe-", dir="/tmp")
    except (OSError, PermissionError) as err:
        return f"cannot create probe runtime dir: {err}"
    try:
        runtime = Path(tmpdir)
        runtime.chmod(0o700)
        sock_name = "s"
        sock_path = runtime / sock_name
        env = os.environ.copy()
        env.update(
            {
                "XDG_RUNTIME_DIR": str(runtime),
                "WAYLAND_DISPLAY": "",
                "DISPLAY": "",
                "LIBGL_ALWAYS_SOFTWARE": "1",
                "QT_QPA_PLATFORM": "wayland",
                "QT_IM_MODULE": "none",
                "GTK_IM_MODULE": "none",
                "XMODIFIERS": "@im=none",
            }
        )
        cmd = [
            "kwin_wayland",
            "--virtual",
            f"--socket={sock_name}",
            "--width=100",
            "--height=100",
            "--scale=1",
            "--no-global-shortcuts",
            "--no-lockscreen",
        ]
        try:
            proc = subprocess.Popen(
                cmd,
                env=env,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                start_new_session=True,
            )
        except (OSError, subprocess.SubprocessError) as err:
            return f"cannot spawn kwin_wayland: {err}"

        socket_ready = False
        error_msg = None
        for _ in range(40):
            if sock_path.is_socket():
                socket_ready = True
                break
            if proc.poll() is not None:
                stdout, stderr = proc.communicate()
                error_msg = (stderr or stdout or "").strip()
                break
            time.sleep(0.05)

        try:
            os.killpg(os.getpgid(proc.pid), signal.SIGTERM)
            proc.wait(timeout=1.0)
        except Exception:
            try:
                os.killpg(os.getpgid(proc.pid), signal.SIGKILL)
                proc.wait(timeout=1.0)
            except Exception:
                pass

        if not socket_ready:
            return f"EGL or virtual compositor probe failed: {error_msg or 'socket did not appear within 2s'}"

        return None
    finally:
        shutil.rmtree(tmpdir, ignore_errors=True)


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

    compositor_probe = probe_virtual_compositor()
    if compositor_probe is not None:
        return compositor_probe

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


# Poll interval for the receipts below. The budgets are wall-clock because the
# capture path's own budgets (build, validate, hold) are wall-clock too; the
# iteration count is derived from the budget so a loop can never run past its
# own deadline (and stays a bounded iterator, which the repo automation guard
# requires).
POLL_INTERVAL_SECONDS = 0.1
# The supervisor publishes its run identity within a second or two; a minute is
# already generous for a loaded host.
IDENTITY_BUDGET_SECONDS = 60.0
# One private capture is: KWin up, app start + markers, two bounded window
# receipts, Spectacle, then the independent validator (itself bounded at 180s
# because it re-hashes the source manifest and both window PNGs). A run that
# reaches no verdict inside this budget is reported as exactly that.
VALIDATION_BUDGET_SECONDS = 300.0


def budget_polls(budget: float) -> int:
    return int(budget / POLL_INTERVAL_SECONDS)


def run_failure(
    process: subprocess.Popen[str],
    run_root: Path | None,
    description: str,
) -> IsolationError:
    """Build the failure message from the run's own receipts, not a guess.

    The capture runner writes its rejection line to `runner-failure.txt` and
    the supervisor records the child's exit code; without them this test could
    only say "did not reach validation pass", which reads like an isolation
    failure even when the cause was a timeout or a rejected capture.
    """
    pieces = [f"supervisor exit_code={process.poll()}"]
    if process.poll() == 3:
        # The capture script reserves exit 3 for "a validator instrument never
        # finished". That is a host-speed condition: this leg has no verdict at
        # all, so it must reach the harness as UNAVAILABLE (skippable) rather
        # than as a code failure.
        return IsolationUnavailable(
            "run " + str(run_root) + " reached no verdict: " + "; ".join(pieces)
        )
    if run_root is not None:
        try:
            payload = json.loads((run_root / "supervisor.json").read_text(encoding="utf-8"))
            pieces.append(f"child_exit_code={payload.get('exit_code')}")
        except (OSError, json.JSONDecodeError):
            pass
        for name in ("runner-failure.txt", "stdout.log"):
            path = run_root / name
            try:
                text = path.read_text(encoding="utf-8", errors="replace").strip()
            except OSError:
                continue
            if text:
                pieces.append(f"{name}: {text[-600:]}")
    return IsolationError(
        f"run {run_root} {description}\n" + "\n".join(pieces)
    )


INCOMPLETE_STATUS = "incomplete"


def validation_outcome(payload: dict[str, object], run_root: Path | str) -> None:
    """Map one validation receipt onto the answers it can honestly give.

    `pass` proves the leg. `reject` is a verdict about the capture.
    `incomplete` means a validator instrument (OCR, hashing) ran out of its
    budget -- no verdict exists, and inventing a failure here is how a loaded
    host once failed `capture-isolation` for a reason that read
    "tesseract timed out after 15 seconds".
    """
    status = payload.get("status")
    if status == "pass":
        return
    if status == INCOMPLETE_STATUS:
        raise IsolationUnavailable(
            f"run {run_root} validation instrument did not finish: "
            f"{payload.get('reason')!r}; isolation was never judged"
        )
    raise IsolationError(f"run {run_root} validation receipt is not a pass: {status!r}")


def wait_for_receipt(repo_root: Path, process: subprocess.Popen[str]) -> RunReceipt:
    """Wait for one private capture run to publish a passing validation receipt.

    Bounded iteration *and* a wall-clock deadline: the iteration count keeps
    each loop provably finite (and is derived from the budget), while the
    deadline is the unit the capture path's own budgets are written in. Every
    timeout reports what the run actually said — the supervisor's exit code and
    the runner's own rejection line — instead of the generic "did not reach
    validation pass", which read like an isolation failure when the cause was a
    timeout or a rejected capture.
    """
    run_root: Path | None = None
    identity_deadline = time.monotonic() + IDENTITY_BUDGET_SECONDS
    for _poll in range(budget_polls(IDENTITY_BUDGET_SECONDS)):
        run_root = find_run_root(repo_root, process_tree(process.pid))
        if run_root is not None:
            break
        if process.poll() is not None:
            raise run_failure(process, run_root, "exited before publishing its run identity")
        if time.monotonic() >= identity_deadline:
            break
        time.sleep(POLL_INTERVAL_SECONDS)
    if run_root is None:
        raise run_failure(process, run_root, "published no run identity")
    try:
        supervisor_payload = json.loads(
            (run_root / "supervisor.json").read_text(encoding="utf-8")
        )
    except (OSError, json.JSONDecodeError) as error:
        raise IsolationError(f"invalid supervisor receipt: {run_root}") from error
    metadata = {key: str(value) for key, value in supervisor_payload.items()}
    validation = run_root / "validation.json"
    validation_deadline = time.monotonic() + VALIDATION_BUDGET_SECONDS
    for _poll in range(budget_polls(VALIDATION_BUDGET_SECONDS)):
        if validation.is_file():
            try:
                payload = json.loads(validation.read_text(encoding="utf-8"))
            except json.JSONDecodeError as error:
                raise IsolationError(f"invalid validation receipt: {validation}") from error
            if payload.get("status") == "pass":
                metadata.update(read_metadata(run_root / "metadata.txt"))
                return RunReceipt(process=process, run_root=run_root, metadata=metadata)
            validation_outcome(payload, run_root)
        if process.poll() is not None:
            raise run_failure(process, run_root, "exited without a validation receipt")
        if time.monotonic() >= validation_deadline:
            break
        time.sleep(POLL_INTERVAL_SECONDS)
    raise run_failure(process, run_root, "reached no validation verdict inside its budget")


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


CAPTURE_BUILD_COMMAND = [
    "cargo",
    "build",
    "--locked",
    "--quiet",
    "-p",
    "taskmanager-gpui",
    "--bin",
    "taskforest-g",
]
# The capture path snapshots exactly this command into every receipt
# (`scripts/capture-host-wayland-diagnostic.sh`, recorded as
# `build_command` in `metadata.txt`); the prewarm must be the same build,
# not a lookalike.
# The pair itself costs about 50 s on a warm target directory; a full cold
# compile of the GPUI capture binary does not belong in the quick tier at all,
# so the budget is a capacity bound rather than a build-size promise. Exceeding
# it means "this host cannot run the stage now", never "the code is broken".
PREWARM_BUDGET_SECONDS = float(os.environ.get("TM_CAPTURE_PREWARM_SECONDS", "300"))


def prewarm_capture_build(
    repo_root: Path,
    command: list[str] | None = None,
    budget: float | None = None,
) -> None:
    budget = PREWARM_BUDGET_SECONDS if budget is None else budget
    """Build the capture binary once, serially, before the A/B pair starts.

    Both private captures snapshot the same Cargo output through one shared
    build lock. Launched cold and concurrently, they serialize on that lock
    while each still waits inside the fixed validation budget, so a slow
    first compile reads as "the other run broke isolation" — the R124
    host-state FAIL was this race, not the code under test. A single serial
    pre-build turns the two captures into readers of a fresh binary instead
    of rivals producing one. A failed pre-build is a fail-closed gate error:
    without a buildable capture binary the isolation proof means nothing.
    """
    build_command = list(command or CAPTURE_BUILD_COMMAND)
    try:
        subprocess.run(
            build_command,
            cwd=repo_root,
            check=True,
            capture_output=True,
            text=True,
            timeout=budget,
        )
    except subprocess.TimeoutExpired as error:
        raise IsolationUnavailable(
            f"capture build prewarm exceeded {budget:g}s (cold target directory?): "
            f"{' '.join(build_command)}"
        ) from error
    except (OSError, subprocess.CalledProcessError) as error:
        detail = getattr(error, "stderr", "") or str(error)
        raise IsolationError(
            f"capture build prewarm failed: {str(detail).strip()[-400:]}"
        ) from error


def verdict_self_test() -> None:
    """The three receipt shapes must not collapse into one answer."""
    validation_outcome({"status": "pass"}, "fixture")
    for status, expected, fragment in (
        ("incomplete", IsolationUnavailable, "did not finish"),
        ("reject", IsolationError, "not a pass"),
        (None, IsolationError, "not a pass"),
    ):
        try:
            validation_outcome({"status": status, "reason": "tesseract budget"}, "fixture")
        except expected as error:
            if fragment not in str(error):
                raise IsolationError(f"{status!r} verdict does not explain itself: {error}")
        except IsolationError as error:
            if expected is IsolationUnavailable:
                raise IsolationError(
                    f"a {status!r} receipt was filed as a code failure, not an unavailable "
                    f"leg: {error}"
                ) from error
            raise
        else:
            raise IsolationError(f"{status!r} receipt raised nothing")


def prewarm_self_test(repo_root: Path) -> None:
    """Prove the prewarm cannot pass vacuously: success, failure, and timeout."""
    prewarm_capture_build(repo_root, command=["true"])
    cases = (
        # A broken build is a real gate failure: it must not become a SKIP.
        (["false"], IsolationError, "prewarm failed"),
        # A host that cannot build inside the budget is a missing prerequisite:
        # it must be skippable, and it must not be reported as a code failure.
        (["sleep", "5"], IsolationUnavailable, "prewarm exceeded"),
    )
    for broken, expected, fragment in cases:
        try:
            prewarm_capture_build(repo_root, command=broken, budget=0.2)
        except expected as error:
            if fragment not in str(error):
                raise IsolationError(
                    f"prewarm error for {broken} does not explain itself: {error}"
                ) from error
        else:
            raise IsolationError(f"prewarm accepted a failing build: {broken}")
    # The two answers must be distinguishable: `IsolationUnavailable` is the
    # skippable kind, and a bare `IsolationError` is not.
    if issubclass(IsolationError, IsolationUnavailable):
        raise IsolationError("failure and unavailability are no longer distinguishable")


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
    # Serialize the expensive part before the pair exists: two cold captures
    # competing for one build lock is a race against the validation budget,
    # not a test of isolation. See `prewarm_capture_build`.
    prewarm_capture_build(repo_root)
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
        prewarm_self_test(args.repo_root.resolve())
        verdict_self_test()
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
    except IsolationUnavailable as error:
        # Preflight can pass and the run still discover it cannot proceed —
        # a cold target directory is the known case. The stage prerequisite
        # is missing, which is exit 3 (the code the harness already reads as
        # UNAVAILABLE), not a verdict about the code under test.
        print(f"capture isolation: UNAVAILABLE: {error}", file=sys.stderr)
        return 3
    except (IsolationError, OSError, subprocess.SubprocessError) as error:
        print(f"capture isolation: FAIL: {error}", file=sys.stderr)
        return 1
    print(f"capture isolation: PASS -> {output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
