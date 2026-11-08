#!/usr/bin/env python3
"""Every test file must be reachable: a test that is never compiled proves nothing.

This workspace keeps its test bodies in per-crate `tests/` trees and mounts
them from production sources with `#[path = "..."] mod ...;`. A file that
nobody mounts is a **dead test**: it reads like evidence, it sits in the diff,
and it never runs. R95 shipped exactly that: the percent owner's four
behavior tests were written, wired by nobody, and therefore never executed by
`cargo nextest` — the owner could have answered "0%" for a missing reading
and the suite would have stayed green.

The rule this gate enforces:

* a file directly under a crate's `tests/` directory is a Cargo test target
  and is live by construction;
* every other `.rs` file under `tests/` must be reachable from a live file
  through `#[path = "..."]`, `mod <name>;` or `include!("...")`;
* an unreachable file is a violation, not a warning — there is no "kept for
  later" state for a test body.

Usage::

    python3 scripts/quality/test_wired_tests.py
    python3 scripts/quality/test_wired_tests.py --self-test
"""

from __future__ import annotations

import argparse
import re
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

# `#[path = "..."]` — the file the attribute points at.
PATH_ATTRIBUTE = re.compile(r"#\s*\[\s*path\s*=\s*\"([^\"]+)\"\s*\]")
# `mod name;` — the file Rust looks for next to the declaring module.
MODULE_DECLARATION = re.compile(
    r"^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;"
)
# `include!("...")` — a file spliced into the including module.
INCLUDE_MACRO = re.compile(r"include!\s*\(\s*\"([^\"]+)\"\s*\)")


def package_roots(repository: Path) -> list[Path]:
    """Every package root that owns a `tests/` tree."""
    roots: list[Path] = []
    if (repository / "Cargo.toml").is_file():
        roots.append(repository)
    crates = repository / "crates"
    if crates.is_dir():
        roots.extend(
            package
            for package in sorted(crates.iterdir())
            if (package / "Cargo.toml").is_file()
        )
    return roots


def manifest_targets(package: Path) -> list[Path]:
    """The files a `[[test]]` / `[[bench]]` / `[[bin]]` section points at.

    Cargo compiles a target whose path the manifest names explicitly, even
    when the file sits inside a subdirectory of `tests/` — the crate's own
    `perf_budget_alloc` target is one of those.
    """
    manifest = package / "Cargo.toml"
    if not manifest.is_file():
        return []
    targets: list[Path] = []
    section: str | None = None
    for raw in manifest.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if line.startswith("[["):
            section = line.strip("[]").strip()
            continue
        if line.startswith("["):
            section = None
            continue
        if section not in {"test", "bench", "bin"}:
            continue
        if not line.startswith("path"):
            continue
        _, _, value = line.partition("=")
        value = value.strip().strip('"')
        if value:
            targets.append((package / value).resolve())
    return targets


def declared_files(source: str, parent: Path) -> list[Path]:
    """The files one source file mounts, resolved against its own directory."""
    mounted: list[Path] = []
    pending_path: str | None = None
    for raw in source.splitlines():
        line = raw.strip()
        attribute = PATH_ATTRIBUTE.search(line)
        if attribute is not None:
            pending_path = attribute.group(1)
            mounted.append((parent / attribute.group(1)).resolve())
            continue
        declaration = MODULE_DECLARATION.match(line)
        if declaration is not None:
            name = declaration.group(1)
            if pending_path is None:
                for candidate in (parent / f"{name}.rs", parent / name / "mod.rs"):
                    if candidate.is_file():
                        mounted.append(candidate.resolve())
                        break
            pending_path = None
            continue
        if line.startswith("mod ") and "{" in line:
            pending_path = None
            continue
        for include in INCLUDE_MACRO.finditer(line):
            mounted.append((parent / include.group(1)).resolve())
        pending_path = None
    return mounted


