#!/usr/bin/env python3
"""One immutable shared-kit revision, resolved the same way for every product.

The shared kit is a private Git workspace. Its crates.io versions can lag the
features a product already consumes, and a local ``[patch.crates-io]`` can
silently rewrite Cargo.lock into path entries that only this workstation can
resolve. Both states made CI fail while a local build stayed green. This gate
reads the committed manifests and locks, not Cargo's resolved metadata, so a
host patch cannot vouch for a public repository state.
"""

from __future__ import annotations

import argparse
import re
import sys
import tempfile
import tomllib
import unittest
from pathlib import Path

KIT_URL = "https://github.com/YellowWhiteBlackCat/zhuge-ui-kit.git"
KIT_PACKAGES = (
    "zhuge-ui-core",
    "zhuge-ui-tokens",
    "zhuge-ui-kit-iced",
    "zhuge-ui-kit-bevy",
)
PACKAGE_BLOCK = re.compile(r"(?ms)^\[\[package\]\]\n(.*?)(?=^\[\[package\]\]\n|\Z)")
FIELD = re.compile(r'^([a-zA-Z_][a-zA-Z0-9_]*) = "([^"]+)"$', re.MULTILINE)
REVISION = re.compile(r"\b[0-9a-f]{40}\b")


def git_revision(manifest: Path) -> str:
    document = tomllib.loads(manifest.read_text(encoding="utf-8"))
    dependencies = document["workspace"]["dependencies"]
    revisions: set[str] = set()
    for name in KIT_PACKAGES:
        entry = dependencies[name]
        assert isinstance(entry, dict), f"{name}: expected a Git dependency table"
        assert entry.get("git") == KIT_URL, f"{name}: wrong Git source"
        revision = entry.get("rev", "")
        assert REVISION.fullmatch(revision), f"{name}: rev must be a full 40-hex commit"
        assert entry.get("version"), f"{name}: version must accompany the Git rev"
        revisions.add(revision)
    assert len(revisions) == 1, "all shared-kit crates must share one Git rev"
    return revisions.pop()


def lock_entries(lock: Path) -> dict[str, dict[str, str]]:
    entries: dict[str, dict[str, str]] = {}
    for block in PACKAGE_BLOCK.findall(lock.read_text(encoding="utf-8")):
        header = block.split("\ndependencies = [", 1)[0]
        fields = dict(FIELD.findall(header))
        if "name" in fields:
            entries[fields["name"]] = fields
    return entries


def audit(manifest: Path, lock: Path) -> list[str]:
    revision = git_revision(manifest)
    entries = lock_entries(lock)
    findings: list[str] = []
    expected_source = f"git+{KIT_URL}?rev={revision}#{revision}"
    for name in KIT_PACKAGES:
        source = entries.get(name, {}).get("source")
        if source != expected_source:
            findings.append(
                f"{name}: lock source must be {expected_source}, got {source!r}"
            )
    return findings


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--self-test", action="store_true")
    arguments = parser.parse_args()
    if arguments.self_test:
        suite = unittest.defaultTestLoader.loadTestsFromTestCase(PinTests)
        result = unittest.TextTestRunner(verbosity=2).run(suite)
        return 0 if result.wasSuccessful() else 1
    findings = audit(arguments.repo_root / "Cargo.toml", arguments.repo_root / "Cargo.lock")
    for finding in findings:
        print(f"VIOLATION: {finding}")
    if findings:
        return 1
    revision = git_revision(arguments.repo_root / "Cargo.toml")
    print(f"shared-kit Git pin: PASS ({revision})")
    return 0


class PinTests(unittest.TestCase):
    def test_a_local_patch_lock_is_rejected(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            revision = "a" * 40
            lines = [
                "[workspace.dependencies]",
                *(
                    f'{name} = {{ git = "{KIT_URL}", rev = "{revision}", version = "{version}" }}'
                    for name, version in zip(
                        KIT_PACKAGES, ("0.1", "0.1", "0.2", "0.4")
                    )
                ),
            ]
            (root / "Cargo.toml").write_text("\n".join(lines), encoding="utf-8")
            (root / "Cargo.lock").write_text(
                '[[package]]\nname = "zhuge-ui-kit-bevy"\nversion = "0.4.1"\n',
                encoding="utf-8",
            )
            self.assertTrue(any("lock source" in item for item in audit(root / "Cargo.toml", root / "Cargo.lock")))

    def test_registry_pins_are_rejected(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            (root / "Cargo.toml").write_text(
                "[workspace.dependencies]\n"
                + "\n".join(f'{name} = "0.1.0"' for name in KIT_PACKAGES),
                encoding="utf-8",
            )
            (root / "Cargo.lock").write_text("", encoding="utf-8")
            with self.assertRaises(AssertionError):
                audit(root / "Cargo.toml", root / "Cargo.lock")

    def test_one_rev_and_matching_git_lock_passes(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            revision = "b" * 40
            (root / "Cargo.toml").write_text(
                "[workspace.dependencies]\n"
                + "\n".join(
                    f'{name} = {{ git = "{KIT_URL}", rev = "{revision}", version = "{version}" }}'
                    for name, version in zip(
                        KIT_PACKAGES, ("0.1", "0.1", "0.2", "0.4")
                    )
                ),
                encoding="utf-8",
            )
            source = f"git+{KIT_URL}?rev={revision}#{revision}"
            (root / "Cargo.lock").write_text(
                "\n".join(
                    f'[[package]]\nname = "{name}"\nversion = "1"\nsource = "{source}"'
                    for name in KIT_PACKAGES
                ),
                encoding="utf-8",
            )
            self.assertEqual(git_revision(root / "Cargo.toml"), revision)
            self.assertEqual(audit(root / "Cargo.toml", root / "Cargo.lock"), [])

    def test_mixed_revisions_are_rejected(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            (root / "Cargo.toml").write_text(
                "[workspace.dependencies]\n"
                + "\n".join(
                    f'{name} = {{ git = "{KIT_URL}", rev = "{letter * 40}", version = "0.1" }}'
                    for letter, name in zip("abcd", KIT_PACKAGES)
                ),
                encoding="utf-8",
            )
            (root / "Cargo.lock").write_text("", encoding="utf-8")
            with self.assertRaises(AssertionError):
                git_revision(root / "Cargo.toml")


if __name__ == "__main__":
    sys.exit(main())
