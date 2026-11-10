#!/usr/bin/env python3
# source-inspection: static-policy
"""ADR-051: conditional compilation is a platform-axis mechanism only.

No workspace manifest may declare a frontend-selecting ``ui-*`` feature, and
no production Rust source may gate code on one. The check walks the two owned
workspaces; ignored build trees, private material and generated caches are
outside the census.
"""

from __future__ import annotations

import re
import sys
import tempfile
import unittest
from pathlib import Path

RUSTDEV = Path(__file__).resolve().parents[3]
REPOS = (
    RUSTDEV / "taskmanager",
    RUSTDEV / "libs" / "zhuge-ui-kit",
)
IGNORED = {".git", ".private", "target", "node_modules", "dist", "build"}
UI_TOKEN = r"ui[-_](?:gpui|iced|bevy|tui|slint|android)"
MANIFEST_HITS = (
    re.compile(rf"(?i)^\s*{UI_TOKEN}[A-Za-z0-9_-]*\s*=\s*\[", re.MULTILINE),
    re.compile(rf'(?i)feature\s*=\s*"[^"]*{UI_TOKEN}[^"]*"', re.MULTILINE),
)
RUST_HIT = re.compile(rf"(?i)#\[cfg\([^]]*(?:feature\s*=\s*\"[^\"]*{UI_TOKEN}[^\"]*\"|\b{UI_TOKEN}\b)[^]]*\)\]")


def relevant(path: Path, root: Path) -> bool:
    relative = path.relative_to(root)
    return not any(part in IGNORED for part in relative.parts)


def findings(root: Path) -> list[str]:
    found: list[str] = []
    for path in sorted(root.rglob("*.rs")):
        if not relevant(path, root) or "/tests/" in path.as_posix():
            continue
        for number, line in enumerate(path.read_text(encoding="utf-8", errors="replace").splitlines(), 1):
            if RUST_HIT.search(line):
                found.append(f"{path.relative_to(root)}:{number}: frontend cfg")
    for path in sorted(root.rglob("Cargo.toml")):
        if not relevant(path, root):
            continue
        text = path.read_text(encoding="utf-8", errors="replace")
        if any(pattern.search(text) for pattern in MANIFEST_HITS):
            found.append(f"{path.relative_to(root)}: frontend feature")
    return sorted(found)


class PlatformAxisOnlyTests(unittest.TestCase):
    def test_owned_workspaces_have_no_frontend_features_or_cfgs(self):
        for root in REPOS:
            with self.subTest(repo=root.name):
                self.assertEqual(findings(root), [])

    def test_planted_frontend_feature_and_cfg_fail(self):
        with tempfile.TemporaryDirectory(prefix="uik-platform-axis-") as raw:
            root = Path(raw)
            (root / "Cargo.toml").write_text(
                '[features]\nui-gpui = []\n', encoding="utf-8"
            )
            (root / "src").mkdir()
            (root / "src" / "lib.rs").write_text(
                '#[cfg(feature = "ui-gpui")]\npub fn gpu() {}\n',
                encoding="utf-8",
            )
            hits = findings(root)
            self.assertEqual(len(hits), 2)
            self.assertTrue(any("frontend feature" in hit for hit in hits))
            self.assertTrue(any("frontend cfg" in hit for hit in hits))


if __name__ == "__main__":
    unittest.main()
