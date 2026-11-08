#!/usr/bin/env python3
"""Check that the GPUI/UI px ratchet gate is wired to a live ledger and bites.

Three properties, because a gate that does neither is decoration:

1. The ratchet ledger records this exact gate file's digest.
2. The gate passes on the recorded stock (the two production trees, as the
   quick gate runs it).
3. On a scratch tree the gate fails on a new literal, fails when a ledgered
   count grows, and fails when ledgered debt disappears (shrink-the-ledger).

Run: ``python3 scripts/quality/test_gpui_px_gate.py``
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
GATE = REPOSITORY / "scripts/quality/gpui_px_gate.py"
LEDGER = REPOSITORY / "scripts/quality/gpui_px_ratchet.json"
PRODUCTION_ROOTS = [
    REPOSITORY / "crates/taskmanager-gpui/src/gpui_app",
    REPOSITORY / "crates/taskmanager-ui/src",
]


def run_gate(*arguments: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(GATE), *arguments],
        capture_output=True,
        text=True,
        check=False,
        timeout=120,
    )


def check_digest() -> None:
    payload = json.loads(LEDGER.read_text(encoding="utf-8"))
    recorded = payload.get("gate", {}).get("sha256")
    if not recorded:
        raise RuntimeError("px ratchet ledger does not record the gate digest")
    actual = hashlib.sha256(GATE.read_bytes()).hexdigest()
    if recorded != actual:
        raise RuntimeError(
            "px ratchet ledger pins a different gate digest; "
            "refresh the ledger with --write-baseline"
        )


def check_stock_passes() -> None:
    result = run_gate(
        *(f"--root={root}" for root in PRODUCTION_ROOTS),
        f"--baseline={LEDGER}",
    )
    if result.returncode != 0:
        raise RuntimeError(f"ledgered stock must pass: {result.stdout}{result.stderr}")


def check_bites() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        source = root / "widget.rs"
        ledger = root / "baseline.json"

        source.write_text("fn a() {\n    div().w(px(28.0));\n}\n", encoding="utf-8")
        written = run_gate(f"--root={root}", "--baseline", str(ledger), "--write-baseline")
        if written.returncode != 0:
            raise RuntimeError(f"write-baseline failed: {written.stdout}{written.stderr}")

        source.write_text(
            "fn a() {\n    div().w(px(28.0));\n    div().h(px(64.0));\n}\n",
            encoding="utf-8",
        )
        grown = run_gate(f"--root={root}", f"--baseline={ledger}")
        if grown.returncode == 0:
            raise RuntimeError("gate must fail on a new px literal")

        source.write_text("fn a() {\n    div().w(px(28.0));\n    div().w(px(28.0));\n}\n", encoding="utf-8")
        counted = run_gate(f"--root={root}", f"--baseline={ledger}")
        if counted.returncode == 0:
            raise RuntimeError("gate must fail when a ledgered combo's count grows")

        source.write_text("fn a() {\n    div().w(px(0.0));\n}\n", encoding="utf-8")
        cleared = run_gate(f"--root={root}", f"--baseline={ledger}")
        if cleared.returncode == 0:
            raise RuntimeError("gate must fail when ledgered debt disappears (shrink the ledger)")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    arguments = parser.parse_args()
    if arguments.self_test:
        return 0
    try:
        check_digest()
        check_stock_passes()
        check_bites()
    except RuntimeError as error:
        print(f"FAIL {error}")
        return 1
    print("gpui-px-gate-test: pass — ledger pinned, stock green, gate bites")
    return 0


if __name__ == "__main__":
    sys.exit(main())
