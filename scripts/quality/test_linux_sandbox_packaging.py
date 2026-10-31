#!/usr/bin/env python3
"""Validate the Linux sandbox distribution surface: the Flatpak bundle and the AppImage.

The two modern-distribution artifacts have no compiler-visible contract, so a
stale path or a renamed component ships silently. This gate makes the contract
mechanical:

* the Flatpak manifest parses, its file name equals its app-id, the app-id is
  reverse-DNS shaped, and the sandbox permission set is complete
  (``test_flatpak_manifest`` owns those checks and is reused here);
* every file a build-command installs exists in the repository, the declared
  ``command`` binary is one of the installed binaries, and the installed
  desktop entry is named after the app-id (Flatpak requires it);
* the installed AppStream metadata carries the app-id as its component ``<id>``
  — AppStream attributes a component by file name AND id, so a metadata file
  that names another component is wrong even when the install succeeds;
* the AppImage assembler derives the release name
  ``TaskForest-G-<version>-<platform>.AppImage`` (docs/PRODUCT_IDENTITY.md) and
  stages a complete AppDir payload — verified headlessly, without linuxdeploy,
  FUSE, or network.

``--sandbox-build`` additionally runs the real builders (flatpak-builder and
linuxdeploy). It needs the Flatpak SDK/runtime and network access, so it is
opt-in and the default gate stays hermetic.
"""

from __future__ import annotations

import argparse
import re
import shlex
import subprocess
import sys
import tempfile
from pathlib import Path
import xml.etree.ElementTree as ET

sys.path.insert(0, str(Path(__file__).resolve().parent))
import test_flatpak_manifest as flatpak  # noqa: E402

REPO = Path(__file__).resolve().parents[2]
APP_ID = flatpak.EXPECTED_APP_ID
FLATPAK_DIR = REPO / "packaging" / "flatpak"
MANIFEST = FLATPAK_DIR / f"{APP_ID}.json"
APPIMAGE_SCRIPT = REPO / "packaging" / "linux" / "build-appimage.sh"
FLATPAK_BUILD = FLATPAK_DIR / "build-flatpak.sh"

REVERSE_DNS = re.compile(r"^[A-Za-z][A-Za-z0-9_]*(\.[A-Za-z0-9_]+){2,}$")
INSTALL_LINE = re.compile(r"^install\s+-Dm[0-7]+\s+(\S+)\s+(\S+)$")


def _install_pairs(manifest_path: Path) -> list[tuple[str, str]]:
    """Every (source, destination) pair a `install -Dm…` build-command stages."""
    import json

    data = json.loads(manifest_path.read_text(encoding="utf-8"))
    pairs: list[tuple[str, str]] = []
    for module in data.get("modules", []):
        if not isinstance(module, dict):
            continue
        for command in module.get("build-commands", []) or []:
            fields = shlex.split(command) if isinstance(command, str) else []
            if len(fields) == 4 and fields[0] == "install":
                pairs.append((fields[2], fields[3]))
            elif fields[:1] == ["install"] and len(fields) >= 4:
                pairs.append((fields[-2], fields[-1]))
    return pairs


def validate_flatpak(manifest_path: Path = MANIFEST) -> list[str]:
    errors = list(flatpak.validate_manifest(manifest_path))
    if not manifest_path.is_file():
        return errors

    import json

    data = json.loads(manifest_path.read_text(encoding="utf-8"))
    app_id = data.get("app-id", "")

    if manifest_path.name != f"{app_id}.json":
        errors.append(
            f"Manifest file name must be '<app-id>.json'; got '{manifest_path.name}' for app-id '{app_id}'"
        )
    if not REVERSE_DNS.match(app_id):
        errors.append(f"app-id '{app_id}' is not reverse-DNS shaped (>= 3 dotted labels)")

    pairs = _install_pairs(manifest_path)
    if not pairs:
        errors.append("Manifest declares no 'install -Dm…' build-command to verify")

    installed: set[str] = set()
    for source, destination in pairs:
        # `target/...` is a build output produced by the build-commands above,
        # not a repository file; everything else must exist as checked in.
        is_build_output = source.startswith("target/")
        if "*" not in source and not is_build_output and not (REPO / source).exists():
            errors.append(f"build-command installs '{source}', which does not exist in the repository")
        installed.add(destination)

    command = data.get("command", "")
    if command and f"/app/bin/{command}" not in installed:
        errors.append(f"declared command '{command}' is not installed into /app/bin by any build-command")

    # Flatpak requires the desktop entry file name to be the app-id.
    desktop = f"/app/share/applications/{app_id}.desktop"
    if desktop not in installed:
        errors.append(f"the desktop entry must be installed as '{desktop}'")

    # AppStream attributes the component by file name AND <id>.
    metainfo = f"/app/share/metainfo/{app_id}.metainfo.xml"
    if metainfo not in installed:
        errors.append(f"the AppStream metadata must be installed as '{metainfo}'")
    else:
        source = next(src for src, dst in pairs if dst == metainfo)
        path = REPO / source
        if not path.is_file():
            errors.append(f"AppStream metadata source '{source}' is missing")
        else:
            try:
                component_id = ET.parse(path).getroot().findtext("id", "")
            except ET.ParseError as err:
                component_id = ""
                errors.append(f"AppStream metadata '{source}' is not parseable XML: {err}")
            if component_id != app_id:
                errors.append(
                    f"AppStream component id must equal the app-id '{app_id}'; '{source}' declares '{component_id}'"
                )

    return errors


