#!/usr/bin/env python3
"""Classify and record the Bevy production optical-GPU proof.

The standard gate invokes this helper after `cargo nextest` has run the one
ignored production-shader test. It turns the run log into a fail-closed receipt
so an ignored test cannot silently become either an optical claim or a missing
evidence path.

Exit codes: 0 for a passing or explicitly degraded proof, 1 for a blocking
classification, and 2 for a malformed invocation or missing proof input.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
from datetime import UTC, datetime
from pathlib import Path

NO_ADAPTER_SIGNATURE = "Unable to find a GPU!"
RECEIPT_LINE = re.compile(
    r"optical-gpu receipt:\s*adapter=(?P<adapter>.*?)\s+operations=(?P<operations>[A-Za-z,]+)"
    r"\s+max_nested_layers=(?P<nested>\d+)\s+max_blur_px=(?P<blur>\d+)"
    r"\s+nesting_layers_proven=(?P<nesting_proven>true|false)"
)
OPERATIONS = (
    "BackdropSampling",
    "Blur",
    "Refraction",
    "Dispersion",
    "Specular",
    "Grain",
)
DIGEST_FILES = (
    "Cargo.lock",
    "Cargo.toml",
    "rust-toolchain.toml",
    ".cargo/config.toml",
    "crates/taskmanager-bevy-ui/Cargo.toml",
    "crates/taskmanager-bevy-ui/src/glass.rs",
    "crates/taskmanager-bevy-ui/src/glass/optical.rs",
    "crates/taskmanager-bevy-ui/src/glass/optical/capability.rs",
    "crates/taskmanager-bevy-ui/src/glass/optical/scene.rs",
    "crates/taskmanager-bevy-ui/src/glass/optical_glass.wesl",
    "crates/taskmanager-bevy-ui/src/palette.rs",
    "crates/taskmanager-bevy-ui/src/window/backdrop_capture.rs",
    "crates/taskmanager-bevy-ui/tests/gui/optical_gpu.rs",
    "crates/taskmanager-bevy-ui/tests/headless/optical_capability.rs",
    "crates/taskmanager-theme/src/optical_glass.rs",
    "scripts/quality/bevy-optical-gpu-gate.sh",
    "scripts/quality/bevy_optical_gpu_receipt.py",
    "scripts/quality/bevy_optical_capability_guard.py",
    "scripts/quality/local-gates.sh",
)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def source_digest(repo: Path) -> str:
    """Return a stable digest of every file that can change the proof."""
    lines: list[str] = []
    for relative in DIGEST_FILES:
        path = repo / relative
        if not path.is_file():
            raise FileNotFoundError(f"missing optical proof input: {relative}")
        lines.append(f"{sha256_file(path)}  {relative}")
    return hashlib.sha256(("\n".join(lines) + "\n").encode("utf-8")).hexdigest()


def parse_run_log(log: str) -> dict[str, str] | None:
    """Return the fields from the proof test's machine-readable receipt line."""
    line = RECEIPT_LINE.search(log)
    return line.groupdict() if line is not None else None


