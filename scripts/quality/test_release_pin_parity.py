#!/usr/bin/env python3
"""The product resolves each shared-kit crate from one published release.

The shared kit is consumed from crates.io: the manifest version, the lock
version, the lock source and the sha256 recorded by Cargo must all describe
the same published registry artifact. A local ``[patch.crates-io]`` or a Git
rev lock is a host-only state and must never be committed as release evidence.
"""

from __future__ import annotations

import re
import sys
import tomllib
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
KIT_PACKAGES = (
    "zhuge-ui-core",
    "zhuge-ui-tokens",
    "zhuge-ui-kit-iced",
    "zhuge-ui-kit-bevy",
)
REGISTRY_SOURCE = "registry+https://github.com/rust-lang/crates.io-index"
PACKAGE = re.compile(r"(?ms)^\[\[package\]\]\n(.*?)(?=^\[\[package\]\]\n|\Z)")
FIELD = re.compile(r'^([a-zA-Z_][a-zA-Z0-9_]*) = "([^"]+)"$', re.MULTILINE)


def declared_versions(root: Path) -> dict[str, str]:
    document = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))
    dependencies = document["workspace"]["dependencies"]
    versions: dict[str, str] = {}
    for name in KIT_PACKAGES:
        value = dependencies[name]
        assert isinstance(value, str), f"{name}: expected a crates.io version requirement"
        assert re.fullmatch(r"\d+\.\d+\.\d+", value), f"{name}: pin an exact release version"
        versions[name] = value
    return versions


def lock_entries(root: Path) -> dict[str, dict[str, str]]:
    entries: dict[str, dict[str, str]] = {}
    for block in PACKAGE.findall((root / "Cargo.lock").read_text(encoding="utf-8")):
        fields = dict(FIELD.findall(block.split("\ndependencies = [", 1)[0]))
        if "name" in fields:
            entries[fields["name"]] = fields
    return entries


def audit(root: Path = ROOT) -> list[str]:
    declared = declared_versions(root)
    locked = lock_entries(root)
    findings: list[str] = []
    for name, version in declared.items():
        entry = locked.get(name, {})
        if entry.get("version") != version:
            findings.append(f"{name}: lock version {entry.get('version')!r} != {version!r}")
        if entry.get("source") != REGISTRY_SOURCE:
            findings.append(f"{name}: lock source {entry.get('source')!r} is not crates.io")
        checksum = entry.get("checksum", "")
        if not re.fullmatch(r"[0-9a-f]{64}", checksum):
            findings.append(f"{name}: lock has no registry checksum")
    return findings


class ReleasePinParityTests(unittest.TestCase):
    def test_the_committed_tree_pins_one_registry_release_per_crate(self):
        self.assertEqual(audit(), [])

    def test_a_git_or_patch_lock_is_rejected(self):
        original = (ROOT / "Cargo.lock").read_text(encoding="utf-8")
        name = KIT_PACKAGES[0]
        version = declared_versions(ROOT)[name]
        mutated = original.replace(
            f'name = "{name}"\nversion = "{version}"\nsource = "{REGISTRY_SOURCE}"',
            f'name = "{name}"\nversion = "{version}"',
            1,
        )
        self.assertNotEqual(original, mutated)
        entry = next(
            fields
            for block in PACKAGE.findall(mutated)
            if (fields := dict(FIELD.findall(block.split("\ndependencies = [", 1)[0]))).get("name") == name
        )
        self.assertNotEqual(entry.get("source"), REGISTRY_SOURCE)


def main() -> int:
    if "--self-test" in sys.argv[1:]:
        suite = unittest.defaultTestLoader.loadTestsFromTestCase(ReleasePinParityTests)
        result = unittest.TextTestRunner(verbosity=2).run(suite)
        return 0 if result.wasSuccessful() else 1
    findings = audit()
    for finding in findings:
        print(f"VIOLATION: {finding}")
    if findings:
        return 1
    print("shared-kit registry pin parity: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
