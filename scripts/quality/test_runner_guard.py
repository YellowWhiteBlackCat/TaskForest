#!/usr/bin/env python3
"""Enforce the repository's nextest and four-job test-runner policy."""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path


GUARD_PATH = "scripts/quality/test_runner_guard.py"
COMMAND_SUFFIXES = {".bash", ".md", ".rs", ".sh", ".toml", ".yaml", ".yml"}
COMMAND_FILENAMES = {"Makefile", "PKGBUILD", "justfile"}
CARGO_GLOBAL = (
    r"(?:\s+(?:\+\S+|--locked|--frozen|--offline|--quiet|--verbose|"
    r"--color(?:=|\s+)\S+|--config(?:=|\s+)\S+|-Z\s+\S+))*"
)
DIRECT_TEST = re.compile(r"\bcargo" + CARGO_GLOBAL + r"\s+(?:llvm-cov\s+)?test\b")
DIRECT_NEXTEST = re.compile(r"\bcargo" + CARGO_GLOBAL + r"\s+nextest\s+run\b")
COVERAGE_NEXTEST = re.compile(r"\bcargo" + CARGO_GLOBAL + r"\s+llvm-cov\s+nextest\b")
MUTANTS = re.compile(r"\bcargo" + CARGO_GLOBAL + r"\s+mutants\b")
JOBS_FOUR = re.compile(
    r"(?<![\w-])(?:-j|--jobs|--test-threads)(?:\s*=?\s*)4(?!\d)"
)


@dataclass(frozen=True, order=True)
class Finding:
    path: str
    line: int
    code: str
    message: str


def repository_files(root: Path) -> list[Path]:
    result = subprocess.run(
        [
            "git",
            "-C",
            str(root),
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ],
        check=True,
        capture_output=True,
        timeout=30,
    )
    return [
        root / item
        for item in result.stdout.decode("utf-8", errors="replace").split("\0")
        if item and item != GUARD_PATH and (root / item).is_file()
    ]


def logical_lines(source: str) -> list[tuple[int, str]]:
    """Join shell/Rust-doc continuation lines while retaining the start line."""

    records: list[tuple[int, str]] = []
    pending: list[str] = []
    start = 1
    for number, line in enumerate(source.splitlines(), start=1):
        if not pending:
            start = number
        pending.append(line)
        if not line.rstrip().endswith("\\"):
            records.append((start, " ".join(pending)))
            pending = []
    if pending:
        records.append((start, " ".join(pending)))
    return records


def mask_outside_backticks(source: str) -> str:
    """Keep Rust/doc inline code and preserve original line numbers."""

    masked: list[str] = []
    in_code = False
    for character in source:
        if character == "`":
            in_code = not in_code
            masked.append(character)
        elif character == "\n":
            masked.append(character)
        else:
            masked.append(character if in_code else " ")
    return "".join(masked)


def markdown_code(source: str) -> str:
    """Keep fenced and inline Markdown code while preserving line numbers."""

    output: list[str] = []
    in_fence = False
    for line in source.splitlines(keepends=True):
        stripped = line.lstrip()
        if stripped.startswith("```") or stripped.startswith("~~~"):
            in_fence = not in_fence
            output.append("\n" if line.endswith("\n") else "")
        elif in_fence:
            output.append(line)
        else:
            output.append(mask_outside_backticks(line))
    return "".join(output)


def command_source(path: str, source: str) -> str:
    """Return only command-bearing text for a tracked file."""

    if path == GUARD_PATH:
        return ""
    file_path = Path(path)
    if file_path.suffix not in COMMAND_SUFFIXES and file_path.name not in COMMAND_FILENAMES:
        return ""
    if file_path.suffix == ".md":
        return markdown_code(source)
    if file_path.suffix == ".rs":
        return mask_outside_backticks(source)
    return "\n".join(
        line for line in source.splitlines() if not line.lstrip().startswith(("#", "//"))
    )