def classify(
    *,
    mode: str,
    exit_code: int,
    discovered: bool,
    log: str,
) -> dict[str, object]:
    """Classify one optical proof invocation without mutating receipts."""
    if mode not in {"auto", "0", "1"}:
        raise ValueError("mode must be auto, 0, or 1")

    if mode == "0":
        return {
            "verdict": "SKIPPED",
            "blocking": False,
            "reason": "TM_OPTICAL_GPU_GATE=0 explicitly disabled the proof",
            "runtime_capability": "PAINTED (unproven)",
            "operations_proven": [],
            "max_nested_layers": 0,
            "max_blur_px": 0,
            "adapter": None,
            "nesting_layers_proven": False,
        }

    if not discovered:
        return {
            "verdict": "REGRESSION",
            "blocking": True,
            "reason": "the named GPU proof was not discovered as an ignored test",
            "runtime_capability": "PAINTED (unproven)",
            "operations_proven": [],
            "max_nested_layers": None,
            "max_blur_px": None,
            "adapter": None,
            "nesting_layers_proven": False,
        }

    parsed = parse_run_log(log)
    if exit_code == 0:
        if parsed is None:
            return {
                "verdict": "REGRESSION",
                "blocking": True,
                "reason": "the GPU proof passed without emitting its receipt line",
                "runtime_capability": "PAINTED (unproven)",
                "operations_proven": [],
                "max_nested_layers": None,
                "max_blur_px": None,
                "adapter": None,
                "nesting_layers_proven": False,
            }
        operations = parsed["operations"].split(",")
        if operations != list(OPERATIONS):
            return {
                "verdict": "REGRESSION",
                "blocking": True,
                "reason": "the GPU proof receipt does not list the complete operation set",
                "runtime_capability": "PAINTED (unproven)",
                "operations_proven": operations,
                "max_nested_layers": int(parsed["nested"]),
                "max_blur_px": int(parsed["blur"]),
                "adapter": parsed["adapter"],
                "nesting_layers_proven": parsed["nesting_proven"] == "true",
            }
        if "device_type: Cpu" in parsed["adapter"]:
            return {
                "verdict": "SOFTWARE_ADAPTER",
                "blocking": mode == "1",
                "reason": (
                    "the proof ran on a CPU/software adapter; the product keeps its "
                    "typed Painted fallback"
                ),
                "runtime_capability": "PAINTED (software adapter)",
                "operations_proven": operations,
                "max_nested_layers": int(parsed["nested"]),
                "max_blur_px": int(parsed["blur"]),
                "adapter": parsed["adapter"],
                "nesting_layers_proven": parsed["nesting_proven"] == "true",
            }
        return {
            "verdict": "PROVEN",
            "blocking": False,
            "reason": "the production WESL proved every required optical operation",
            "runtime_capability": "OPTICAL (proven)",
            "operations_proven": operations,
            "max_nested_layers": int(parsed["nested"]),
            "max_blur_px": int(parsed["blur"]),
            "adapter": parsed["adapter"],
            "nesting_layers_proven": parsed["nesting_proven"] == "true",
        }

    if NO_ADAPTER_SIGNATURE in log:
        return {
            "verdict": "NO_ADAPTER",
            "blocking": mode == "1",
            "reason": (
                "wgpu selected no adapter; the renderer reports UNSUPPORTED and "
                "the theme returns Painted"
            ),
            "runtime_capability": "PAINTED (unproven)",
            "operations_proven": [],
            "max_nested_layers": 0,
            "max_blur_px": 0,
            "adapter": None,
            "nesting_layers_proven": False,
        }

    reason = "the ignored GPU proof failed; this is a production optical regression"
    if parsed is not None:
        reason = "the ignored GPU proof failed after emitting its receipt line"
    return {
        "verdict": "REGRESSION",
        "blocking": True,
        "reason": reason,
        "runtime_capability": "PAINTED (unproven)",
        "operations_proven": parsed["operations"].split(",") if parsed else [],
        "max_nested_layers": int(parsed["nested"]) if parsed else None,
        "max_blur_px": int(parsed["blur"]) if parsed else None,
        "adapter": parsed["adapter"] if parsed else None,
        "nesting_layers_proven": parsed["nesting_proven"] == "true" if parsed else False,
    }


def write_receipt(
    path: Path,
    *,
    repo: Path,
    mode: str,
    result: dict[str, object],
    exit_code: int,
    discovery_log: str,
    run_log: str,
) -> None:
    """Atomically write the private standard-gate receipt."""
    try:
        git_head = subprocess.check_output(
            ["git", "rev-parse", "HEAD"],
            cwd=repo,
            text=True,
            stderr=subprocess.DEVNULL,
            timeout=10,
        ).strip()
    except (OSError, subprocess.SubprocessError):
        git_head = "unknown"

    receipt = {
        "schema_version": 1,
        "kind": "bevy-optical-gpu-proof",
        "generated_at": datetime.now(UTC).isoformat(timespec="seconds"),
        "mode": mode,
        "nextest_exit_code": exit_code,
        "git_head": git_head,
        "source_digest": source_digest(repo),
        "test": {
            "package": "taskmanager-bevy-ui",
            "target": "lib",
            "name": "production_shader_renders_optical_effects_and_tracks_the_backdrop",
        },
        **result,
        "discovery_excerpt": first_nonempty(discovery_log),
        "run_excerpt": first_nonempty(run_log),
    }
    path.parent.mkdir(parents=True, exist_ok=True)
    temp = path.with_suffix(path.suffix + f".tmp.{os.getpid()}")
    temp.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    os.replace(temp, path)


def first_nonempty(text: str) -> str:
    for line in reversed(text.splitlines()):
        value = line.strip()
        if value and "optical-gpu receipt:" not in value:
            return value
    return ""


def cmd_digest(args: argparse.Namespace) -> int:
    print(source_digest(args.repo_root.resolve()))
    return 0