def live_files(repository: Path) -> set[Path]:
    """Every file the compiler will actually read."""
    live: set[Path] = set()
    processed: set[Path] = set()
    queue: list[Path] = []
    for package in package_roots(repository):
        source = package / "src"
        if source.is_dir():
            queue.extend(path.resolve() for path in sorted(source.rglob("*.rs")))
        tests = package / "tests"
        if tests.is_dir():
            # A file directly under `tests/` is a Cargo test target.
            for path in sorted(tests.glob("*.rs")):
                live.add(path.resolve())
                queue.append(path.resolve())
        for target in manifest_targets(package):
            live.add(target)
            queue.append(target)
    index = 0
    while index < len(queue):
        current = queue[index]
        index += 1
        if current in processed or not current.is_file():
            continue
        processed.add(current)
        live.add(current)
        try:
            source = current.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            continue
        for mounted in declared_files(source, current.parent):
            if mounted not in live:
                queue.append(mounted)
    return live


def orphan_test_files(repository: Path) -> list[str]:
    """Test files no compiler will read."""
    live = live_files(repository)
    orphans: list[str] = []
    for package in package_roots(repository):
        tests = package / "tests"
        if not tests.is_dir():
            continue
        for path in sorted(tests.rglob("*.rs")):
            if path.resolve() in live:
                continue
            orphans.append(path.relative_to(repository).as_posix())
    return orphans


class WiredTestsTests(unittest.TestCase):
    def test_every_test_file_is_mounted_by_some_compiled_file(self):
        orphans = orphan_test_files(ROOT)
        self.assertEqual(
            orphans,
            [],
            "these test files are never compiled:\n" + "\n".join(orphans),
        )

    def test_the_gate_bites_on_an_injected_orphan(self):
        with tempfile.TemporaryDirectory(prefix="wired-tests-") as raw:
            repository = Path(raw)
            package = repository / "crates" / "demo"
            (package / "src").mkdir(parents=True)
            (package / "tests" / "headless").mkdir(parents=True)
            (package / "Cargo.toml").write_text(
                '[package]\nname="demo"\n', encoding="utf-8"
            )
            (package / "src" / "lib.rs").write_text(
                '#[cfg(test)]\n#[path = "../tests/headless/live.rs"]\nmod live;\n',
                encoding="utf-8",
            )
            (package / "tests" / "headless" / "live.rs").write_text(
                "#[test]\nfn live() {}\n", encoding="utf-8"
            )
            (package / "tests" / "headless" / "orphan.rs").write_text(
                "#[test]\nfn orphan() {}\n", encoding="utf-8"
            )
            self.assertEqual(
                orphan_test_files(repository),
                ["crates/demo/tests/headless/orphan.rs"],
                "an unmounted test file must be reported",
            )

    def test_a_mounted_submodule_is_not_an_orphan(self):
        with tempfile.TemporaryDirectory(prefix="wired-tests-") as raw:
            repository = Path(raw)
            package = repository / "crates" / "demo"
            (package / "src").mkdir(parents=True)
            (package / "tests" / "headless" / "nested").mkdir(parents=True)
            (package / "Cargo.toml").write_text(
                '[package]\nname="demo"\n', encoding="utf-8"
            )
            (package / "src" / "lib.rs").write_text(
                '#[cfg(test)]\n#[path = "../tests/headless/entry.rs"]\nmod entry;\n',
                encoding="utf-8",
            )
            (package / "tests" / "headless" / "entry.rs").write_text(
                '#[path = "nested/child.rs"]\nmod child;\n', encoding="utf-8"
            )
            (package / "tests" / "headless" / "nested" / "child.rs").write_text(
                "#[test]\nfn child() {}\n", encoding="utf-8"
            )
            self.assertEqual(
                orphan_test_files(repository),
                [],
                "a nested module mounted by a live file is live",
            )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    arguments = parser.parse_args()
    if arguments.self_test:
        suite = unittest.defaultTestLoader.loadTestsFromTestCase(WiredTestsTests)
        result = unittest.TextTestRunner(verbosity=2).run(suite)
        return 0 if result.wasSuccessful() else 1
    orphans = orphan_test_files(ROOT)
    for orphan in orphans:
        print(f"  FAIL unmounted test file: {orphan}")
    if orphans:
        print(
            f"wired-tests gate: FAIL {len(orphans)} test file(s) are never "
            "compiled; mount them with #[path = ...] mod ...; or delete them"
        )
        return 1
    print("wired-tests gate: PASS every test file is mounted by a compiled file")
    return 0


if __name__ == "__main__":
    sys.exit(main())