def command_segments(line: str) -> list[tuple[re.Match[str], str]]:
    matches = sorted(
        (
            match
            for pattern in (DIRECT_TEST, DIRECT_NEXTEST, COVERAGE_NEXTEST, MUTANTS)
            for match in pattern.finditer(line)
        ),
        key=lambda match: match.start(),
    )
    segments: list[tuple[re.Match[str], str]] = []
    for index, match in enumerate(matches):
        end = matches[index + 1].start() if index + 1 < len(matches) else len(line)
        segments.append((match, line[match.end() : end]))
    return segments


def validate_text(path: str, source: str) -> list[Finding]:
    findings: list[Finding] = []
    for line_number, line in logical_lines(source):
        for match, tail in command_segments(line):
            command = match.group(0)
            if DIRECT_TEST.fullmatch(command):
                if "--doc" not in tail:
                    findings.append(
                        Finding(
                            path,
                            line_number,
                            "TEST001",
                            "non-doctest cargo test is forbidden; use cargo nextest run",
                        )
                    )
                elif not JOBS_FOUR.search(tail):
                    findings.append(
                        Finding(
                            path,
                            line_number,
                            "TEST002",
                            "doctest cargo test must pass an explicit -j 4",
                        )
                    )
            elif DIRECT_NEXTEST.fullmatch(command) or COVERAGE_NEXTEST.fullmatch(command):
                if not JOBS_FOUR.search(tail):
                    findings.append(
                        Finding(
                            path,
                            line_number,
                            "TEST003",
                            "nextest test execution must pass an explicit -j 4",
                        )
                    )
            elif MUTANTS.fullmatch(command) and "--test-tool nextest" in tail:
                if not JOBS_FOUR.search(tail):
                    findings.append(
                        Finding(
                            path,
                            line_number,
                            "TEST004",
                            "cargo mutants nextest execution must pass an explicit -j 4",
                        )
                    )
    return findings


def validate(root: Path) -> list[Finding]:
    findings: list[Finding] = []
    for path in repository_files(root):
        data = path.read_bytes()
        if b"\0" in data:
            continue
        try:
            source = data.decode("utf-8")
        except UnicodeDecodeError:
            continue
        relative = path.relative_to(root).as_posix()
        findings.extend(validate_text(relative, command_source(relative, source)))
    return sorted(findings)


def self_test() -> None:
    compliant = """
cargo nextest run --locked --workspace -j 4
cargo llvm-cov nextest --workspace --profile ci --test-threads=4
cargo mutants --test-tool nextest -j 4
cargo test --locked --doc --workspace -j 4
"""
    assert not validate_text("sample", compliant)

    findings = validate_text(
        "sample",
        "\n".join(
            (
                "cargo test -p package",
                "cargo --locked test -p package",
                "cargo nextest run --workspace",
                "cargo llvm-cov nextest --workspace",
                "cargo mutants --test-tool nextest",
                "cargo test --doc --workspace",
            )
        ),
    )
    assert [item.code for item in findings] == [
        "TEST001",
        "TEST001",
        "TEST003",
        "TEST003",
        "TEST004",
        "TEST002",
    ]
    assert not validate_text("sample", "cargo nextest list --workspace")
    assert not validate_text(
        "sample",
        "cargo nextest run --workspace \\\n+--features test-support -j 4",
    )
    print("test-runner-guard self-test: PASS")


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, default=Path.cwd())
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)
    if args.self_test:
        self_test()
        return 0
    try:
        findings = validate(args.repo_root.resolve())
    except (OSError, subprocess.SubprocessError) as error:
        print(f"test-runner-guard: FAIL: {error}", file=sys.stderr)
        return 1
    for finding in findings:
        print(f"{finding.path}:{finding.line}: {finding.code}: {finding.message}")
    print(f"test-runner-guard: {'FAIL' if findings else 'PASS'} ({len(findings)} finding(s))")
    return int(bool(findings))


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
