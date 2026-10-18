#!/usr/bin/env python3
"""Validate TaskForest Flatpak manifest syntax, reverse-DNS naming, and required sandbox permissions."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path


EXPECTED_APP_ID = "io.github.YellowWhiteBlackCat.TaskForest"
REQUIRED_FINISH_ARGS = (
    "--share=ipc",
    "--socket=wayland",
    "--device=dri",
    "--filesystem=/proc:ro",
    "--filesystem=/sys:ro",
)


def validate_manifest(path: Path) -> list[str]:
    errors: list[str] = []
    if not path.is_file():
        return [f"Flatpak manifest not found: {path}"]

    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as err:
        return [f"Flatpak manifest JSON parse error: {err}"]

    app_id = data.get("app-id")
    if app_id != EXPECTED_APP_ID:
        errors.append(f"Expected app-id '{EXPECTED_APP_ID}', got '{app_id}'")

    command = data.get("command")
    if not command:
        errors.append("Manifest missing 'command' entry")

    finish_args = data.get("finish-args", [])
    if not isinstance(finish_args, list):
        errors.append("'finish-args' must be a list")
    else:
        for arg in REQUIRED_FINISH_ARGS:
            if arg not in finish_args:
                errors.append(f"Missing required finish-arg: '{arg}'")

    modules = data.get("modules", [])
    if not modules or not isinstance(modules, list):
        errors.append("Manifest missing 'modules' list")

    return errors


def self_test() -> int:
    valid_data = {
        "app-id": EXPECTED_APP_ID,
        "command": "taskforest-g",
        "finish-args": list(REQUIRED_FINISH_ARGS),
        "modules": [{"name": "taskforest"}],
    }
    import tempfile

    with tempfile.TemporaryDirectory() as tmp:
        p = Path(tmp) / "test.json"
        p.write_text(json.dumps(valid_data), encoding="utf-8")
        errors = validate_manifest(p)
        assert not errors, f"self-test failed on valid manifest: {errors}"

        # Test invalid app-id
        invalid = valid_data.copy()
        invalid["app-id"] = "com.wrong.app"
        p.write_text(json.dumps(invalid), encoding="utf-8")
        errors = validate_manifest(p)
        assert any("Expected app-id" in e for e in errors), "self-test failed to catch wrong app-id"

    print("Flatpak manifest validator self-test: PASS")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true", help="run unit self-tests")
    args = parser.parse_args()

    if args.self_test:
        return self_test()

    repo_root = Path(__file__).resolve().parents[2]
    manifest = repo_root / "packaging" / "flatpak" / f"{EXPECTED_APP_ID}.json"

    errors = validate_manifest(manifest)
    if errors:
        for err in errors:
            print(f"FAIL: {err}", file=sys.stderr)
        return 1

    print(f"Flatpak manifest {manifest.name}: PASS (app-id '{EXPECTED_APP_ID}', permissions verified)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
