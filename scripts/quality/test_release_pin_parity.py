#!/usr/bin/env python3
"""The two product trees must resolve one immutable shared-kit Git revision.

The private kit workspace is consumed by Git rev, not by a crates.io release
that can lag the features the products already use. All four declarations in
each product must name the same full commit, and both lockfiles must resolve
those packages from that exact Git source. A local ``[patch.crates-io]`` may
rewrite a host's lockfile into path or registry entries; committed locks must
never carry that host-only state. The negative fixture proves a version or
source split makes the comparison fail.
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
KIT_URL = "https://github.com/YellowWhiteBlackCat/zhuge-ui-kit.git"
REQUIREMENT = re.compile(
    r'^([A-Za-z0-9_-]+)\s*=\s*\{[^}]*\bgit\s*=\s*"([^"]+)"[^}]*\brev\s*=\s*"([0-9a-f]{40})"[^}]*\}',
    re.MULTILINE,
)
PACKAGE = re.compile(r"(?ms)^\[\[package\]\]\n(.*?)(?=^\[\[package\]\]\n|\Z)")
FIELD = re.compile(r'^([a-zA-Z_][a-zA-Z0-9_]*) = "([^"]+)"$', re.MULTILINE)


def kit_versions() -> dict[str, str]:
    versions = {}
    crates = KIT / "crates"
    if crates.is_dir():
        for manifest in sorted(crates.glob("*/Cargo.toml")):
            document = tomllib.loads(manifest.read_text(encoding="utf-8"))
            package = document["package"]
            if package.get("publish") is not False:
                versions[package["name"]] = package["version"]
    if versions:
        return versions
    # A clean CI clone has no sibling kit checkout. The committed lock still
    # names the four packages and their versions, so the single-product check
    # can verify the public repository state without pretending to know the
    # private workspace's current manifests.
    for name, fields in lock_entries(ROOT / "Cargo.lock").items():
        if name.startswith("zhuge-ui-") and fields.get("source", "").startswith("git+"):
            versions[name] = fields["version"]
    return versions


def declared_pins(manifest: Path) -> dict[str, tuple[str, str]]:
    return {
        match.group(1): (match.group(2), match.group(3))
        for match in REQUIREMENT.finditer(manifest.read_text(encoding="utf-8"))
    }


def lock_entries(lock: Path) -> dict[str, dict[str, str]]:
    text = lock.read_text(encoding="utf-8")
    entries: dict[str, dict[str, str]] = {}
    for block in PACKAGE.findall(text):
        header = block.split("\ndependencies = [", 1)[0]
        fields = dict(FIELD.findall(header))
        if "name" in fields:
            entries[fields["name"]] = fields
    return entries


def audit(products: tuple[Path, ...] = PRODUCTS) -> list[str]:
    versions = kit_versions()
    findings: list[str] = []
    revisions: set[str] = set()
    if not versions:
        return ["the shared kit manifests expose no publishable crate versions"]
    for product in products:
        declared = declared_pins(product / "Cargo.toml")
        locked = lock_entries(product / "Cargo.lock")
        for name, version in versions.items():
            pin = declared.get(name)
            if pin is None:
                findings.append(f"{product.name}: {name} has no Git rev declaration")
                continue
            source, revision = pin
            if source != KIT_URL:
                findings.append(f"{product.name}: {name} uses Git source {source!r}")
            revisions.add(revision)
            entry = locked.get(name, {})
            if entry.get("version") != version:
                findings.append(
                    f"{product.name}: {name} lock version {entry.get('version')!r} != {version!r}"
                )
            expected = f"git+{KIT_URL}?rev={revision}#{revision}"
            if entry.get("source") != expected:
                findings.append(
                    f"{product.name}: {name} lock source {entry.get('source')!r} != {expected!r}"
                )
    if len(revisions) != 1:
        findings.append(f"the two products must share one kit rev, found {sorted(revisions)}")
    return findings


class ReleasePinParityTests(unittest.TestCase):
    def test_both_products_pin_the_same_immutable_revision(self):
        self.assertEqual(audit(), [])

    def test_a_version_or_source_split_fails(self):
        versions = kit_versions()
        name = next(iter(versions))
        original = (ROOT / "Cargo.lock").read_text(encoding="utf-8")
        revision = declared_pins(ROOT / "Cargo.toml")[name][1]
        expected = f"git+{KIT_URL}?rev={revision}#{revision}"
        version_split = original.replace(
            f'name = "{name}"\nversion = "{versions[name]}"',
            f'name = "{name}"\nversion = "0.0.0"',
            1,
        )
        source_split = original.replace(f'source = "{expected}"', "", 1)
        self.assertNotEqual(original, version_split)
        self.assertNotEqual(original, source_split)
        for split in (version_split, source_split):
            entry = next(
                fields
                for block in PACKAGE.findall(split)
                if (fields := dict(FIELD.findall(block.split("\ndependencies = [", 1)[0]))).get("name") == name
            )
            self.assertTrue(
                entry.get("version") != versions[name] or entry.get("source") != expected
            )


def main() -> int:
    arguments = sys.argv[1:]
    if "--self-test" in arguments:
        suite = unittest.defaultTestLoader.loadTestsFromTestCase(ReleasePinParityTests)
        result = unittest.TextTestRunner(verbosity=2).run(suite)
        return 0 if result.wasSuccessful() else 1
    products = PRODUCTS if KIT.is_dir() and (RUSTDEV / "music-frog").is_dir() else PRODUCTS[:1]
    findings = audit(products=products)
    for finding in findings:
        print(f"VIOLATION: {finding}")
    if findings:
        return 1
    print("shared-kit Git pin parity: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
