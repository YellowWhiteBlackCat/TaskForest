#!/usr/bin/env python3
"""Validate the current public documentation surface."""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
import tempfile
from pathlib import Path
from urllib.parse import unquote


LIVING_DOC_MAX_LINES = 200
AGENTS_MAX_LINES = 80
MARKDOWN_LINK = re.compile(r"(?<!!)\[[^\]]*\]\(([^)]+)\)")
PRIVATE_ROUTE_MARKERS = (
    "docs/archive",
    "docs/todo.md",
    "docs/roadmap_next",
    "docs/mission-center-score",
    "docs/mission-center-parity",
    "docs/multi-frontend-functional-score",
    "docs/bonus-score",
    "docs/system-install-host-receipt",
)


def public_markdown_paths(root: Path) -> list[Path]:
    paths: set[Path] = set()
    for name in ("README.md", "AGENTS.md", "CLAUDE.md"):
        path = root / name
        if path.is_file():
            paths.add(path)
    for base in (root / "docs", root / "adr", root / "crates", root / "polkit"):
        if not base.is_dir():
            continue
        paths.update(path for path in base.rglob("*.md") if path.is_file())
    return sorted(paths)


def tracked_paths(root: Path) -> list[Path]:
    result = subprocess.run(
        ["git", "-C", str(root), "ls-files", "-z"],
        check=True,
        capture_output=True,
        timeout=30,
    )
    return [root / item for item in result.stdout.decode().split("\0") if item]


def local_destination(raw: str) -> str | None:
    destination = raw.strip()
    if destination.startswith("<"):
        end = destination.find(">")
        if end < 0:
            return None
        destination = destination[1:end]
    else:
        destination = destination.split(None, 1)[0]
    lowered = destination.lower()
    if not destination or lowered.startswith(
        ("#", "http://", "https://", "mailto:", "data:", "ftp:", "//")
    ):
        return None
    return unquote(destination.split("#", 1)[0]) or None


def link_violations(root: Path, paths: list[Path]) -> list[str]:
    findings: list[str] = []
    resolved_root = root.resolve()
    for path in paths:
        text = path.read_text(encoding="utf-8")
        for match in MARKDOWN_LINK.finditer(text):
            destination = local_destination(match.group(1))
            if destination is None:
                continue
            line = text.count("\n", 0, match.start()) + 1
            relative = path.relative_to(root).as_posix()
            normalized = destination.replace("\\", "/").lower()
            if any(marker in normalized for marker in PRIVATE_ROUTE_MARKERS):
                findings.append(f"{relative}:{line}: link enters private route: {destination}")
                continue
            target = (path.parent / destination).resolve()
            if target != resolved_root and resolved_root not in target.parents:
                findings.append(f"{relative}:{line}: local link escapes repository: {destination}")
            elif not target.exists():
                findings.append(f"{relative}:{line}: missing local link target: {destination}")
    return findings


def route_violations(root: Path) -> list[str]:
    index = root / "docs" / "README.md"
    if not index.is_file():
        return ["docs/README.md: missing public documentation index"]
    text = index.read_text(encoding="utf-8")
    linked = {
        local_destination(match.group(1)).replace("\\", "/")
        for match in MARKDOWN_LINK.finditer(text)
        if local_destination(match.group(1)) is not None
    }
    findings: list[str] = []
    for path in sorted((root / "docs").glob("*.md")):
        if path.name == "README.md":
            continue
        if path.name not in linked:
            findings.append(f"orphan current document: {path.relative_to(root).as_posix()}")
    screenshot_policy = Path("screenshots/README.md")
    if screenshot_policy.as_posix() not in linked:
        findings.append("docs/README.md: public screenshot policy is not routed")
    return findings


def crate_readme_violations(root: Path) -> list[str]:
    findings: list[str] = []
    crates = root / "crates"
    for package in sorted(crates.iterdir()):
        if not package.is_dir() or not (package / "Cargo.toml").is_file():
            continue
        readme = package / "README.md"
        relative = readme.relative_to(root).as_posix()
        if not readme.is_file():
            findings.append(f"{relative}: missing crate README")
            continue
        text = readme.read_text(encoding="utf-8")
        for heading in ("## Role", "## Boundary", "## Contract and verification"):
            if heading not in text:
                findings.append(f"{relative}: missing required section {heading!r}")
    return findings


