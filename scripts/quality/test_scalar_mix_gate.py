#!/usr/bin/env python3
"""Check that the vendored scalar-mix gate is the shared implementation and bites.

Two properties are verified, because a gate that does neither is decoration:

1. The vendored gate file is pinned by the digest recorded in the ratchet ledger,
   so an edited copy without a refreshed ledger is visible.
2. The gate fails on a new hand-written scalar mix that is not in the ledger, and
   passes on the recorded debt it was written against. It also leaves the three
   neighbours alone: a component-wise color mix, a local wrapper that delegates to
   the owner with its own progress clamp, and a crate that does not depend on the
   shared owner at all.

Run: ``python3 scripts/quality/test_scalar_mix_gate.py``
"""

from __future__ import annotations

import hashlib
import json
import subprocess
import sys
import tempfile
from pathlib import Path

REPOSITORY = Path(__file__).resolve().parents[2]
GATE = REPOSITORY / "scripts/quality/scalar_mix_gate.py"
LEDGER = REPOSITORY / "scripts/quality/scalar_mix_ratchet.json"
SOURCE = "zhuge-ui-kit:crates/zhuge-ui-core/tools/check_scalar_mix.py"


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
        manifest += "zhuge-ui-core = \"0.1.14\"\n"
    (crate / "Cargo.toml").write_text(manifest, encoding="utf-8")
    (crate / "src" / "lib.rs").write_text(source, encoding="utf-8")


def check_gate_bites() -> None:
    """Recorded debt passes; a new hand-written mix outside the ledger fails."""

    with tempfile.TemporaryDirectory(prefix="scalar-mix-gate-") as raw:
        workspace = Path(raw)
        consumer = workspace / "consumer"
        write_crate(
            workspace,
            "consumer",
            "fn alpha(low: f32, high: f32, wave: f32) -> f32 {\n"
            "    low + (high - low) * wave\n}\n",
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
            "fn width(base: f32, minimum: f32, factor: f32) -> f32 {\n"
            "    minimum + (base - minimum) * factor\n}\n",
            encoding="utf-8",
        )
        failing = run_gate("--root", str(consumer), "--baseline", str(ledger))
        if failing.returncode == 0:
            raise RuntimeError("a hand-written scalar mix outside the ledger must fail")
        if "UIK-MIX-001" not in failing.stderr:
            raise RuntimeError(f"unexpected failure output: {failing.stderr}")

        # The owner call, with the surface's own progress policy, is the legal form.
        source.write_text(
            "use zhuge_ui_core::easing::lerp;\n\n"
            "pub fn lerp_clamped(start: f32, end: f32, t: f32) -> f32 {\n"
            "    lerp(start, end, t.clamp(0.0, 1.0))\n}\n\n"
            "pub fn tint(start: Rgba, end: Rgba, t: f32) -> Rgba {\n"
            "    Rgba {\n        r: start.r + (end.r - start.r) * t,\n    }\n}\n",
            encoding="utf-8",
        )
        legal = run_gate("--root", str(consumer), "--baseline", str(ledger))
        if legal.returncode != 0:
            raise RuntimeError(
                f"the owner call, a delegating wrapper and a component-wise mix must be legal: "
                f"{legal.stderr}"
            )

        # A crate that never chose the dependency has no owner to call.
        outsider = workspace / "outsider"
        write_crate(
            workspace,
            "outsider",
            "fn opacity(low: f32, high: f32, breath: f32) -> f32 {\n"
            "    low + (high - low) * breath\n}\n",
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
    print("scalar mix gate: vendored copy pinned and biting — PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
