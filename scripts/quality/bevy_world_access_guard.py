#!/usr/bin/env python3
"""Reject unrestricted world access in production Bevy UI code."""
from __future__ import annotations
import argparse
import importlib.util
import re
import sys
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


mask_noncode = load_shared_scene_owner().mask_noncode

RULES = (
    ("BEVY-WORLD-001", re.compile(r"\b(?:World|DeferredWorld|UnsafeWorldCell)\b"), "declare specific system parameters instead of the whole world"),
    ("BEVY-WORLD-002", re.compile(r"\.\s*world(?:_mut)?\s*\("), "business cannot recover the world through App"),
    ("BEVY-WORLD-003", re.compile(r"\.\s*queue\s*\(\s*(?:move\s+)?\|"), "queued closures must not hide unrestricted world access"),
)

def violations(source: str) -> list[tuple[int, str, str]]:
    masked = mask_noncode(source)
    return sorted((masked.count("\n", 0, match.start()) + 1, code, reason)
                  for code, pattern, reason in RULES for match in pattern.finditer(masked))

def self_test() -> None:
    for source in ("fn draw(world: &mut World) {}", "fn bind(world: DeferredWorld) {}", "struct Cache { world: UnsafeWorldCell<'static> }", "app.world_mut().resource_mut::<State>();", "commands.queue(move |context| context.resource_mut::<State>());"):
        assert violations(source), source
    assert not violations('fn draw(state: Res<State>, nodes: Query<&Node>, commands: Commands) {}')
    assert not violations('// World is engine-owned\nlet text = "DeferredWorld app.world()";')
    assert not violations('use bevy::scene::CommandsSceneExt;')
    print("Bevy world access self-test: PASS")

def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--repo-root", type=Path, default=Path(__file__).resolve().parents[2])
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return 0
    root = args.repo_root.resolve()
    files = sorted((root / "crates/taskmanager-bevy-ui/src").rglob("*.rs"))
    count = 0
    for path in files:
        for line, code, reason in violations(path.read_text()):
            print(f"{path.relative_to(root)}:{line}: {code}: {reason}")
            count += 1
    print(f"Bevy world access: {'FAIL' if count else 'PASS'} ({len(files)} files, {count} findings)")
    return int(count != 0)

if __name__ == "__main__":
    raise SystemExit(main())
