#!/usr/bin/env python3
"""Enforce the repository's nextest and four-job test-runner policy.

The guard rejects real test invocations, not prose mentions. It inspects only
executable contexts: script-like files (shell, Makefile, justfile, PKGBUILD and
CI/TOML/YAML configuration) in full minus their comment lines, and Bash/Sh/
Console/Shell (or untagged) fenced code blocks inside Markdown and Rust
sources. Inline code spans, fences in other languages, prose, and comment lines
are mentions and are ignored.
"""

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
FENCE_MARKERS = ("```", "~~~")
SCRIPT_FENCE_LANGUAGES = {"", "bash", "sh", "console", "shell"}
CONFIG_COMMENT_PREFIXES = ("#", "//")
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


def numbered_lines(source: str) -> list[tuple[int, str]]:
    """Pair each source line with its 1-based number so findings keep real lines."""

    return list(enumerate(source.splitlines(), start=1))


def logical_lines(records: list[tuple[int, str]]) -> list[tuple[int, str]]:
    """Join shell/Rust-doc continuation lines while retaining the start line.

    Input records already carry the file's original line numbers; joining never
    renumbers a finding to the position it happens to occupy once comment lines
    have been removed.
    """

    joined: list[tuple[int, str]] = []
    pending: list[str] = []
    start = 1
    for number, line in records:
        if not pending:
            start = number
        pending.append(line)
        if not line.rstrip().endswith("\\"):
            joined.append((start, " ".join(pending)))
            pending = []
    if pending:
        joined.append((start, " ".join(pending)))
    return joined


def fence_language(stripped: str) -> tuple[str, str] | None:
    """Classify a fence line as ``(marker, language)`` or ``None``.

    A longer run of the marker character yields an empty language so it can be
    recognized as a closing or informational fence without a shell tag.
    """

    for marker in FENCE_MARKERS:
        if stripped.startswith(marker):
            rest = stripped[len(marker) :].strip()
            if not rest:
                return marker, ""
            token = rest.split()[0]
            if set(token) == {marker[0]}:
                return marker, ""
            return marker, token.lower()
    return None


def fence_closes(stripped: str, marker: str) -> bool:
    """Return whether ``stripped`` closes a fence opened with ``marker``."""

    if not stripped.startswith(marker):
        return False
    return set(stripped[len(marker) :].strip()) <= {marker[0]}


def strip_rust_comment(line: str) -> str:
    """Blank a leading Rust comment marker while keeping line numbers."""

    leading = line[: len(line) - len(line.lstrip())]
    stripped = line.lstrip()
    for marker in ("///", "//!", "//"):
        if stripped.startswith(marker):
            return f"{leading}{' ' * len(marker)}{stripped[len(marker) :]}"
    if stripped.startswith("* "):
        return f"{leading} {stripped[1:]}"
    return line


def markdown_code(source: str, line_view=None) -> str:
    """Keep only lines inside executable fenced code blocks.

    ``line_view`` optionally blanks a host-language comment marker (Rust) so
    fences and commands written in doc comments stay visible. Fences tagged
    with a language other than Bash/Sh/Console/Shell are treated as
    non-executable and masked in full.
    """

    view = line_view or (lambda line: line)
    output: list[str] = []
    fence_marker: str | None = None
    executable = False
    for line in source.splitlines():
        seen = view(line)
        stripped = seen.lstrip()
        if fence_marker is None:
            output.append("")
            fence = fence_language(stripped)
            if fence is not None:
                fence_marker, language = fence
                executable = language in SCRIPT_FENCE_LANGUAGES
            continue
        if fence_closes(stripped, fence_marker):
            fence_marker = None
            executable = False
            output.append("")
        elif executable and not stripped.startswith("#"):
            output.append(seen)
        else:
            output.append("")
    return "\n".join(output)


