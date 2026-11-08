#!/usr/bin/env python3
"""Enforce the Bevy frontend's 100% ``bsn!`` UI-authoring contract.

The production Bevy crate has one route for declaring UI hierarchy: a Bevy
Scene built by ``bsn!``. This guard deliberately does not reject ECS state
binding. It rejects the older or parallel hierarchy routes instead:

* UI primitives such as ``Node { ... }``, ``Children [ ... ]`` and ``Text(...)``
  outside a ``bsn!`` scene;
* ``with_children``/manual child-link APIs and legacy UI bundles;
* direct ``commands.spawn``/``world.spawn`` calls, except observer
  infrastructure that is not a UI tree;
* ``template_value``/``TemplateValue`` in any position (hard zero): values a
  scene can express are declared structurally, foreign whole values that a
  scene cannot express go through the registered deferred ``apply_scene``
  patch exit, never through the value wrapper;
* a ``.insert(`` chained directly onto ``spawn_scene``/``spawn_scene_list``:
  markers, styles and hierarchy components belong in the declared scene, and
  deferred representation is stamped with ``apply_scene`` of a ``bsn!`` patch.

It scans only ``crates/taskmanager-bevy-ui/src`` by default. Tests and Bevy's
own plugin internals are outside the production authoring contract. The
scanner masks comments and literals before checking balanced macro spans, so
examples in documentation cannot satisfy or trip the rule accidentally.
"""

from __future__ import annotations

import argparse
import importlib.util
import re
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path


def load_shared_scene_owner():
    """Import the vendored zhuge-ui-kit scene executor beside this guard."""

    owner_path = Path(__file__).with_name("bevy_scene_gate.py")
    spec = importlib.util.spec_from_file_location("bevy_scene_gate", owner_path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load shared scene executor: {owner_path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


scene_owner = load_shared_scene_owner()
mask_noncode = scene_owner.mask_noncode
line_number = scene_owner.line_number
scene_spans = scene_owner.scene_spans
inside_scene = scene_owner.inside_scene
matching_paren = scene_owner.matching_paren


BSN_START = re.compile(r"\bbsn!\s*\{")
UI_CONSTRUCTION = re.compile(
    r"\b(?:Node|Children|Text|ImageNode)\s*(?:\{|\[|\()"
    r"|\b(?:NodeBundle|TextBundle|ButtonBundle|ImageBundle|ChildBuilder|"
    r"ChildSpawnerCommands)\b"
)
MANUAL_CHILD_API = re.compile(
    r"\.\s*(?:with_children|push_children|add_child|add_children)\s*\("
)
DIRECT_SPAWN = re.compile(
    r"\b(?:commands|world|world_mut)\s*\.\s*spawn(?:_batch)?\s*\("
)
ALLOWED_SPAWN_PREFIXES = ("Observer::new",)
TEMPLATE_VALUE = re.compile(r"\btemplate_value\b|\bTemplateValue\b")
SCENE_MOUNT = re.compile(
    r"\.\s*(?:spawn_scene|spawn_scene_list|queue_spawn_scene)\s*\("
)
INSERT_CHAIN = re.compile(r"\s*\.\s*insert\s*\(")


@dataclass(frozen=True)
class Violation:
    path: str
    line: int
    code: str
    detail: str


def in_return_position(masked: str, start: int) -> bool:
    """A ``Type {`` match right after ``->`` is a signature, not construction.

    The type may be path-qualified (``accesskit::Node {``), so walk the
    ``::``-separated prefix back before looking for the return arrow.
    """

    prefix = re.search(r"[\w:]+$", masked[:start])
    before = masked[: prefix.start()] if prefix else masked[:start]
    return before.rstrip().endswith("->")


def rust_files(root: Path) -> list[Path]:
    if root.is_file():
        return [root] if root.suffix == ".rs" else []
    return sorted(path for path in root.rglob("*.rs") if path.is_file())


def scan_file(path: Path, repository: Path) -> list[Violation]:
    original = path.read_text(encoding="utf-8")
    masked = mask_noncode(original)
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
        if not inside_scene(match.start(), spans) and not in_return_position(
            masked, match.start()
        ):
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

    for match in TEMPLATE_VALUE.finditer(masked):
        violations.append(
            Violation(
                relative,
                line_number(original, match.start()),
                "BEVY-BSN-005",
                "template_value is hard-zero; declare the fields or patch with "
                "apply_scene(bsn! { ... })",
            )
        )

    for match in SCENE_MOUNT.finditer(masked):
        opening = masked.find("(", match.start(), match.end())
        closing = matching_paren(masked, opening)
        if closing is None:
            continue
        chained = INSERT_CHAIN.match(masked, closing + 1)
        if chained:
            insert_at = masked.find(".", chained.start())
            violations.append(
                Violation(
                    relative,
                    line_number(original, insert_at),
                    "BEVY-BSN-006",
                    "creation-chain insert bypasses the declared scene; move the "
                    "component into bsn! or stamp it with apply_scene",
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
                commands.spawn_scene(bsn! { Camera2d });
            }
            fn infra(mut commands: Commands) {
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
                let plate = ImageNode { image };
                commands.spawn(Button);
                commands.entity(root).with_children(|_| {});
                commands
                    .spawn_scene(bsn! { Node { } })
                    .insert(PageContent { page });
                let wrapped = template_value(transform);
            }
            use bevy::scene::template_value;
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
            "BEVY-BSN-001",
            "BEVY-BSN-002",
            "BEVY-BSN-003",
            "BEVY-BSN-005",
            "BEVY-BSN-005",
            "BEVY-BSN-006",
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
