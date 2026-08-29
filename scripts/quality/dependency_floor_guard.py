#!/usr/bin/env python3
"""Fail when the lockfile contains a known vulnerable dependency release."""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path


MINIMUMS: dict[str, tuple[int, int, int]] = {
    "lru": (0, 18, 2),
}
PACKAGE_BLOCK = re.compile(r"(?ms)^\[\[package\]\]\n(.*?)(?=^\[\[package\]\]\n|\Z)")
FIELD = re.compile(r'^([a-zA-Z_][a-zA-Z0-9_]*) = "([^"]+)"$', re.MULTILINE)
SEMVER = re.compile(r"^(\d+)\.(\d+)\.(\d+)(?:[-+].*)?$")


def package_versions(lock_text: str) -> list[tuple[str, str]]:
    packages: list[tuple[str, str]] = []
    for block in PACKAGE_BLOCK.findall(lock_text):
        fields = dict(FIELD.findall(block))
        name = fields.get("name")
        version = fields.get("version")
        if name is not None and version is not None:
            packages.append((name, version))
    return packages


def version_tuple(version: str) -> tuple[int, int, int]:
    match = SEMVER.fullmatch(version)
    if match is None:
        raise ValueError(f"unsupported semver in Cargo.lock: {version}")
    return tuple(int(part) for part in match.groups())


def violations(lock_text: str) -> list[str]:
    findings: list[str] = []
    for name, version in package_versions(lock_text):
        minimum = MINIMUMS.get(name)
        if minimum is not None and version_tuple(version) < minimum:
            minimum_text = ".".join(str(part) for part in minimum)
            findings.append(f"{name} {version} is below the fixed floor {minimum_text}")
    return findings


def self_test() -> None:
    old = '[[package]]\nname = "lru"\nversion = "0.16.4"\n'
    fixed = '[[package]]\nname = "lru"\nversion = "0.18.2"\n'
    absent = '[[package]]\nname = "other"\nversion = "0.1.0"\n'
    assert violations(old) == ["lru 0.16.4 is below the fixed floor 0.18.2"]
    assert violations(fixed) == []
    assert violations(absent) == []


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("dependency floor guard self-test: PASS")
        return 0

    lockfile = args.repo_root.resolve() / "Cargo.lock"
    findings = violations(lockfile.read_text(encoding="utf-8"))
    for finding in findings:
        print(f"dependency-floor: {finding}", file=sys.stderr)
    if findings:
        return 1
    print("dependency-floor: PASS (lru >= 0.18.2 or absent)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
