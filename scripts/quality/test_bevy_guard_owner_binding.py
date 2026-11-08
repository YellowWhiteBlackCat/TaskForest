#!/usr/bin/env python3
"""Prove product Bevy guards bind the vendored shared scene executor."""

from __future__ import annotations

import ast
import hashlib
import json
from pathlib import Path

import re

REPOSITORY = Path(__file__).resolve().parents[2]
QUALITY = REPOSITORY / "scripts/quality"
GATE = QUALITY / "bevy_scene_gate.py"
LEDGER = QUALITY / "bevy_scene_ratchet.json"
PRODUCT_GUARDS = (
    QUALITY / "bevy_bsn_guard.py",
    QUALITY / "bevy_rem_guard.py",
    QUALITY / "bevy_world_access_guard.py",
)
OWNER_BINDING = re.compile(
    r"mask_noncode\s*=\s*(?:scene_owner|load_shared_scene_owner\(\))\.mask_noncode"
)
FORBIDDEN_DUPLICATES = {
    "_blank",
    "mask_rust",
    "scene_spans",
    "inside_scene",
    "matching_paren",
}
FORBIDDEN_LEXICAL_IMPORT = re.compile(
    r"from\s+(?:bevy_bsn_guard|rust_syntax)\s+import[^\n]*\bmask_noncode\b"
)


def check_ledger_pins_vendored_owner() -> None:
    payload = json.loads(LEDGER.read_text(encoding="utf-8"))
    recorded = payload.get("gate", {}).get("sha256")
    actual = hashlib.sha256(GATE.read_bytes()).hexdigest()
    if recorded != actual:
        raise RuntimeError(f"vendored scene executor digest drift: {recorded} != {actual}")


def check_products_import_owner() -> None:
    for path in PRODUCT_GUARDS:
        source = path.read_text(encoding="utf-8")
        tree = ast.parse(source, filename=str(path))
        local_definitions = {
            node.name
            for node in tree.body
            if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef))
        }
        duplicates = sorted(local_definitions & FORBIDDEN_DUPLICATES)
        if duplicates:
            raise RuntimeError(f"{path.name}: local lexical duplicates: {duplicates}")
        if "load_shared_scene_owner" not in source or "bevy_scene_gate.py" not in source:
            raise RuntimeError(f"{path.name}: does not bind the vendored shared executor")
        if OWNER_BINDING.search(source) is None:
            raise RuntimeError(f"{path.name}: mask_noncode is not bound to the shared executor")
        if FORBIDDEN_LEXICAL_IMPORT.search(source):
            raise RuntimeError(f"{path.name}: imports a product-local duplicate of the shared masker")


def main() -> int:
    check_ledger_pins_vendored_owner()
    check_products_import_owner()
    print("Bevy product guards: vendored shared lexical owner bound — PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
