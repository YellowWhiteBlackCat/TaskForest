#!/usr/bin/env python3
"""Fail-closed privacy and publication guard for the public repository."""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
import tempfile
from pathlib import Path


SECRET_PATTERNS = (
    re.compile(rb"-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----"),
    re.compile(rb"\bgh[pousr]_[A-Za-z0-9_]{20,}\b"),
    re.compile(rb"\bgithub_pat_[A-Za-z0-9_]{20,}\b"),
    re.compile(rb"\bAKIA[0-9A-Z]{16}\b"),
    re.compile(rb"\bxox[baprs]-[A-Za-z0-9-]{20,}\b"),
    re.compile(rb"\bBearer\s+[A-Za-z0-9._-]{20,}\b"),
)
PRIVATE_PATH_PATTERN = re.compile(
    rb"(?<![A-Za-z0-9_])(?:/run/media/(?!<user>)[A-Za-z0-9._-]+|"
    rb"/(?:home|Users)/(?!<user>)[A-Za-z0-9._-]+|"
    rb"/mnt/c/Users/(?!<user>)[A-Za-z0-9._-]+|"
    rb"[A-Za-z]:[\\/](?:Users|users)[\\/](?!<user>)[A-Za-z0-9._-]+)"
)
FORBIDDEN_PREFIXES = (
    ".private/",
    "docs/archive/",
    "docs/screenshots/",
)
FORBIDDEN_PUBLIC_FILES = {
    "docs/BEVY_UI_FRONTEND.md",
    "docs/BEVY_UPSTREAM_WATCH.md",
    "docs/TODO.md",
    "docs/ROADMAP_NEXT.md",
    "docs/CHANGELOG.md",
    "docs/CORE_100_CLOSURE.md",
    "docs/GPUI_INTERACTION_ACCEPTANCE.md",
    "docs/GPUI_UPSTREAM_WATCH.md",
    "docs/MISSION_CENTER_SCORE.md",
    "docs/MULTI_FRONTEND_FUNCTIONAL_GAP.md",
    "docs/BONUS_SCORE.md",
    "docs/SESSION_SURFACES.md",
    "docs/WSL_UPSTREAM_WATCH.md",
    "packaging/linux/io.github.YellowWhiteBlackCat.TaskMochi.desktop",
    "packaging/linux/io.github.YellowWhiteBlackCat.TaskMochi.metainfo.xml",
    "docs/mission-center-score.tsv",
    "docs/mission-center-parity.tsv",
    "docs/multi-frontend-functional-score.tsv",
    "docs/bonus-score.tsv",
    "docs/system-install-host-receipt.tsv",
    "docs/quality/design-debt.md",
    "docs/quality/bench-trend.tsv",
    "docs/quality/bloat-trend.tsv",
    "docs/quality/module-doc-report.md",
    "docs/quality/rust-line-report.md",
    "scripts/mission_center_score.py",
    "scripts/multi_frontend_functional_score.py",
    "scripts/bonus_score.py",
    "scripts/dev-install-desktop.sh",
    "tests/logic/mission_center_score_test.rs",
    "tests/logic/mission_center_parity_ledger_test.rs",
}


def git_lines(root: Path, *arguments: str) -> list[str]:
    result = subprocess.run(
        ["git", "-C", str(root), *arguments],
        check=True,
        capture_output=True,
        timeout=30,
    )
    return result.stdout.decode("utf-8", errors="replace").splitlines()


def current_tracked_files(root: Path) -> list[Path]:
    """Return files tracked by the index that still exist in the worktree."""

    return [root / item for item in git_lines(root, "ls-files") if (root / item).is_file()]


def relative(path: Path, root: Path) -> str:
    return path.relative_to(root).as_posix()


def path_violations(root: Path, files: list[Path]) -> list[str]:
    findings: list[str] = []
    for path in files:
        name = relative(path, root)
        if name == "docs/screenshots/README.md":
            continue
        if name in FORBIDDEN_PUBLIC_FILES or any(name.startswith(prefix) for prefix in FORBIDDEN_PREFIXES):
            findings.append(f"private publication path is tracked: {name}")
    return findings


def content_violations(root: Path, files: list[Path]) -> list[str]:
    findings: list[str] = []
    for path in files:
        name = relative(path, root)
        data = path.read_bytes()
        if b"\0" in data:
            continue
        for pattern in SECRET_PATTERNS:
            if pattern.search(data):
                findings.append(f"credential-like pattern in tracked file: {name}")
                break
        if PRIVATE_PATH_PATTERN.search(data):
            findings.append(f"host-specific path in tracked file: {name}")
        try:
            data.decode("utf-8")
        except UnicodeDecodeError:
            findings.append(f"non-UTF-8 text must be reviewed before publication: {name}")
    return findings


def history_violations(root: Path) -> list[str]:
    findings: list[str] = []
    for item in git_lines(root, "rev-list", "--objects", "--all"):
        _, _, name = item.partition(" ")
        normalized = name.replace("\\", "/")
        if normalized == "docs/screenshots/README.md":
            continue
        if normalized in FORBIDDEN_PUBLIC_FILES or any(
            normalized.startswith(prefix) for prefix in FORBIDDEN_PREFIXES
        ):
            findings.append(f"private path remains in Git history: {normalized}")
    return findings


def validate(root: Path, include_history: bool) -> list[str]:
    files = current_tracked_files(root)
    findings = path_violations(root, files) + content_violations(root, files)
    if include_history:
        findings.extend(history_violations(root))
    return findings


def self_test() -> None:
    assert SECRET_PATTERNS[0].search(b"-----BEGIN " + b"PRIVATE KEY-----")
    assert PRIVATE_PATH_PATTERN.search(b"/run/" + b"media/person/disk/project")
    assert PRIVATE_PATH_PATTERN.search(b"/" + b"Users/person/project")
    assert PRIVATE_PATH_PATTERN.search(b"C:" + b"/User" + b"s/person/project")
    with tempfile.TemporaryDirectory(prefix="public-repo-guard-") as directory:
        root = Path(directory)
        path = root / "sample.txt"
        path.write_text("/run/" + "media/person/disk/project\n", encoding="utf-8")
        assert content_violations(root, [path])
    print("public-repo-guard self-test: PASS")


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, default=Path.cwd())
    parser.add_argument("--history", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)
    if args.self_test:
        self_test()
        return 0
    findings = validate(args.repo_root.resolve(), args.history)
    for finding in findings:
        print(f"public repository violation: {finding}")
    mode = " + history" if args.history else ""
    print(f"public-repo-guard{mode}: {'FAIL' if findings else 'PASS'} ({len(findings)} finding(s))")
    return 1 if findings else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
