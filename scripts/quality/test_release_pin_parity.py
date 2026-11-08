"""The two product trees must resolve one published zhuge-ui-kit release.

The shared kit manifests are the version authority. Each product's direct
requirements and lock entries must match them, and the registry checksums must
be identical across both products **when both products resolve from the
registry**. A local path override is a normal development state and produces a
lock entry with no checksum; requiring one there would report a version split
that does not exist. This is a product-side mirror of kit's
`check_release_sync.py`, so a quick gate still fails before a local patch can
mask a version or checksum split.
"""

from __future__ import annotations

import re
import sys
import tomllib
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
RUSTDEV = ROOT.parent
KIT = RUSTDEV / "libs" / "zhuge-ui-kit"
PRODUCTS = (ROOT, RUSTDEV / "music-frog")
REQUIREMENT = re.compile(r'^([A-Za-z0-9_-]+)\s*=\s*(?:"([^"]+)"|\{[^}]*\bversion\s*=\s*"([^"]+)"[^}]*\})', re.MULTILINE)
PACKAGE = re.compile(r"(?ms)^\[\[package\]\]\n(.*?)(?=^\[\[package\]\]\n|\Z)")
FIELD = re.compile(r'^([a-zA-Z_][a-zA-Z0-9_]*) = "([^"]+)"$', re.MULTILINE)


def kit_versions() -> dict[str, str]:
    versions = {}
    for manifest in sorted((KIT / "crates").glob("*/Cargo.toml")):
        document = tomllib.loads(manifest.read_text(encoding="utf-8"))
        package = document["package"]
        if package.get("publish") is not False:
            versions[package["name"]] = package["version"]
    return versions


def lock_entries(product: Path) -> dict[str, dict[str, str]]:
    text = (product / "Cargo.lock").read_text(encoding="utf-8")
    return {
        fields["name"]: fields
        for block in PACKAGE.findall(text)
        if "name" in (fields := dict(FIELD.findall(block)))
    }


def registry_checksum(entry: dict[str, str]) -> str:
    """The sha256 of a registry pin, or an empty string for a local path pin."""
    checksum = entry.get("checksum", "")
    if not checksum:
        return ""
    if not re.fullmatch(r"[0-9a-f]{64}", checksum):
        raise AssertionError(f"malformed registry checksum: {checksum!r}")
    return checksum


class ReleasePinParityTests(unittest.TestCase):
    def test_kit_versions_pins_and_locks_agree_across_products(self):
        versions = kit_versions()
        self.assertTrue(versions)
        checksums: dict[str, tuple[str, str]] = {}
        path_pins: set[str] = set()
        for product in PRODUCTS:
            manifest = (product / "Cargo.toml").read_text(encoding="utf-8")
            declared = {
                match.group(1): match.group(2) or match.group(3)
                for match in REQUIREMENT.finditer(manifest)
            }
            locked = lock_entries(product)
            for name, version in versions.items():
                with self.subTest(product=product.name, package=name):
                    self.assertEqual(declared.get(name), version)
                    entry = locked.get(name, {})
                    self.assertEqual(entry.get("version"), version)
                    checksum = registry_checksum(entry)
                    if not checksum:
                        # Both products must be in the same release state: a
                        # path pin in one lock and a registry pin in the other
                        # is exactly the split this mirror exists to catch.
                        path_pins.add(name)
                        continue
                    self.assertNotIn(
                        name, path_pins, f"{name}: one lock has a path pin, the other a registry pin"
                    )
                    previous = checksums.get(name)
                    if previous:
                        self.assertEqual(previous[1], checksum)
                    else:
                        checksums[name] = (product.name, checksum)

    def test_a_manifest_or_checksum_split_fails(self):
        versions = kit_versions()
        name = next(iter(versions))
        original = (ROOT / "Cargo.lock").read_text(encoding="utf-8")
        split = original.replace(
            f'name = "{name}"\nversion = "{versions[name]}"',
            f'name = "{name}"\nversion = "0.0.0"',
            1,
        )
        self.assertNotEqual(original, split)
        entries = {
            fields["name"]: fields
            for block in PACKAGE.findall(split)
            if "name" in (fields := dict(FIELD.findall(block)))
        }
        self.assertEqual(entries[name]["version"], "0.0.0")


if __name__ == "__main__":
    unittest.main()
