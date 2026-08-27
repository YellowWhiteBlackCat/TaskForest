#!/usr/bin/env python3
"""Report or enforce module-level documentation (`//!`) on significant Rust files.

A Rust module of meaningful size should open with a `//!` doc comment stating its
purpose. This guard flags files that lack one. Candidates are every `lib.rs` /
`main.rs` crate root plus any source file whose non-comment line count reaches
``--min-lines``. Test trees and vendored patches are excluded.

Exit code: 1 in ``--mode enforce`` when any candidate lacks a header, else 0.
"""

from __future__ import annotations

import argparse
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

MIN_LINES = 60


@dataclass(frozen=True)
class FileCheck:
    path: str
    code_lines: int
    is_root: bool
    has_header: bool


def line_has_code(line: str, block_depth: int) -> tuple[bool, int]:
    """Reuse the comment-aware code detector from the line-budget guard.

    `//!` and `///` start with `//`, so they are treated as comments (not code),
    which lets the header scan distinguish a module doc from real code.
    """
    has_code = False
    index = 0
    while index < len(line):
        if block_depth:
            if line.startswith("/*", index):
                block_depth += 1
                index += 2
            elif line.startswith("*/", index):
                block_depth -= 1
                index += 2
            else:
                index += 1
            continue
        if line[index].isspace():
            index += 1
        elif line.startswith("//", index):
            break
        elif line.startswith("/*", index):
            block_depth += 1
            index += 2
        else:
            has_code = True
            index += 1
    return has_code, block_depth


def count_code_lines(path: Path) -> int:
    count = 0
    block_depth = 0
    with path.open("r", encoding="utf-8", errors="replace") as handle:
        for line in handle:
            has_code, block_depth = line_has_code(line, block_depth)
            count += int(has_code)
    return count


def has_module_header(path: Path) -> bool:
    """True when a `//!` line appears before the first code-bearing line.

    Blank lines, ordinary `//` comments, `///` item docs, and `#![..]`/`#[..]`
    attributes are skipped so a header placed after inner attributes still counts.
    """
    block_depth = 0
    with path.open("r", encoding="utf-8", errors="replace") as handle:
        for line in handle:
            stripped = line.lstrip()
            if block_depth:
                _, block_depth = line_has_code(line, block_depth)
                continue
            if stripped.startswith("//!"):
                return True
            # Skip blank lines, ordinary comments, and attributes so they do not
            # read as the first code line and prematurely end the header scan.
            if (
                not stripped
                or stripped.startswith("//")
                or stripped.startswith("#!")
                or stripped.startswith("#[")
            ):
                _, block_depth = line_has_code(line, block_depth)
                continue
            has_code, block_depth = line_has_code(line, block_depth)
            if has_code:
                return False
    return False


def is_test_file(path: Path) -> bool:
    """Inline test modules and files inside a `tests/` tree carry no module docs."""
    return (
        path.name == "tests.rs"
        or path.name.endswith("_tests.rs")
        or any(part == "tests" for part in path.parts)
    )


def iter_candidates(repository: Path, min_lines: int):
    roots = [repository / "src"]
    crates = repository / "crates"
    if crates.is_dir():
        for manifest in sorted(crates.glob("*/Cargo.toml")):
            roots.append(manifest.parent / "src")
    for root in roots:
        if not root.is_dir():
            continue
        for path in sorted(root.rglob("*.rs")):
            if is_test_file(path):
                continue
            is_root = path.name in ("lib.rs", "main.rs")
            code_lines = count_code_lines(path)
            if is_root or code_lines >= min_lines:
                yield path, is_root, code_lines


def collect(repository: Path, min_lines: int) -> list[FileCheck]:
    checks: list[FileCheck] = []
    for path, is_root, code_lines in iter_candidates(repository, min_lines):
        checks.append(
            FileCheck(
                path.relative_to(repository).as_posix(),
                code_lines,
                is_root,
                has_module_header(path),
            )
        )
    if not checks:
        raise RuntimeError("Module-doc guard scanned zero files")
    return checks


def report(checks: list[FileCheck], min_lines: int) -> str:
    missing = [c for c in checks if not c.has_header]
    lines = [
        "# Rust Module-Doc Header Report",
        "",
        f"- Scanned candidates: {len(checks)}",
        f"- Header required for: crate roots (lib.rs/main.rs) and files >= {min_lines} non-comment lines",
        f"- Missing headers: {len(missing)}",
        "",
        "## Missing module-level `//!` headers",
        "",
    ]
    lines.extend(
        f"- `{c.path}` ({'root' if c.is_root else f'{c.code_lines} code lines'})" for c in missing
    )
    if not missing:
        lines.append("_None._")
    lines.extend(
        (
            "",
            "## Policy",
            "",
            "Open each significant module with a `//!` purpose statement. State what the module "
            "owns and its key invariant in 1-3 lines; never generic filler. See "
            "`crates/taskmanager-telemetry-store/src/lib.rs` for the house style.",
        )
    )
    return "\n".join(lines) + "\n"


def self_test() -> None:
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        ok = root / "has.rs"
        ok.write_text("//! Purpose.\n//!\n//! Invariant.\n\npub fn a() {}\n", encoding="utf-8")
        if not has_module_header(ok):
            raise RuntimeError("header self-test (present) failed")
        attr = root / "attr.rs"
        attr.write_text(
            "#![forbid(unsafe_code)]\n#![deny(clippy::wildcard_imports)]\n\n//! Header after attrs.\n\npub fn b() {}\n",
            encoding="utf-8",
        )
        if not has_module_header(attr):
            raise RuntimeError("header self-test (after attributes) failed")
        none = root / "none.rs"
        none.write_text("pub mod x;\npub use x::*;\n", encoding="utf-8")
        if has_module_header(none):
            raise RuntimeError("header self-test (absent) failed")
        itemdoc = root / "item.rs"
        itemdoc.write_text("/// Item doc only.\npub struct S;\n", encoding="utf-8")
        if has_module_header(itemdoc):
            raise RuntimeError("header self-test (item doc mistaken for module doc) failed")
        # code-line counting matches the line guard: 1 code line, comment-aware.
        if count_code_lines(ok) != 1:
            raise RuntimeError("code-line count self-test failed")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mode", choices=("report", "enforce"), default="report")
    parser.add_argument("--repo-root", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--min-lines", type=int, default=MIN_LINES)
    parser.add_argument("--report-file", type=Path)
    parser.add_argument("--self-test", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        if args.self_test:
            self_test()
            print("Module-doc guard self-test: PASS")
            return 0
        repository = args.repo_root.resolve()
        checks = collect(repository, args.min_lines)
        content = report(checks, args.min_lines)
        if args.report_file:
            output = args.report_file
            if not output.is_absolute():
                output = repository / output
            output.parent.mkdir(parents=True, exist_ok=True)
            output.write_text(content, encoding="utf-8")
        missing = [c for c in checks if not c.has_header]
        print(
            f"Module-doc guard: candidates={len(checks)} missing={len(missing)} mode={args.mode}"
        )
        for item in missing:
            kind = "root" if item.is_root else f"{item.code_lines} code lines"
            print(f" - missing: {item.path} ({kind})")
        return 1 if args.mode == "enforce" and missing else 0
    except (OSError, RuntimeError) as error:
        print(f"Module-doc guard: FAIL: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