def command_source(path: str, source: str) -> list[tuple[int, str]]:
    """Return numbered command-bearing lines for a tracked file.

    Each record keeps the file's original 1-based line number, so a finding
    still points at the real line after comment lines are removed instead of
    being renumbered by the surviving text.
    """

    if path == GUARD_PATH:
        return []
    file_path = Path(path)
    if file_path.suffix not in COMMAND_SUFFIXES and file_path.name not in COMMAND_FILENAMES:
        return []
    if file_path.suffix == ".md":
        return numbered_lines(markdown_code(source))
    if file_path.suffix == ".rs":
        return numbered_lines(markdown_code(source, strip_rust_comment))
    return [
        (number, line)
        for number, line in numbered_lines(source)
        if not line.lstrip().startswith(CONFIG_COMMENT_PREFIXES)
    ]


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


def validate_text(
    path: str, source: str | list[tuple[int, str]]
) -> list[Finding]:
    records = numbered_lines(source) if isinstance(source, str) else source
    findings: list[Finding] = []
    for line_number, line in logical_lines(records):
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
    def scan(name: str, source: str) -> list[Finding]:
        return validate_text(name, command_source(name, source))

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

    # Prose and inline code spans are mentions, never invocations.
    assert not scan(
        "guide.md",
        "# Guide\n"
        "Bare cargo test is rejected; write `cargo test` never.\n"
        "Also `cargo nextest run --workspace` misses its job count.\n",
    )
    assert not scan("lib.rs", "//! Use `cargo test` never.\n")

    # A real bare cargo test inside a bash fence is still rejected.
    assert [item.code for item in scan("guide.md", "```bash\ncargo test -p package\n```\n")] == [
        "TEST001"
    ]

    # A comment line inside a bash fence is only a mention.
    assert not scan(
        "guide.md",
        "```bash\n# cargo test is forbidden\ncargo nextest run -j 4\n```\n",
    )

    # Non-executable fences and Rust doc-comment fences are handled by tag.
    assert not scan("guide.md", '```rust\nfn main() { let _ = "cargo test"; }\n```\n')
    assert [
        item.code
        for item in scan("lib.rs", "//! ```bash\n//! cargo test\n//! ```\n")
    ] == ["TEST001"]
    assert not scan("lib.rs", "//! ```bash\n//! # cargo test\n//! ```\n")

    # Doctest whitelist and the mandatory job count survive context filtering.
    assert not scan("guide.md", "```sh\ncargo test --locked --doc --workspace -j 4\n```\n")
    assert [item.code for item in scan("guide.md", "```sh\ncargo test --doc --workspace\n```\n")] == [
        "TEST002"
    ]
    assert [
        item.code
        for item in scan("guide.md", "```console\ncargo nextest run --workspace\n```\n")
    ] == ["TEST003"]

    # Script-like files are executable in full, minus their comment lines.
    assert not scan("run.sh", "#!/bin/sh\n# cargo test -p package\ncargo nextest run -j 4\n")
    assert [item.code for item in scan("run.sh", "cargo test -p package\n")] == ["TEST001"]
    assert not scan("policy.toml", "# cargo test -p package\n")
    assert [item.code for item in scan("policy.toml", "command = \"cargo test -p package\"\n")] == [
        "TEST001"
    ]

    # Findings keep the file's real line even when comment lines are removed.
    assert [
        (item.code, item.line)
        for item in scan(
            "run.sh",
            "#!/bin/sh\n"
            "# cargo test -p package\n"
            "# another mention\n"
            "cargo test -p package\n",
        )
    ] == [("TEST001", 4)]
    assert [
        (item.code, item.line)
        for item in scan("policy.toml", "# mention\ncommand = \"cargo nextest run\"\n")
    ] == [("TEST003", 2)]
    assert [
        (item.code, item.line)
        for item in scan("guide.md", "prose\n```bash\ncargo test -p package\n```\n")
    ] == [("TEST001", 3)]
    # A continuation reports the start line of the joined command.
    assert [
        (item.code, item.line)
        for item in scan(
            "run.sh",
            "# header\ncargo nextest run --workspace \\\n  --features test-support\n",
        )
    ] == [("TEST003", 2)]

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
