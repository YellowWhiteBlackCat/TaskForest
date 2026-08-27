#!/usr/bin/env python3
"""Enforce the Bevy frontend's 100% ``bsn!`` UI-authoring contract.

The production Bevy crate has one route for declaring UI hierarchy: a Bevy
Scene built by ``bsn!``. This guard deliberately does not reject ECS state
binding. It rejects the older or parallel hierarchy routes instead:

* UI primitives such as ``Node { ... }``, ``Children [ ... ]`` and ``Text(...)``
  outside a ``bsn!`` scene;
* ``with_children``/manual child-link APIs and legacy UI bundles;
* direct ``commands.spawn``/``world.spawn`` calls, except the camera and
  observer infrastructure that is not a UI tree.

It scans only ``crates/taskmanager-bevy-ui/src`` by default. Tests and Bevy's
own plugin internals are outside the production authoring contract. The
scanner masks comments and literals before checking balanced macro spans, so
examples in documentation cannot satisfy or trip the rule accidentally.
"""

from __future__ import annotations

import argparse
import re
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path


BSN_START = re.compile(r"\bbsn!\s*\{")
UI_CONSTRUCTION = re.compile(
    r"\b(?:Node|Children|Text)\s*(?:\{|\[|\()"
    r"|\b(?:NodeBundle|TextBundle|ButtonBundle|ImageBundle|ChildBuilder|"
    r"ChildSpawnerCommands)\b"
)
MANUAL_CHILD_API = re.compile(
    r"\.\s*(?:with_children|push_children|add_child|add_children)\s*\("
)
DIRECT_SPAWN = re.compile(
    r"\b(?:commands|world|world_mut)\s*\.\s*spawn(?:_batch)?\s*\("
)
ALLOWED_SPAWN_PREFIXES = ("Observer::new", "Camera2d")


@dataclass(frozen=True)
class Violation:
    path: str
    line: int
    code: str
    detail: str


def _blank(chars: list[str], start: int, end: int) -> None:
    """Replace a non-code range with spaces while retaining line endings."""

    for index in range(start, min(end, len(chars))):
        if chars[index] not in "\r\n":
            chars[index] = " "


def mask_rust(text: str) -> str:
    """Mask comments and literals without changing offsets or line numbers."""

    chars = list(text)
    length = len(text)
    index = 0
    block_depth = 0
    while index < length:
        pair = text[index : index + 2]
        if block_depth:
            if pair == "/*":
                _blank(chars, index, index + 2)
                block_depth += 1
                index += 2
            elif pair == "*/":
                _blank(chars, index, index + 2)
                block_depth -= 1
                index += 2
            else:
                _blank(chars, index, index + 1)
                index += 1
            continue

        if pair == "//":
            end = text.find("\n", index)
            _blank(chars, index, length if end < 0 else end)
            index = length if end < 0 else end
            continue
        if pair == "/*":
            _blank(chars, index, index + 2)
            block_depth = 1
            index += 2
            continue

        raw = re.match(r"(?:br|r)(#+)?\"", text[index:])
        if raw:
            hashes = raw.group(1) or ""
            content_start = index + len(raw.group(0))
            terminator = f'"{hashes}'
            end = text.find(terminator, content_start)
            end = length if end < 0 else end + len(terminator)
            _blank(chars, index, end)
            index = end
            continue

        if text[index] == '"':
            end = index + 1
            escaped = False
            while end < length:
                current = text[end]
                if current == "\n" and not escaped:
                    break
                if current == '"' and not escaped:
                    end += 1
                    break
                if current == "\\" and not escaped:
                    escaped = True
                else:
                    escaped = False
                end += 1
            _blank(chars, index, end)
            index = end
            continue

        # A Rust character literal can contain braces. A lifetime (`'name`)
        # has no closing quote and is intentionally left as code.
        if text[index] == "'":
            end = index + 1
            escaped = False
            while end < length and text[end] not in "\r\n":
                current = text[end]
                if current == "'" and not escaped:
                    end += 1
                    _blank(chars, index, end)
                    index = end
                    break
                if current == "\\" and not escaped:
                    escaped = True
                else:
                    escaped = False
                end += 1
            else:
                index += 1
            continue

        index += 1
    return "".join(chars)


def line_number(text: str, offset: int) -> int:
    return text.count("\n", 0, offset) + 1


def scene_spans(masked: str) -> tuple[list[tuple[int, int]], list[int]]:
    """Return balanced ``bsn! { ... }`` spans and unbalanced start offsets."""

    spans: list[tuple[int, int]] = []
    unbalanced: list[int] = []
    for match in BSN_START.finditer(masked):
        opening = masked.find("{", match.start(), match.end())
        depth = 0
        closing = None
        for index in range(opening, len(masked)):
            if masked[index] == "{":
                depth += 1
            elif masked[index] == "}":
                depth -= 1
                if depth == 0:
                    closing = index + 1
                    break
        if closing is None:
            unbalanced.append(match.start())
        else:
            spans.append((match.start(), closing))
    return spans, unbalanced


