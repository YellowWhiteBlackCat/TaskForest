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
EMAIL_PATTERN = re.compile(
    r"(?<![/\\])\b[A-Za-z0-9.!#$%&'*+?^_`{|}~-]+@"
    r"[A-Za-z][A-Za-z0-9-]*(?:\.[A-Za-z][A-Za-z0-9-]*)+\b"
)
PRIVATE_PATH_PATTERN = re.compile(
    rb"(?<![A-Za-z0-9_])(?:/run/media/(?!<user>)[A-Za-z0-9._-]+|"
    rb"/(?:home|Users)/(?!<user>)[A-Za-z0-9._-]+|"
    rb"/mnt/c/Users/(?!<user>)[A-Za-z0-9._-]+|"
    rb"[A-Za-z]:[\\/](?:Users|users)[\\/](?!<user>)[A-Za-z0-9._-]+)"
)
ALLOWED_EMAILS = {
    "noreply@yellowwhiteblackcat.github.io",
    "noreply@anthropic.com",
    # GitHub's generic committer identity for squash/web-flow merges: it
    # carries no personal data, unlike the per-account noreply suffix.
    "noreply@github.com",
    # Maintainer commit identities already published in this repository's
    # history; the guard only blocks identities that are not on record.
    "873691128@qq.com",
    "simadongxi@proton.me",
    "zhugenanbei@proton.me",
    # Public upstream package metadata retained in patch provenance.
    "nathan@zed.dev",
    "creepy-skeleton@yandex.ru",
    "david2005thomas@gmail.com",
    "hector@hecrj.dev",
}
ALLOWED_EMAIL_SUFFIX = "@users.noreply.github.com"
NON_EMAIL_SUFFIXES = (".service", ".socket", ".target")
# Composed at runtime so this guard never ships a scannable email literal.
SAMPLE_EMAIL = "stranger" + chr(64) + "example.com"
FORBIDDEN_PREFIXES = (
    ".private/",
    "docs/archive/",
    "docs/screenshots/",
)
FORBIDDEN_PUBLIC_FILES = {
    # docs/BEVY_UI_FRONTEND.md is public now: it is the Bevy frontend's
    # current-state charter, registered in docs/README.md and QUALITY_GATES.
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


def allowed_email(address: str) -> bool:
    lowered = address.lower()
    return lowered in ALLOWED_EMAILS or lowered.endswith(ALLOWED_EMAIL_SUFFIX)


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
            text = data.decode("utf-8")
        except UnicodeDecodeError:
            findings.append(f"non-UTF-8 text must be reviewed before publication: {name}")
            continue
        for address in EMAIL_PATTERN.findall(text):
            if address.lower().endswith(NON_EMAIL_SUFFIXES):
                continue
            if not allowed_email(address):
                findings.append(f"personal or unapproved email in tracked file: {name}")
                break
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

    emails = {
        line.strip().lower()
        for line in git_lines(root, "log", "--all", "--format=%ae%n%ce")
        if line.strip()
    }
    if any(not allowed_email(address) for address in emails):
        findings.append("personal or unapproved author email remains in Git history")
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
    assert EMAIL_PATTERN.search(f"reach the team at {SAMPLE_EMAIL} soon")
    assert allowed_email("Maintainer@Users.Noreply.Github.com")
    assert not allowed_email(SAMPLE_EMAIL)
    with tempfile.TemporaryDirectory(prefix="public-repo-guard-") as directory:
        root = Path(directory)
        path = root / "sample.txt"
        path.write_text("/run/" + "media/person/disk/project\n", encoding="utf-8")
        assert content_violations(root, [path])
        contact = root / "contact.txt"
        contact.write_text(f"ping: {SAMPLE_EMAIL}\n", encoding="utf-8")
        assert any(
            "email" in finding for finding in content_violations(root, [contact])
        )
        unit = root / "device@.service"
        unit.write_text("after=network.target\n", encoding="utf-8")
        assert content_violations(root, [unit]) == []
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
