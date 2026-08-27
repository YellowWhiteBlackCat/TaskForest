#!/usr/bin/env python3
"""Guard Rust import and public-surface conventions without file allowlists.

The workspace has two structural rules:

* imports and public facades use the source symbol directly; ``use ... as ...``
  (including ``as _`` and renamed ``pub use`` items) is not allowed.
* production state and function contracts do not encode surface presence with
  ``*_open: bool`` / ``*_was_open: bool``. They use a typed surface or an
  exhaustive presence transition instead.

Established aggregate modules may use a wildcard to forward their one owned
surface (for example ``taskmanager-core::core``). The guard therefore checks
the ambiguity-causing alias, not a hand-maintained list of facade files or
symbols. Explicit and wildcard re-exports remain visible to the compiler and
are owned by the module boundary that declares them.

The guard discovers every Rust file recursively below its roots. It does not
carry a per-file exception list. ``--self-test`` exercises both positive and
negative cases, including comments containing Rust-looking text.
"""

from __future__ import annotations

import argparse
import re
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path


USE_ITEM = re.compile(
    r"(?ms)^\s*(?:(?:pub)(?:\s*\([^)]*\))?\s+)?use\b.*?;"
)
IMPORT_ALIAS = re.compile(r"\bas\s+(?:_|[A-Za-z_][A-Za-z0-9_]*)\b")
PUBLIC_USE = re.compile(r"^\s*pub(?:\s*\([^)]*\))?\s+use\b")
VISIBILITY_BOOL = re.compile(
    r"(?m)^\s*(?:pub(?:\s*\([^)]*\))?\s+)?"
    r"[A-Za-z_][A-Za-z0-9_]*(?:_open|_was_open)\s*:\s*bool\b"
)
# `patches/` contains vendored upstream crates. It is a dependency source tree,
# not TaskForest's owned public surface; workspace crates that consume it are
# still scanned. The remaining exclusions are generated or tool-owned trees.
EXCLUDED_DIRS = frozenset({".git", ".tmp", "node_modules", "patches", "target"})


@dataclass(frozen=True)
class Violation:
    path: str
    line: int
    code: str
    detail: str


def mask_comments(text: str) -> str:
    """Replace Rust line/block comments with spaces while preserving lines."""

    chars = list(text)
    index = 0
    block_depth = 0
    while index < len(chars):
        pair = "".join(chars[index : index + 2])
        if block_depth:
            if pair == "/*":
                chars[index] = chars[index + 1] = " "
                block_depth += 1
                index += 2
            elif pair == "*/":
                chars[index] = chars[index + 1] = " "
                block_depth -= 1
                index += 2
            else:
                if chars[index] not in "\r\n":
                    chars[index] = " "
                index += 1
            continue
        if pair == "//":
            chars[index] = chars[index + 1] = " "
            index += 2
            while index < len(chars) and chars[index] not in "\r\n":
                chars[index] = " "
                index += 1
            continue
        if pair == "/*":
            chars[index] = chars[index + 1] = " "
            block_depth = 1
            index += 2
            continue
        index += 1
    return "".join(chars)


def rust_files(root: Path) -> list[Path]:
    if root.is_file():
        return [root] if root.suffix == ".rs" else []
    return sorted(
        path
        for path in root.rglob("*.rs")
        if path.is_file()
        and not any(
            part in EXCLUDED_DIRS for part in path.relative_to(root).parts[:-1]
        )
    )


def scan_file(path: Path, repository: Path) -> list[Violation]:
    original = path.read_text(encoding="utf-8")
    text = mask_comments(original)
    violations: list[Violation] = []
    for match in USE_ITEM.finditer(text):
        item = match.group(0)
        line = text.count("\n", 0, match.start()) + 1
        relative = path.relative_to(repository).as_posix()
        if IMPORT_ALIAS.search(item):
            public_reexport = PUBLIC_USE.match(item) is not None
            violations.append(
                Violation(
                    relative,
                    line,
                    "RUST-SURFACE-002" if public_reexport else "RUST-SURFACE-001",
                    (
                        "renamed public re-exports are forbidden; expose the original symbol"
                        if public_reexport
                        else "import aliases are forbidden; import the symbol by its real name"
                    ),
                )
            )
    relative_path = path.relative_to(repository)
    if "src" in relative_path.parts:
        for match in VISIBILITY_BOOL.finditer(text):
            line = text.count("\n", 0, match.start()) + 1
            violations.append(
                Violation(
                    relative_path.as_posix(),
                    line,
                    "RUST-SURFACE-003",
                    "surface presence bools are forbidden; use a typed surface or presence transition",
                )
            )
    return violations


def scan(repository: Path, roots: list[Path]) -> list[Violation]:
    violations: list[Violation] = []
    for root in roots:
        if not root.exists():
            continue
        for path in rust_files(root):
            violations.extend(scan_file(path, repository))
    return violations


def default_roots(repository: Path) -> list[Path]:
    roots = [repository / "src", repository / "tests", repository / "build.rs"]
    crates = repository / "crates"
    if crates.is_dir():
        for manifest in sorted(crates.glob("*/Cargo.toml")):
            package = manifest.parent
            roots.extend(
                (
                    package / "src",
                    package / "tests",
                    package / "examples",
                    package / "benches",
                    package / "build.rs",
                )
            )
    return roots


def self_test() -> int:
    with tempfile.TemporaryDirectory(prefix="rust-surface-guard-") as raw:
        repository = Path(raw)
        source = repository / "src"
        source.mkdir()
        cases = {
            "clean.rs": "use gpui::AppContext;\npub use crate::api::Button;\n",
            "alias.rs": "use gpui::AppContext as _;\n",
            "multiline_alias.rs": "use crate::{\n    AppContext,\n    Button as LegacyButton,\n};\n",
            "wildcard.rs": "pub use crate::primitives::*;\n",
            "public_alias.rs": "pub use crate::api::Button as LegacyButton;\n",
            "scoped_public_alias.rs": "pub(crate) use crate::api::Button as InternalButton;\n",
            "comment.rs": "/* use gpui::AppContext as _; */\n",
            "visibility_bool.rs": "struct DialogState {\n    dialog_open: bool,\n}\n",
            "visibility_comment.rs": "// struct Old { modal_open: bool }\n",
        }
        for name, content in cases.items():
            (source / name).write_text(content, encoding="utf-8")
        found = scan(repository, [source])
        codes = {(item.path, item.code) for item in found}
        expected = {
            ("src/alias.rs", "RUST-SURFACE-001"),
            ("src/multiline_alias.rs", "RUST-SURFACE-001"),
            ("src/public_alias.rs", "RUST-SURFACE-002"),
            ("src/scoped_public_alias.rs", "RUST-SURFACE-002"),
            ("src/visibility_bool.rs", "RUST-SURFACE-003"),
        }
        if codes != expected:
            print(f"self-test failed: expected {expected}, found {codes}", file=sys.stderr)
            return 1
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--mode", choices=("report", "enforce"), default="enforce")
    parser.add_argument("--root", action="append", dest="roots")
    args = parser.parse_args()
    if args.self_test:
        return self_test()

    repository = Path(__file__).resolve().parents[2]
    roots = (
        [(repository / raw).resolve() for raw in args.roots]
        if args.roots
        else default_roots(repository)
    )
    violations = scan(repository, roots)
    for item in violations:
        print(f"{item.path}:{item.line}: {item.code}: {item.detail}")
    if violations and args.mode == "enforce":
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
