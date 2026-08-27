#!/usr/bin/env python3
"""Default-deny guard for source-inspection tests (上位规则).

The repository standard forbids using production source text as a substitute
for software-semantic verification. Tests must not read production Rust source
to prove production behavior.

Source inspection is allowed only in three declared categories:

  - static-policy:        policies that live in the source text itself (crate
                          attributes, cfg/feature boundaries, dependency
                          direction, forbidden legacy tokens).
  - source-transformation: source transformation / code generation tests
                          (generators, formatters, compile_fail fixtures).
  - textual-artifact:     artifacts whose contract is the text itself (locale
                          catalogs, SVG assets, capture receipts).

Every test file that reads production Rust source MUST declare one of these
categories in its header (first 10 lines):

    //! source-inspection: static-policy

Files without a valid declaration are violations. Enforcement keys on the
declared category, not on any specific text API (contains/find/regex/...):
the banned behavior is using source-text presence to prove software
semantics, and no text API can launder it back into a valid test.

The guard also rejects test-scratch helpers that leak into production source
(``repo_temp_dir`` must never appear under ``src/``), so an environment or
sync error cannot silently pollute production with test code.

``--mode report`` lists violations without failing; ``--mode enforce`` turns
the list into a hard failure.
"""

from __future__ import annotations

import argparse
import re
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

SOURCE_READ = re.compile(r"read_to_string|include_str!")
SOURCE_WALKERS = ("rust_sources(", "ecs_sources(", "core_metric_sources(")
RUST_MARKER = re.compile(r'\.rs["\']|"rs"')
DECLARATION = re.compile(r"^//!?\s*source-inspection:\s*([a-z-]+)\s*$")
ALLOWED_CATEGORIES = frozenset(
    {"static-policy", "source-transformation", "textual-artifact"}
)
HEADER_SCAN_LINES = 10


@dataclass(frozen=True)
class Violation:
    path: str
    line: int
    code: str


def reads_rust_source(text: str) -> bool:
    if any(walker in text for walker in SOURCE_WALKERS):
        return True
    if re.search(r"extension.{0,80}?\"rs\"", text, re.DOTALL):
        return True
    readers = list(
        re.finditer(r"read_to_string\(|\bread\(|\bsource\(|include_str!\(", text)
    )
    rust_paths = list(re.finditer(r'\.rs["\']', text))
    return any(
        abs(reader.start() - path.start()) <= 240
        for reader in readers
        for path in rust_paths
    )


def declaration_category(text: str) -> str | None:
    for line in text.splitlines()[:HEADER_SCAN_LINES]:
        match = DECLARATION.match(line.strip())
        if match and match.group(1) in ALLOWED_CATEGORIES:
            return match.group(1)
    return None


def scan_file(path: Path, repository: Path) -> list[Violation]:
    text = path.read_text(encoding="utf-8")
    if not reads_rust_source(text):
        return []
    if declaration_category(text) is None:
        return [
            Violation(
                path=path.relative_to(repository).as_posix(),
                line=1,
                code=(
                    "production source inspection without declared category; "
                    "declare source-inspection: "
                    "static-policy|source-transformation|textual-artifact"
                ),
            )
        ]
    return []


def test_files(repository: Path) -> list[Path]:
    roots: list[Path] = [repository / "tests"]
    crates = repository / "crates"
    if crates.is_dir():
        roots.extend(
            package / "tests"
            for package in sorted(crates.iterdir())
            if (package / "Cargo.toml").is_file() and (package / "tests").is_dir()
        )
    files: list[Path] = []
    for root in roots:
        if root.is_dir():
            files.extend(sorted(root.rglob("*.rs")))
    return files


def production_roots(repository: Path) -> list[Path]:
    roots: list[Path] = [repository / "src"]
    crates = repository / "crates"
    if crates.is_dir():
        roots.extend(
            package / "src"
            for package in sorted(crates.iterdir())
            if (package / "Cargo.toml").is_file()
        )
    return roots