def cmd_write(args: argparse.Namespace) -> int:
    repo = args.repo_root.resolve()
    run_text = Path(args.run_log).read_text(encoding="utf-8", errors="replace")
    discovery_text = Path(args.discovery_log).read_text(encoding="utf-8", errors="replace")
    result = classify(
        mode=args.mode,
        exit_code=args.exit_code,
        discovered=args.discovered,
        log=run_text,
    )
    write_receipt(
        Path(args.receipt),
        repo=repo,
        mode=args.mode,
        result=result,
        exit_code=args.exit_code,
        discovery_log=discovery_text,
        run_log=run_text,
    )
    verdict = str(result["verdict"])
    capability = str(result["runtime_capability"])
    reason = str(result["reason"])
    if bool(result["blocking"]):
        print(f"bevy-optical-gpu: FAIL ({verdict}) {reason}", file=sys.stderr)
        return 1
    if verdict == "PROVEN":
        print(f"bevy-optical-gpu: PASS ({capability}) {reason}")
        return 0
    print(f"bevy-optical-gpu: PASS-DEGRADED ({verdict}) {capability}; {reason}")
    return 0


def self_test() -> int:
    receipt_line = (
        'optical-gpu receipt: adapter=AdapterInfo { name: "GPU", '
        "device_type: DiscreteGpu, backend: Vulkan } "
        "operations=BackdropSampling,Blur,Refraction,Dispersion,Specular,Grain "
        "max_nested_layers=1 max_blur_px=64 nesting_layers_proven=false"
    )
    no_adapter = f"panicked at renderer/mod.rs:1: {NO_ADAPTER_SIGNATURE}"
    failure = "assertion failed: blur should suppress high-frequency backdrop detail"

    cases = [
        ("auto", 0, True, receipt_line, "PROVEN", "OPTICAL (proven)", False),
        ("1", 0, True, receipt_line, "PROVEN", "OPTICAL (proven)", False),
        ("auto", 1, True, no_adapter, "NO_ADAPTER", "PAINTED (unproven)", False),
        ("1", 1, True, no_adapter, "NO_ADAPTER", "PAINTED (unproven)", True),
        ("auto", 1, True, failure, "REGRESSION", "PAINTED (unproven)", True),
        ("auto", 0, False, "", "REGRESSION", "PAINTED (unproven)", True),
        ("auto", 0, True, "all tests ok", "REGRESSION", "PAINTED (unproven)", True),
        ("0", 0, True, receipt_line, "SKIPPED", "PAINTED (unproven)", False),
    ]
    for mode, exit_code, discovered, log, verdict, capability, blocking in cases:
        result = classify(mode=mode, exit_code=exit_code, discovered=discovered, log=log)
        if (
            result["verdict"] != verdict
            or result["runtime_capability"] != capability
            or bool(result["blocking"]) != blocking
        ):
            print(f"self-test FAIL: {mode}/{exit_code}/{discovered} -> {result}")
            return 1

    software = (
        'optical-gpu receipt: adapter=AdapterInfo { name: "llvmpipe", '
        "device_type: Cpu, backend: Vulkan } "
        "operations=BackdropSampling,Blur,Refraction,Dispersion,Specular,Grain "
        "max_nested_layers=1 max_blur_px=64 nesting_layers_proven=false"
    )
    result = classify(mode="auto", exit_code=0, discovered=True, log=software)
    if (
        result["verdict"] != "SOFTWARE_ADAPTER"
        or result["runtime_capability"] != "PAINTED (software adapter)"
        or bool(result["blocking"])
    ):
        print(f"self-test FAIL: software adapter classification: {result}")
        return 1
    result = classify(mode="1", exit_code=0, discovered=True, log=software)
    if not bool(result["blocking"]):
        print(f"self-test FAIL: forced mode must reject software adapter: {result}")
        return 1

    print("bevy-optical-gpu receipt self-test: PASS")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, default=Path(__file__).resolve().parents[2])
    sub = parser.add_subparsers(dest="command", required=True)

    digest = sub.add_parser("digest", help="print the proof-source digest")
    digest.set_defaults(handler=cmd_digest)

    write = sub.add_parser("write", help="classify a run and write its receipt")
    write.add_argument("--mode", choices=("auto", "0", "1"), required=True)
    write.add_argument("--discovery-log", type=Path, required=True)
    write.add_argument("--run-log", type=Path, required=True)
    write.add_argument("--exit-code", type=int, required=True)
    write.add_argument("--discovered", action="store_true")
    write.add_argument("--receipt", type=Path, required=True)
    write.set_defaults(handler=cmd_write)

    test = sub.add_parser("self-test", help="verify classification without a GPU")
    test.set_defaults(handler=lambda _args: self_test())

    args = parser.parse_args()
    try:
        return int(args.handler(args))
    except (FileNotFoundError, ValueError) as error:
        print(f"bevy-optical-gpu receipt: {error}", file=sys.stderr)
        return 2
    except OSError as error:
        print(f"bevy-optical-gpu receipt: I/O failed: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
