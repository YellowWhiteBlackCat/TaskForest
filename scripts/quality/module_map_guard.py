#!/usr/bin/env python3
"""Validate that module names in crate README '## Module map' sections exist in src/."""

from __future__ import annotations

import argparse
import re
import sys
import tempfile
from pathlib import Path


# Rust language keywords and special module names that should never be
# reported as ghosts — they appear in module map prose or are implicit.
SKIP_NAMES: frozenset[str] = frozenset({
    "lib", "main", "mod", "pub", "crate", "use", "fn", "impl",
    "self", "super", "const", "static", "struct", "enum", "trait",
    "type", "where", "async", "await", "dyn", "extern", "let",
    "mut", "ref", "return", "match", "if", "else", "for", "in",
    "loop", "while", "break", "continue", "move", "unsafe", "true",
    "false", "as", "macro", "macro_rules", "cfg", "test", "tests",
    "bench", "build", "prelude",
    # Path prefixes that appear in module map notation but are not modules.
    "src",
})

# Patterns for extracting module references from module map text.
#
# RS_FILE: match bare identifiers ending with `.rs`.
RS_FILE = re.compile(r"\b([a-z_][a-z0-9_]*)\.rs\b")
#
# DIR_MODULE: match a directory module name — an identifier followed by `/`
# and then whitespace, `)`, or end-of-line.  This lookahead is critical:
# prose like `alert/SMART`, `config/theme`, `demo/capture`, `/dev/cpu/`
# has a word character after the slash, so it is never matched.  Real
# module-map directory entries always have whitespace after the trailing `/`.
DIR_MODULE = re.compile(r"\b([a-z_][a-z0-9_]*)/(?=\s|$|\))")
#
# Fenced code block content.
CODE_BLOCK = re.compile(r"```(?:text)?\n(.*?)```", re.DOTALL)
# Backtick-quoted inline content (for bullet-list format module maps).
BACKTICK_CONTENT = re.compile(r"`([^`]+)`")


def _extract_from_text(text: str, names: set[str]) -> None:
    """Apply RS_FILE and DIR_MODULE to *text*, adding results to *names*."""
    for match in RS_FILE.finditer(text):
        name = match.group(1)
        if name not in SKIP_NAMES:
            names.add(name)
    for match in DIR_MODULE.finditer(text):
        name = match.group(1)
        if name not in SKIP_NAMES:
            names.add(name)


def module_stems_in_src(src: Path) -> set[str]:
    """Collect every module stem present in the crate's src/ tree.

    A stem is the filename without `.rs` for files, or the directory name
    for directories.  Both forms represent a Rust module.
    """
    stems: set[str] = set()
    if not src.is_dir():
        return stems
    for item in src.rglob("*"):
        if item.is_file() and item.suffix == ".rs":
            stems.add(item.stem)
        elif item.is_dir():
            stems.add(item.name)
    return stems


def extract_module_map(readme: Path) -> str | None:
    """Return the text of the '## Module map' section, or None."""
    text = readme.read_text(encoding="utf-8")
    start = text.find("## Module map")
    if start < 0:
        return None
    rest = text[start + len("## Module map"):]
    end = rest.find("\n## ")
    if end >= 0:
        return rest[:end]
    return rest


def extract_module_names(section: str) -> set[str]:
    """Extract module names from the module map section text.

    Two formats are handled:

    1. Fenced code blocks (```text ... ```): each line is scanned for
       ``name.rs`` and ``name/`` patterns.  The DIR_MODULE regex requires
       whitespace (or ``)`` / end-of-line) after the ``/``, which eliminates
       prose patterns like ``alert/SMART`` and ``config/theme``.

    2. Bullet-list / prose format (e.g. bevy-ui): module references are
       extracted from backtick-quoted content only, so surrounding prose
       does not contribute false positives.
    """
    names: set[str] = set()

    # 1. Extract from fenced code blocks.
    for block_match in CODE_BLOCK.finditer(section):
        block = block_match.group(1)
        _extract_from_text(block, names)

    # 2. Extract from backtick-quoted content outside code blocks.
    outside = CODE_BLOCK.sub("", section)
    for bt_match in BACKTICK_CONTENT.finditer(outside):
        _extract_from_text(bt_match.group(1), names)

    return names


def ghost_violations(root: Path) -> list[str]:
    """Find documented-but-missing module names across all crates."""
    findings: list[str] = []
    crates_dir = root / "crates"
    if not crates_dir.is_dir():
        return findings
    for package in sorted(crates_dir.iterdir()):
        if not package.is_dir() or not package.name.startswith("taskmanager-"):
            continue
        readme = package / "README.md"
        if not readme.is_file():
            continue
        section = extract_module_map(readme)
        if section is None:
            continue
        src = package / "src"
        present = module_stems_in_src(src)
        documented = extract_module_names(section)
        relative_readme = readme.relative_to(root).as_posix()
        for name in sorted(documented - present):
            findings.append(
                f"{relative_readme}: ghost module '{name}' "
                f"(documented in Module map but not found in {src.relative_to(root).as_posix()}/)"
            )
    return findings


