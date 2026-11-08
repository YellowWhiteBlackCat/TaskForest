#!/usr/bin/env python3
"""Check that the vendored easing-ownership gate is the shared implementation and bites.

Two properties are verified, because a gate that does neither is decoration:

1. The vendored gate file is pinned by the digest recorded in the ratchet ledger,
   so an edited copy without a refreshed ledger is visible.
2. The gate fails on a new hand-spelled smoothstep ramp or a hand-rolled cubic-Bézier
   easing solve that is not in the ledger, and passes on the recorded debt it was
   written against. It also leaves the four neighbours alone: the owner's ramp read
   backwards, shader source with a WGSL `smoothstep` builtin, a Catmull-Rom control
   point (a curve evaluated, not solved), and a crate that does not depend on the
   shared owner at all.

Run: ``python3 scripts/quality/test_easing_owner_gate.py``
"""

from __future__ import annotations

import hashlib
import json
import subprocess
import sys
import tempfile
from pathlib import Path

REPOSITORY = Path(__file__).resolve().parents[2]
GATE = REPOSITORY / "scripts/quality/easing_owner_gate.py"
LEDGER = REPOSITORY / "scripts/quality/easing_owner_ratchet.json"
SOURCE = "zhuge-ui-kit:crates/zhuge-ui-core/tools/check_easing_owner.py"


def run_gate(*arguments: str) -> subprocess.CompletedProcess[str]:
    """Run the vendored gate and capture its verdict."""

    return subprocess.run(
        [sys.executable, str(GATE), *arguments],
        capture_output=True,
        text=True,
        check=False,
        timeout=300,
    )


def check_digest() -> None:
    """The ledger must pin the digest of the gate file that ships with it."""

    payload = json.loads(LEDGER.read_text(encoding="utf-8"))
    recorded = payload.get("gate", {}).get("sha256")
    if not recorded:
        raise RuntimeError("ratchet ledger does not record the gate digest")
    actual = hashlib.sha256(GATE.read_bytes()).hexdigest()
    if recorded != actual:
        raise RuntimeError(
            f"gate digest drift: ledger {recorded} != file {actual}; refresh the "
            "vendored gate and the ledger together"
        )
    source = payload.get("gate", {}).get("source")
    if source != SOURCE:
        raise RuntimeError(f"ledger points at an unexpected gate source: {source!r}")


def write_crate(workspace: Path, name: str, source: str, *, with_dependency: bool) -> None:
    """Lay out a crate directory the gate can judge dependency direction from."""

    crate = workspace / name
    (crate / "src").mkdir(parents=True)
    manifest = "[package]\nname = %r\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[dependencies]\n" % name
    if with_dependency:
        import tomllib

        workspace = (REPOSITORY / "Cargo.toml").read_text(encoding="utf-8")
        pinned = tomllib.loads(workspace)["workspace"]["dependencies"]["zhuge-ui-core"]
        if isinstance(pinned, str):
            version = pinned
        else:
            version = pinned["version"]
        manifest += "zhuge-ui-core = \"%s\"\n" % version
    (crate / "Cargo.toml").write_text(manifest, encoding="utf-8")
    (crate / "src" / "lib.rs").write_text(source, encoding="utf-8")


