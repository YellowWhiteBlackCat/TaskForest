#!/usr/bin/env python3
"""Check that the vendored band-arithmetic gate is the shared implementation and bites.

Two properties are verified, because a gate that does neither is decoration:

1. The vendored gate file is pinned by the digest recorded in the ratchet ledger,
   so an edited copy without a refreshed ledger is visible.
2. The gate fails on a new hand-derived band or pool arithmetic that is not in the
   ledger, and passes on the recorded debt it was written against.

Run: ``python3 scripts/quality/test_band_arithmetic_gate.py``
"""

from __future__ import annotations

import hashlib
import json
import subprocess
import sys
import tempfile
from pathlib import Path

REPOSITORY = Path(__file__).resolve().parents[2]
GATE = REPOSITORY / "scripts/quality/band_arithmetic_gate.py"
LEDGER = REPOSITORY / "scripts/quality/band_arithmetic_ratchet.json"


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
    if source != "zhuge-ui-kit:crates/zhuge-ui-core/tools/check_band_arithmetic.py":
        raise RuntimeError(f"ledger points at an unexpected gate source: {source!r}")


def check_gate_bites() -> None:
    """Recorded debt passes; a new second band arithmetic outside the ledger fails."""

    with tempfile.TemporaryDirectory(prefix="band-arithmetic-gate-") as raw:
        workspace = Path(raw)
        tree = workspace / "src"
        tree.mkdir()
        (tree / "page.rs").write_text(
            "fn page() {\n    let capacity = (viewport / row).ceil() as usize + OVERSCAN * 2;\n}\n",
            encoding="utf-8",
        )
        ledger = workspace / "ledger.json"
        written = run_gate("--root", str(tree), "--write-baseline", str(ledger))
        if written.returncode != 0:
            raise RuntimeError(f"gate could not write a ledger: {written.stderr}")
        passing = run_gate("--root", str(tree), "--baseline", str(ledger))
        if passing.returncode != 0:
            raise RuntimeError(f"recorded debt must pass: {passing.stderr}")
        (tree / "other.rs").write_text(
            "fn proof() {\n    let bound = (viewport / row).ceil() as usize + 2;\n}\n",
            encoding="utf-8",
        )
        failing = run_gate("--root", str(tree), "--baseline", str(ledger))
        if failing.returncode == 0:
            raise RuntimeError("a new hand-derived mount bound outside the ledger must fail")
        if "UIK-BAND-002" not in failing.stderr:
            raise RuntimeError(f"unexpected failure output: {failing.stderr}")
        # The owner call is the legal form and must not trip the gate.
        (tree / "other.rs").write_text(
            "fn proof() {\n    let bound = max_band_rows(row, viewport, OVERSCAN);\n}\n",
            encoding="utf-8",
        )
        legal = run_gate("--root", str(tree), "--baseline", str(ledger))
        if legal.returncode != 0:
            raise RuntimeError(f"the owner call must be legal: {legal.stderr}")


def main() -> int:
    check_digest()
    check_gate_bites()
    print("band arithmetic gate: vendored copy pinned and biting — PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