def self_test() -> None:
    """Minimal self-test covering code-block, backtick, and false-positive cases."""
    with tempfile.TemporaryDirectory(prefix="module-map-guard-") as directory:
        root = Path(directory)
        crate = root / "crates" / "taskmanager-demo"
        src = crate / "src"
        src.mkdir(parents=True)
        (src / "lib.rs").write_text("", encoding="utf-8")
        (src / "foo.rs").write_text("", encoding="utf-8")
        (src / "bar").mkdir()
        (src / "bar" / "baz.rs").write_text("", encoding="utf-8")

        # Code-block format.
        # - ghost_mod.rs and another_ghost/ are absent modules (ghosts).
        # - Description text mentions "alert/SMART", "config/theme",
        #   and "/dev/cpu/" — none of these should be extracted.
        readme = crate / "README.md"
        readme.write_text(
            "# demo\n\n"
            "## Role\n\nA demo crate.\n\n"
            "## Module map\n\n"
            "```text\n"
            "src/foo.rs              does foo\n"
            "├── bar/                does bar\n"
            "├── ghost_mod.rs        does not exist\n"
            "├── another_ghost/      also does not exist\n"
            "├── lib.rs              should be skipped (keyword)\n"
            "src/baz.rs              alert/SMART evidence window\n"
            "                        refresh scheduling and config/theme substitution\n"
            "                        pre-read of /dev/cpu/*/msr registers\n"
            "                        decision (shared/local/accepted-difference/unsupported)\n"
            "                        deterministic demo/capture/test seam\n"
            "```\n\n"
            "## Boundary\n",
            encoding="utf-8",
        )
        findings = ghost_violations(root)
        ghosts = {f.split("'")[1] for f in findings}
        assert "ghost_mod" in ghosts, f"expected ghost_mod in ghosts: {ghosts}"
        assert "another_ghost" in ghosts, f"expected another_ghost in ghosts: {ghosts}"
        assert "foo" not in ghosts, f"foo should not be a ghost: {ghosts}"
        assert "bar" not in ghosts, f"bar should not be a ghost: {ghosts}"
        assert "lib" not in ghosts, f"lib should be skipped: {ghosts}"
        assert "baz" not in ghosts, f"baz should not be a ghost: {ghosts}"
        # False positives from description prose must not appear.
        for fp in ("alert", "config", "dev", "cpu", "shared", "local",
                    "difference", "demo", "capture"):
            assert fp not in ghosts, f"'{fp}' from description should not be extracted: {ghosts}"
        assert len(ghosts) == 2, f"expected exactly 2 ghosts, got {len(ghosts)}: {ghosts}"

        # Backtick format: "confirm/" in prose must not be extracted.
        crate2 = root / "crates" / "taskmanager-bt"
        src2 = crate2 / "src"
        src2.mkdir(parents=True)
        (src2 / "lib.rs").write_text("", encoding="utf-8")
        (src2 / "real.rs").write_text("", encoding="utf-8")
        readme2 = crate2 / "README.md"
        readme2.write_text(
            "# bt\n\n"
            "## Role\n\nBacktick test.\n\n"
            "## Module map\n\n"
            "- `src/real.rs` -- typed confirm/dismiss paths\n"
            "- `src/ghost_bt.rs` -- absent module\n\n"
            "## Boundary\n",
            encoding="utf-8",
        )
        findings2 = ghost_violations(root)
        ghosts2 = {f.split("'")[1] for f in findings2 if "taskmanager-bt" in f}
        assert "ghost_bt" in ghosts2, f"expected ghost_bt in ghosts2: {ghosts2}"
        assert "real" not in ghosts2, f"real should not be a ghost: {ghosts2}"
        assert "confirm" not in ghosts2, f"confirm from prose should not be extracted: {ghosts2}"
        assert len(ghosts2) == 1, f"expected exactly 1 ghost for bt, got {len(ghosts2)}: {ghosts2}"

    print("module-map-guard self-test: ok")


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, default=Path.cwd())
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)
    if args.self_test:
        self_test()
        return 0
    findings = ghost_violations(args.repo_root.resolve())
    for finding in findings:
        print(f"module map violation: {finding}")
    print(f"module-map-guard: {'FAIL' if findings else 'PASS'} ({len(findings)} ghost(s))")
    return 1 if findings else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