def public_path_violations(root: Path) -> list[str]:
    forbidden_prefixes = (
        ".private/",
        "docs/archive/",
        "docs/screenshots/",
    )
    forbidden_exact = {
        "docs/TODO.md",
        "docs/ROADMAP_NEXT.md",
        "docs/CHANGELOG.md",
        "docs/CORE_100_CLOSURE.md",
        "docs/MISSION_CENTER_SCORE.md",
        "docs/MULTI_FRONTEND_FUNCTIONAL_GAP.md",
        "docs/BONUS_SCORE.md",
        "docs/system-install-host-receipt.tsv",
    }
    findings: list[str] = []
    for path in tracked_paths(root):
        relative = path.relative_to(root).as_posix()
        if relative == "docs/screenshots/README.md":
            continue
        if relative in forbidden_exact or any(relative.startswith(prefix) for prefix in forbidden_prefixes):
            findings.append(f"private path is tracked: {relative}")
    return findings


def content_violations(root: Path, paths: list[Path]) -> list[str]:
    findings: list[str] = []
    for path in paths:
        relative = path.relative_to(root).as_posix()
        text = path.read_text(encoding="utf-8")
        lowered = text.lower()
        for marker in PRIVATE_ROUTE_MARKERS:
            if marker in lowered and not (
                relative in {"AGENTS.md", "docs/README.md", "docs/QUALITY_GATES.md"}
                and marker == "docs/archive"
            ):
                findings.append(f"{relative}: private route text remains: {marker}")
        if relative == "README.md":
            if "实验阶段" not in text or "尚无稳定版本" not in text:
                findings.append("README.md: public experimental-status warning is incomplete")
            if re.search(r"\b\d{1,3}(?:\.\d+)?\s*/\s*100\b", text):
                findings.append("README.md: internal completion score must not be public")
    return findings


def validate(root: Path) -> list[str]:
    paths = public_markdown_paths(root)
    findings = public_path_violations(root)
    for path in paths:
        relative = path.relative_to(root).as_posix()
        line_count = len(path.read_text(encoding="utf-8").splitlines())
        if relative == "AGENTS.md":
            limit = AGENTS_MAX_LINES
        elif relative.startswith("docs/"):
            limit = LIVING_DOC_MAX_LINES
        else:
            continue
        if line_count > limit:
            findings.append(f"{relative}: {line_count} lines exceeds {limit}")
    findings.extend(link_violations(root, paths))
    findings.extend(route_violations(root))
    findings.extend(crate_readme_violations(root))
    findings.extend(content_violations(root, paths))
    return findings


def self_test() -> None:
    with tempfile.TemporaryDirectory(prefix="public-doc-guard-") as directory:
        root = Path(directory)
        (root / "docs").mkdir()
        (root / "crates" / "demo").mkdir(parents=True)
        (root / "README.md").write_text("实验阶段\n尚无稳定版本\n", encoding="utf-8")
        (root / "AGENTS.md").write_text("# A\n", encoding="utf-8")
        (root / "docs" / "README.md").write_text(
            "[x](x.md) [截图](screenshots/README.md)\n", encoding="utf-8"
        )
        (root / "docs" / "x.md").write_text("current\n", encoding="utf-8")
        (root / "docs" / "screenshots").mkdir()
        (root / "docs" / "screenshots" / "README.md").write_text("policy\n", encoding="utf-8")
        (root / "crates" / "demo" / "Cargo.toml").write_text("[package]\n", encoding="utf-8")
        (root / "crates" / "demo" / "README.md").write_text(
            "## Role\n## Boundary\n## Contract and verification\n", encoding="utf-8"
        )
        assert not link_violations(root, public_markdown_paths(root))
        assert not route_violations(root)
        (root / "docs" / "x.md").write_text("[old](../docs/archive/old.md)\n", encoding="utf-8")
        assert any(
            "private route" in item
            for item in link_violations(root, [root / "docs" / "x.md"])
        )
    print("doc-governance-guard self-test: ok")


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, default=Path.cwd())
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)
    if args.self_test:
        self_test()
        return 0
    findings = validate(args.repo_root.resolve())
    for finding in findings:
        print(f"doc governance violation: {finding}")
    print(f"doc-governance-guard: {'FAIL' if findings else 'PASS'} ({len(findings)} violation(s))")
    return 1 if findings else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
