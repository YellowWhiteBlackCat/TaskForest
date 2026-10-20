#!/usr/bin/env python3
"""Check that the vendored Bevy scene gate is the shared implementation and bites.

Two properties are verified, because a gate that does neither is decoration:

1. The vendored gate file is byte-identical to the one the shared package
   `zhuge-ui-kit` ships, pinned by the digest recorded in the ratchet ledger.
2. The gate fails on a new px literal that is not in the ledger, and passes on
   the recorded debt it was written against.

Run: ``python3 scripts/quality/test_bevy_scene_gate.py``
"""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
import tempfile
from pathlib import Path

REPOSITORY = Path(__file__).resolve().parents[2]
GATE = REPOSITORY / "scripts/quality/bevy_scene_gate.py"
LEDGER = REPOSITORY / "scripts/quality/bevy_scene_ratchet.json"


def run_gate(*arguments: str) -> subprocess.CompletedProcess[str]:
    """Run the vendored gate and capture its verdict."""

    return subprocess.run(
        [sys.executable, str(GATE), *arguments],
        capture_output=True,
        text=True,
        check=False,
        timeout=120,
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


def check_gate_bites() -> None:
    """Recorded debt passes; a new px literal outside the ledger fails."""

    with tempfile.TemporaryDirectory(prefix="bevy-scene-gate-") as raw:
        workspace = Path(raw)
        tree = workspace / "src"
        tree.mkdir()
        (tree / "page.rs").write_text(
            "fn scene() { bsn! { Node { width: px(4.0) } } }\n", encoding="utf-8"
        )
        ledger = workspace / "ledger.json"
        written = run_gate("--root", str(tree), "--write-baseline", str(ledger))
        if written.returncode != 0:
            raise RuntimeError(f"gate could not write a ledger: {written.stderr}")
        passing = run_gate("--root", str(tree), "--baseline", str(ledger))
        if passing.returncode != 0:
            raise RuntimeError(f"recorded debt must pass: {passing.stderr}")
        (tree / "other.rs").write_text(
            "fn scene() { bsn! { Node { height: px(8.0) } } }\n", encoding="utf-8"
        )
        failing = run_gate("--root", str(tree), "--baseline", str(ledger))
        if failing.returncode == 0:
            raise RuntimeError("a new px literal outside the ledger must fail")
        if "UIK-REM-001" not in failing.stderr:
            raise RuntimeError(f"unexpected failure output: {failing.stderr}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.parse_args()
    check_digest()
    check_gate_bites()
    print("Bevy scene gate: vendored copy pinned and biting — PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