def check_gate_bites() -> None:
    """Recorded debt passes; a new ramp or easing solve outside the ledger fails."""

    with tempfile.TemporaryDirectory(prefix="easing-owner-gate-") as raw:
        workspace = Path(raw)
        consumer = workspace / "consumer"
        write_crate(
            workspace,
            "consumer",
            "fn coverage(t: f32) -> f32 {\n    t * t * (3.0 - 2.0 * t)\n}\n",
            with_dependency=True,
        )
        ledger = workspace / "ledger.json"
        written = run_gate("--root", str(consumer), "--write-baseline", str(ledger))
        if written.returncode != 0:
            raise RuntimeError(f"gate could not write a ledger: {written.stderr}")
        passing = run_gate("--root", str(consumer), "--baseline", str(ledger))
        if passing.returncode != 0:
            raise RuntimeError(f"recorded debt must pass: {passing.stderr}")

        source = consumer / "src" / "lib.rs"
        source.write_text(
            "fn falloff(distance: f32, blur: f32) -> f32 {\n"
            "    let t = distance / blur;\n"
            "    (1.0 - t * t * (3.0 - 2.0 * t)).clamp(0.0, 1.0)\n}\n",
            encoding="utf-8",
        )
        failing = run_gate("--root", str(consumer), "--baseline", str(ledger))
        if failing.returncode == 0:
            raise RuntimeError("a hand-spelled ramp outside the ledger must fail")
        if "UIK-EASE-001" not in failing.stderr:
            raise RuntimeError(f"unexpected failure output: {failing.stderr}")

        # The second byte-identical copy in another function of the same file is a
        # second copy of the fact, not the same ledger entry.
        source.write_text(
            "fn first(t: f32) -> f32 {\n    t * t * (3.0 - 2.0 * t)\n}\n"
            "fn second(t: f32) -> f32 {\n    t * t * (3.0 - 2.0 * t)\n}\n",
            encoding="utf-8",
        )
        twice = run_gate("--root", str(consumer), "--baseline", str(ledger), "--mode", "report")
        if "violations=2" not in twice.stdout:
            raise RuntimeError(
                f"two identical ramps in two functions must count twice, got: {twice.stdout}"
            )

        # A hand-rolled easing solve is the other half of the law.
        source.write_text(
            "fn solve(target: f32, x1: f32, x2: f32) -> f32 {\n"
            "    let mut parameter = target;\n"
            "    for _ in 0..8 {\n"
            "        let error = 3.0 * parameter * parameter * x1 + parameter * parameter * parameter - target;\n"
            "        let slope = 3.0 * x1 * parameter + 3.0 * parameter * parameter * x2;\n"
            "        parameter = (parameter - error / slope).clamp(0.0, 1.0);\n"
            "    }\n"
            "    parameter\n}\n",
            encoding="utf-8",
        )
        solving = run_gate("--root", str(consumer), "--baseline", str(ledger))
        if solving.returncode == 0:
            raise RuntimeError("a hand-rolled Bezier easing solve outside the ledger must fail")
        if "UIK-EASE-002" not in solving.stderr:
            raise RuntimeError(f"unexpected solve verdict: {solving.stderr}")

        # The legal neighbourhood: the owner's ramp read backwards, shader source,
        # and a curve that is only evaluated.
        source.write_text(
            "use zhuge_ui_core::easing::smoothstep;\n\n"
            "pub fn edge_coverage(t: f32) -> f32 {\n    (1.0 - smoothstep(t)).clamp(0.0, 1.0)\n}\n\n"
            "const WGSL: &str = r#\"\n"
            "fn ramp(t: f32) -> f32 {\n    return t * t * (3.0 - 2.0 * t);\n}\n"
            "let band = smoothstep(0.42, 0.5, phase);\n\"#;\n\n"
            "pub fn control_point(start: f32, previous: f32, tension: f32) -> f32 {\n"
            "    start + (previous - start) * tension\n}\n",
            encoding="utf-8",
        )
        legal = run_gate("--root", str(consumer), "--baseline", str(ledger))
        if legal.returncode != 0:
            raise RuntimeError(
                f"the owner call, shader source and an evaluated control point must be legal: "
                f"{legal.stderr}"
            )

        # A crate that never chose the dependency has no owner to call.
        outsider = workspace / "outsider"
        write_crate(
            workspace,
            "outsider",
            "fn coverage(t: f32) -> f32 {\n    t * t * (3.0 - 2.0 * t)\n}\n",
            with_dependency=False,
        )
        outside = run_gate("--root", str(outsider))
        if outside.returncode != 0:
            raise RuntimeError(
                f"a crate without the shared dependency must not be charged debt: {outside.stderr}"
            )


def main() -> int:
    check_digest()
    check_gate_bites()
    print("easing owner gate: vendored copy pinned and biting — PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
