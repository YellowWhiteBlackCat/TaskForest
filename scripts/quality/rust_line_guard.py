#!/usr/bin/env python3
"""Report or enforce non-comment Rust file line budgets."""

from __future__ import annotations

import argparse
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

SOURCE_WARN = 650
SOURCE_LIMIT = 1200
TEST_LIMIT = 999


@dataclass(frozen=True)
class FileStat:
    path: str
    kind: str
    code_lines: int
    level: str


def line_has_code(line: str, block_depth: int) -> tuple[bool, int]:
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


def collect(repository: Path) -> list[FileStat]:
    stats: list[FileStat] = []
    roots = [(repository / "src", "source"), (repository / "tests", "test")]
    crates = repository / "crates"
    if crates.is_dir():
        for manifest in sorted(crates.glob("*/Cargo.toml")):
            package = manifest.parent
            roots.extend(((package / "src", "source"), (package / "tests", "test")))
    for root, kind in roots:
        if not root.is_dir():
            continue
        for path in sorted(root.rglob("*.rs")):
            lines = count_code_lines(path)
            limit = SOURCE_LIMIT if kind == "source" else TEST_LIMIT
            if lines > limit:
                level = "fail"
            elif kind == "source" and lines >= SOURCE_WARN:
                level = "warn"
            else:
                level = "ok"
            stats.append(FileStat(path.relative_to(repository).as_posix(), kind, lines, level))
    if not stats:
        raise RuntimeError("Rust line guard scanned zero files")
    return stats


def report(stats: list[FileStat]) -> str:
    warnings = sorted((item for item in stats if item.level == "warn"), key=lambda item: -item.code_lines)
    failures = sorted((item for item in stats if item.level == "fail"), key=lambda item: -item.code_lines)
    lines = [
        "# Rust File Line Budget Report",
        "",
        f"- Scanned files: {len(stats)}",
        f"- Source warning: {SOURCE_WARN} non-comment lines",
        f"- Source hard limit: {SOURCE_LIMIT} non-comment lines",
        f"- Test hard limit: {TEST_LIMIT} non-comment lines",
        f"- Warnings: {len(warnings)}",
        f"- Failures: {len(failures)}",
        "",
        "## Hard-limit findings",
        "",
    ]
    lines.extend(
        (f"- `{item.path}`: {item.code_lines} ({item.kind})" for item in failures),
    )
    if not failures:
        lines.append("_None._")
    lines.extend(("", "## Warning findings", ""))
    lines.extend(f"- `{item.path}`: {item.code_lines} ({item.kind})" for item in warnings)
    if not warnings:
        lines.append("_None._")
    lines.extend(
        (
            "",
            "## Policy",
            "",
            "Split by named responsibility before adding logic. Do not use line-number parts, " +
            "generic helpers, `include!`, or blanket re-exports to hide coupling.",
        )
    )
    return "\n".join(lines) + "\n"


def self_test() -> None:
    sample = "//! doc\n\nfn a() {} // code\n/* outer\n /* nested */\n*/\n// comment\nfn b() {}\n"
    with tempfile.TemporaryDirectory() as temporary:
        path = Path(temporary) / "sample.rs"
        path.write_text(sample, encoding="utf-8")
        if count_code_lines(path) != 2:
            raise RuntimeError("comment-aware line count self-test failed")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mode", choices=("report", "enforce"), default="report")
    parser.add_argument("--repo-root", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--report-file", type=Path)
    parser.add_argument("--self-test", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        if args.self_test:
            self_test()
            print("Rust line guard self-test: PASS")
            return 0
        repository = args.repo_root.resolve()
        stats = collect(repository)
        content = report(stats)
        if args.report_file:
            output = args.report_file
            if not output.is_absolute():
                output = repository / output
            output.parent.mkdir(parents=True, exist_ok=True)
            output.write_text(content, encoding="utf-8")
        failures = [item for item in stats if item.level == "fail"]
        warnings = [item for item in stats if item.level == "warn"]
        print(
            f"Rust line guard: scanned={len(stats)} warnings={len(warnings)} "
            f"failures={len(failures)} mode={args.mode}"
        )
        for item in sorted(failures + warnings, key=lambda value: (-value.code_lines, value.path)):
            print(f" - {item.level}: {item.path}: {item.code_lines}")
        return 1 if args.mode == "enforce" and failures else 0
    except (OSError, RuntimeError) as error:
        print(f"Rust line guard: FAIL: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