def scan_production(repository: Path) -> list[Violation]:
    violations: list[Violation] = []
    for root in production_roots(repository):
        if not root.is_dir():
            continue
        for path in sorted(root.rglob("*.rs")):
            for index, line in enumerate(
                path.read_text(encoding="utf-8", errors="replace").splitlines(),
                start=1,
            ):
                if "repo_temp_dir" in line:
                    violations.append(
                        Violation(
                            path=path.relative_to(repository).as_posix(),
                            line=index,
                            code="test scratch helper leaked into production source",
                        )
                    )
    return violations


def self_test() -> int:
    failures = 0
    with tempfile.TemporaryDirectory(prefix="source-inspection-gate-") as raw:
        repository = Path(raw)
        tests = repository / "tests" / "logic"
        tests.mkdir(parents=True)
        (repository / "src").mkdir()
        (repository / "src" / "lib.rs").write_text(
            "pub fn run() {}\n", encoding="utf-8"
        )
        cases = [
            (
                "undeclared.rs",
                'let source = fs::read_to_string("src/lib.rs").unwrap();\n'
                'assert!(source.contains("run"));\n',
                1,
            ),
            (
                "declared_static.rs",
                "//! source-inspection: static-policy\n"
                'let source = fs::read_to_string("src/lib.rs").unwrap();\n'
                'assert!(source.contains("run"));\n',
                0,
            ),
            (
                "declared_invalid.rs",
                "//! source-inspection: behavior\n"
                'let source = fs::read_to_string("src/lib.rs").unwrap();\n'
                'assert!(source.contains("run"));\n',
                1,
            ),
            (
                "negative_guard.rs",
                "//! source-inspection: static-policy\n"
                'let source = fs::read_to_string("src/lib.rs").unwrap();\n'
                'assert!(!source.contains("run"));\n',
                0,
            ),
            (
                "non_rust_read.rs",
                'let text = fs::read_to_string("README.md").unwrap();\n'
                'assert!(text.contains("task"));\n',
                0,
            ),
            (
                "behavior_only.rs",
                'let text = render();\n'
                'assert!(text.contains("CPU"));\n',
                0,
            ),
            (
                "helper_read_undeclared.rs",
                'let backend = read("src/lib.rs");\n'
                'assert!(backend.contains("pub fn"));\n',
                1,
            ),
            (
                "walker_declared.rs",
                "//! source-inspection: source-transformation\n"
                'let code = rust_sources(&root.join("src"));\n'
                'assert!(code.contains("run"));\n',
                0,
            ),
        ]
        for name, content, expected in cases:
            path = tests / name
            path.write_text(content, encoding="utf-8")
            actual = len(scan_file(path, repository))
            if actual != expected:
                print(f"self-test FAIL {name}: expected {expected}, got {actual}")
                failures += 1
        (repository / "src" / "lib.rs").write_text(
            "pub(crate) fn repo_temp_dir() {}\n", encoding="utf-8"
        )
        leak_count = len(scan_production(repository))
        if leak_count != 1:
            print(f"self-test FAIL production-leak: expected 1, got {leak_count}")
            failures += 1
    return failures


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--mode", choices=("report", "enforce"), default="report"
    )
    parser.add_argument("--repo-root", type=Path, default=Path.cwd())
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)

    if args.self_test:
        return 1 if self_test() else 0

    repository = args.repo_root.resolve()
    inspected: list[Path] = []
    for path in test_files(repository):
        text = path.read_text(encoding="utf-8")
        if reads_rust_source(text):
            inspected.append(path)
    production_leaks = scan_production(repository)

    violations: list[Violation] = []
    declared = 0
    for path in inspected:
        text = path.read_text(encoding="utf-8")
        if declaration_category(text) is not None:
            declared += 1
        else:
            violations.extend(scan_file(path, repository))

    for violation in violations:
        print(
            f"{violation.path}:{violation.line}: "
            f"{violation.code}"
        )

    print(
        f"source-inspection files: {len(inspected)} "
        f"(declared={declared}, undeclared={len(violations)})"
    )
    for leak in production_leaks:
        print(f"{leak.path}:{leak.line}: production-leak: {leak.code}")
    if args.mode == "enforce" and (violations or production_leaks):
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
