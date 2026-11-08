#!/usr/bin/env python3
"""Check that the vendored interval-projection gate is the shared implementation and bites.

Two properties are verified, because a gate that does neither is decoration:

1. The vendored gate file is pinned by the digest recorded in the ratchet ledger,
   so an edited copy without a refreshed ledger is visible.
2. The gate fails on a new hand-derived projection that is not in the ledger, and
   passes on the recorded debt it was written against. It also passes the owner
   call, and refuses to flag the two neighbours that are not this law: an
   unclamped ratio along a segment, and a paint coverage ramp.

Run: ``python3 scripts/quality/test_interval_normalization_gate.py``
"""

from __future__ import annotations

import hashlib
import json
import subprocess
import sys
import tempfile
from pathlib import Path

REPOSITORY = Path(__file__).resolve().parents[2]
GATE = REPOSITORY / "scripts/quality/interval_normalization_gate.py"
LEDGER = REPOSITORY / "scripts/quality/interval_normalization_ratchet.json"
SOURCE = "zhuge-ui-kit:crates/zhuge-ui-core/tools/check_interval_normalization.py"


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


def check_gate_bites() -> None:
    """Recorded debt passes; a new hand-written projection outside the ledger fails."""

    with tempfile.TemporaryDirectory(prefix="interval-gate-") as raw:
        workspace = Path(raw)
        tree = workspace / "src"
        tree.mkdir()
        (tree / "gauge.rs").write_text(
            "fn gauge(value: f32, min: f32, max: f32) -> f32 {\n"
            "    ((value - min) / (max - min)).clamp(0.0, 1.0)\n}\n",
            encoding="utf-8",
        )
        ledger = workspace / "ledger.json"
        written = run_gate("--root", str(tree), "--write-baseline", str(ledger))
        if written.returncode != 0:
            raise RuntimeError(f"gate could not write a ledger: {written.stderr}")
        passing = run_gate("--root", str(tree), "--baseline", str(ledger))
        if passing.returncode != 0:
            raise RuntimeError(f"recorded debt must pass: {passing.stderr}")

        # A projection split across statements is the same breach (UIK-VALUE-002).
        (tree / "split.rs").write_text(
            "fn badge(value: f32, floor: f32, ceiling: f32) -> f32 {\n"
            "    let span = ceiling - floor;\n"
            "    let raw = (value - floor) / span;\n"
            "    raw.clamp(0.0, 1.0)\n}\n",
            encoding="utf-8",
        )
        failing = run_gate("--root", str(tree), "--baseline", str(ledger))
        if failing.returncode == 0:
            raise RuntimeError("a hand-derived projection outside the ledger must fail")
        if "UIK-VALUE-002" not in failing.stderr:
            raise RuntimeError(f"unexpected failure output: {failing.stderr}")

        # A denominator renamed away from the vocabulary is still the projection.
        (tree / "split.rs").write_text(
            "fn gauge(value: f32, floor: f32, ceiling: f32) -> f32 {\n"
            "    let reach = ceiling - floor;\n"
            "    let shown = (value - floor) / reach;\n"
            "    shown.clamp(0.0, 1.0)\n}\n",
            encoding="utf-8",
        )
        renamed = run_gate("--root", str(tree), "--baseline", str(ledger))
        if renamed.returncode == 0 or "UIK-VALUE-003" not in renamed.stderr:
            raise RuntimeError(
                f"a renamed span must trip UIK-VALUE-003: rc={renamed.returncode} "
                f"{renamed.stderr}"
            )

        # The owner call is the legal form.
        (tree / "split.rs").write_text(
            "fn gauge(value: f32, floor: f32, ceiling: f32) -> f32 {\n"
            "    interval_fraction(value, floor, ceiling, 0.5)\n}\n",
            encoding="utf-8",
        )
        legal = run_gate("--root", str(tree), "--baseline", str(ledger))
        if legal.returncode != 0:
            raise RuntimeError(f"the owner call must be legal: {legal.stderr}")

        # The two neighbours are not this law: an unclamped ratio along a segment,
        # and a coverage ramp that inverts a pixel row into alpha.
        (tree / "split.rs").write_text(
            "fn stroke(xa: i32, xb: i32, x: i32, a_y: f32, b_y: f32) -> f32 {\n"
            "    let span = (xb - xa).abs().max(1) as f32;\n"
            "    let t = (x - xa).abs() as f32 / span;\n"
            "    a_y + (b_y - a_y) * t\n}\n",
            encoding="utf-8",
        )
        (tree / "wash.rs").write_text(
            "fn paint(row: f32, top: f32, baseline: f32) -> f32 {\n"
            "    let span = (baseline - top).max(1.0);\n"
            "    (1.0 - (row - top) / span).clamp(0.0, 1.0)\n}\n",
            encoding="utf-8",
        )
        neighbours = run_gate("--root", str(tree), "--baseline", str(ledger))
        if neighbours.returncode != 0:
            raise RuntimeError(
                f"a ratio along a segment and a coverage ramp are not this law: "
                f"{neighbours.stderr}"
            )


def main() -> int:
    check_digest()
    check_gate_bites()
    print("interval normalization gate: vendored copy pinned and biting — PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