def inside_scene(offset: int, spans: list[tuple[int, int]]) -> bool:
    return any(start <= offset < end for start, end in spans)


def matching_paren(masked: str, opening: int) -> int | None:
    depth = 0
    for index in range(opening, len(masked)):
        if masked[index] == "(":
            depth += 1
        elif masked[index] == ")":
            depth -= 1
            if depth == 0:
                return index
    return None


def rust_files(root: Path) -> list[Path]:
    if root.is_file():
        return [root] if root.suffix == ".rs" else []
    return sorted(path for path in root.rglob("*.rs") if path.is_file())


def scan_file(path: Path, repository: Path) -> list[Violation]:
    original = path.read_text(encoding="utf-8")
    masked = mask_rust(original)
    spans, unbalanced = scene_spans(masked)
    relative = path.relative_to(repository).as_posix()
    violations: list[Violation] = []

    for offset in unbalanced:
        violations.append(
            Violation(
                relative,
                line_number(original, offset),
                "BEVY-BSN-004",
                "bsn! scene has unbalanced braces",
            )
        )

    for match in UI_CONSTRUCTION.finditer(masked):
        if not inside_scene(match.start(), spans):
            violations.append(
                Violation(
                    relative,
                    line_number(original, match.start()),
                    "BEVY-BSN-001",
                    "UI hierarchy construction must be inside a bsn! Scene",
                )
            )

    for match in MANUAL_CHILD_API.finditer(masked):
        violations.append(
            Violation(
                relative,
                line_number(original, match.start()),
                "BEVY-BSN-002",
                "manual child-link APIs are forbidden; compose a bsn! Scene",
            )
        )

    for match in DIRECT_SPAWN.finditer(masked):
        opening = masked.find("(", match.start(), match.end())
        closing = matching_paren(masked, opening)
        arguments = masked[opening + 1 : closing].lstrip() if closing else ""
        if arguments.startswith(ALLOWED_SPAWN_PREFIXES):
            continue
        violations.append(
            Violation(
                relative,
                line_number(original, match.start()),
                "BEVY-BSN-003",
                "direct entity spawn is forbidden; mount UI through spawn_scene",
            )
        )
    return violations


def scan(repository: Path, root: Path) -> list[Violation]:
    violations: list[Violation] = []
    for path in rust_files(root):
        violations.extend(scan_file(path, repository))
    return violations


def self_test() -> int:
    with tempfile.TemporaryDirectory(prefix="bevy-bsn-guard-") as raw:
        repository = Path(raw)
        source = repository / "crates/taskmanager-bevy-ui/src"
        source.mkdir(parents=True)
        (source / "clean.rs").write_text(
            """
            fn scene() {
                bsn! { Node { } Children [ ( Text(value) ) ] }
            }
            fn infra(mut commands: Commands) {
                commands.spawn(Camera2d);
                commands.spawn(Observer::new(bind));
            }
            // Node { } Children [ ] Text(value) commands.spawn(Button);
            let example = "Node { Children [ Text(value) ] }";
            """,
            encoding="utf-8",
        )
        (source / "violations.rs").write_text(
            """
            fn bad(mut commands: Commands) {
                let node = Node { width: px(1.0) };
                let children = Children [];
                let text = Text(value);
                commands.spawn(Button);
                commands.entity(root).with_children(|_| {});
            }
            """,
            encoding="utf-8",
        )
        (source / "broken.rs").write_text("fn broken() { bsn! {\n", encoding="utf-8")
        found = scan(repository, source)
        codes = [item.code for item in found]
        expected = [
            "BEVY-BSN-001",
            "BEVY-BSN-001",
            "BEVY-BSN-001",
            "BEVY-BSN-002",
            "BEVY-BSN-003",
            "BEVY-BSN-004",
        ]
        if sorted(codes) != sorted(expected):
            print(f"self-test failed: expected {expected}, found {codes}", file=sys.stderr)
            return 1
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--mode", choices=("report", "enforce"), default="enforce")
    parser.add_argument("--root", type=Path)
    args = parser.parse_args()
    if args.self_test:
        return self_test()

    repository = Path(__file__).resolve().parents[2]
    root = args.root or Path("crates/taskmanager-bevy-ui/src")
    if not root.is_absolute():
        root = repository / root
    root = root.resolve()
    violations = scan(repository, root)
    for item in violations:
        print(f"{item.path}:{item.line}: {item.code}: {item.detail}")
    return int(bool(violations) and args.mode == "enforce")


if __name__ == "__main__":
    raise SystemExit(main())