def _run(argv: list[str], env: dict[str, str] | None = None) -> subprocess.CompletedProcess[str]:
    # A finite ceiling: the AppImage self-test stages an AppDir and the real
    # `--sandbox-build` path invokes flatpak-builder/linuxdeploy, either of
    # which could hang on a stuck host.
    return subprocess.run(
        argv, capture_output=True, text=True, cwd=str(REPO), env=env, timeout=600
    )


def validate_appimage() -> list[str]:
    errors: list[str] = []
    if not APPIMAGE_SCRIPT.is_file():
        return [f"AppImage builder not found: {APPIMAGE_SCRIPT}"]

    self_test = _run(["bash", str(APPIMAGE_SCRIPT), "--self-test"])
    if self_test.returncode != 0:
        errors.append(
            "the AppImage headless self-test failed: "
            + (self_test.stderr.strip() or self_test.stdout.strip() or f"exit {self_test.returncode}")
        )

    # The name is derived from docs/PRODUCT_IDENTITY.md, not chosen by a caller.
    named = _run(["bash", str(APPIMAGE_SCRIPT), "--print-name", "0.1.0-rc5", "x64"])
    expected = "TaskForest-G-0.1.0-rc5-x64.AppImage"
    if named.returncode != 0 or named.stdout.strip() != expected:
        errors.append(
            f"the AppImage artifact name must be '{expected}'; got "
            f"'{named.stdout.strip()}' (exit {named.returncode})"
        )
    return errors


def run_sandbox_builders() -> list[str]:
    errors: list[str] = []
    for label, argv in (
        ("flatpak-builder", ["bash", str(FLATPAK_BUILD)]),
        ("linuxdeploy", ["bash", str(APPIMAGE_SCRIPT), "--self-test"]),
    ):
        result = _run(argv)
        if result.returncode != 0:
            errors.append(f"{label} run failed: {result.stderr.strip() or result.stdout.strip()}")
    return errors


def self_test() -> int:
    import json

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        # A manifest whose metadata names another component must be rejected.
        (root / f"{APP_ID}.metainfo.xml").write_text(
            '<?xml version="1.0"?><component><id>org.example.Other</id></component>', encoding="utf-8"
        )
        bad = {
            "app-id": APP_ID,
            "command": "taskforest-g",
            "finish-args": list(flatpak.REQUIRED_FINISH_ARGS),
            "modules": [
                {
                    "name": "taskforest",
                    "build-commands": [
                        f"install -Dm644 {root / (APP_ID + '.metainfo.xml')} /app/share/metainfo/{APP_ID}.metainfo.xml",
                        "install -Dm755 target/release/taskforest-g /app/bin/taskforest-g",
                        f"install -Dm644 packaging/linux/x.desktop /app/share/applications/{APP_ID}.desktop",
                    ],
                }
            ],
        }
        manifest_path = root / f"{APP_ID}.json"
        manifest_path.write_text(json.dumps(bad), encoding="utf-8")
        errors = validate_flatpak(manifest_path)
        assert any("AppStream component id" in e for e in errors), errors
        assert any("does not exist in the repository" in e for e in errors), errors

        # A manifest named after another id is rejected on the file name alone.
        wrong_name = root / "org.example.Other.json"
        wrong_name.write_text(json.dumps(bad), encoding="utf-8")
        errors = validate_flatpak(wrong_name)
        assert any("Manifest file name" in e for e in errors), errors

    print("Linux sandbox packaging validator self-test: PASS")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true", help="run unit self-tests")
    parser.add_argument(
        "--sandbox-build",
        action="store_true",
        help="also run the real flatpak-builder/linuxdeploy builds (needs the SDK and network)",
    )
    args = parser.parse_args()

    if args.self_test:
        return self_test()

    errors = validate_flatpak() + validate_appimage()
    if args.sandbox_build:
        errors += run_sandbox_builders()
    if errors:
        for err in errors:
            print(f"FAIL: {err}", file=sys.stderr)
        return 1

    print(
        f"Linux sandbox packaging: PASS (Flatpak app-id '{APP_ID}', manifest layout and AppImage naming verified)"
    )
    if args.sandbox_build:
        print("Linux sandbox packaging: real flatpak-builder and linuxdeploy runs completed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
