#!/usr/bin/env python3
"""Check the audited scene registry against real test and capture discovery.

The parity audit names 55 product scenes. ``scripts/parity/scene_registry.tsv``
binds every scene, in audit order, to one evidence anchor per frontend:

* ``test:<nextest name>`` — a behavior test resolved through that frontend's
  live ``cargo nextest list`` discovery;
* ``capture:<scenario>`` — a committed capture scenario when no scene-specific
  behavior test exists (visual evidence, explicitly typed);
* ``-`` — only where the audit declares the scene ``Unsupported`` or
  ``Pending`` for that frontend.

The guard fails closed on scene set/order drift, a missing delivered cell, a
dangling test, or an unknown capture scenario. Per-frontend behavior remains
owner-reviewed; this gate makes every claim's evidence addressable.
"""

from __future__ import annotations

import argparse
import csv
import io
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path

AUDIT = Path("docs/UI_PARITY_AUDIT.md")
REGISTRY = Path("scripts/parity/scene_registry.tsv")
SCENE = re.compile(r"`([a-z][a-z0-9-]*)`")
FRONTENDS = ("gpui", "iced", "tui", "bevy")
AUDIT_COLUMNS = {"iced": 3, "bevy": 4, "tui": 5}
CAPTURES = {
    "gpui": Path("scripts/capture_scenarios.tsv"),
    "iced": Path("scripts/capture_iced_scenarios.tsv"),
    "tui": Path("scripts/capture_tui_scenarios.tsv"),
    "bevy": Path("scripts/capture_bevy_scenarios.tsv"),
}
PACKAGES = {
    "gpui": ("taskmanager-gpui", ("--features", "test-support")),
    "iced": ("taskmanager-iced", ()),
    "tui": ("taskmanager-tui", ()),
    "bevy": ("taskmanager-bevy-ui", ()),
}
DELIVERED = {"Aligned", "AcceptedDifference", "Partial"}


@dataclass(frozen=True)
class Verdict:
    code: int
    message: str


def audit_scenes(text: str) -> list[str]:
    table = text.rsplit("差异索引", 1)[0]
    return list(dict.fromkeys(SCENE.findall(table)))


def audit_statuses(text: str) -> dict[str, dict[str, str]]:
    statuses: dict[str, dict[str, str]] = {}
    for line in text.splitlines():
        if not line.startswith("|"):
            continue
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        if len(cells) != 7:
            continue
        token = re.search(r"`([a-z0-9-]+)`", cells[1])
        if token is None:
            continue
        statuses[token.group(1)] = {
            frontend: cells[index].split("[", 1)[0].strip()
            for frontend, index in AUDIT_COLUMNS.items()
        }
    if not statuses:
        raise ValueError("audit has no scene status rows")
    return statuses


def registry_rows(text: str) -> list[dict[str, str]]:
    rows: list[dict[str, str]] = []
    for raw in text.splitlines():
        line = raw.strip()
        if not line or line.startswith("#") or line.startswith("scene\t"):
            continue
        fields = line.split("\t")
        if len(fields) != 1 + len(FRONTENDS):
            raise ValueError(f"registry row must have 5 fields, got {raw!r}")
        scene, *evidence = (field.strip() for field in fields)
        if not scene:
            raise ValueError(f"empty scene in {raw!r}")
        rows.append(dict(zip(FRONTENDS, evidence), scene=scene))
    return rows


def capture_names(path: Path) -> set[str]:
    if not path.is_file():
        raise ValueError(f"missing capture matrix {path}")
    lines = [
        line
        for line in path.read_text(encoding="utf-8").splitlines()
        if line and not line.startswith("#")
    ]
    reader = csv.DictReader(io.StringIO("\n".join(lines)), delimiter="\t")
    if reader.fieldnames is None or "name" not in reader.fieldnames:
        raise ValueError(f"{path} has no name column")
    return {row["name"] for row in reader if row.get("name")}


def discover(
    tests: dict[str, set[str]] | None,
    needed: set[str],
) -> dict[str, set[str]]:
    if tests is not None:
        return {frontend: tests[frontend] for frontend in needed}
    discovered: dict[str, set[str]] = {}
    for frontend in needed:
        package, features = PACKAGES[frontend]
        result = subprocess.run(
            ["cargo", "nextest", "list", "-p", package, *features],
            capture_output=True,
            text=True,
            check=False,
            timeout=300,
        )
        if result.returncode != 0:
            raise RuntimeError(
                f"cargo nextest list failed for {package}: "
                + (result.stderr.strip().splitlines() or ["no stderr"])[-1]
            )
        names = {
            line.split(" ", 1)[-1].strip()
            for line in result.stdout.splitlines()
            if line.startswith(package)
        }
        if not names:
            raise RuntimeError(f"no tests discovered for {package}")
        discovered[frontend] = names
    return discovered


def evaluate(
    scene_text: str,
    registry_text: str,
    tests: dict[str, set[str]] | None = None,
    captures: dict[str, set[str]] | None = None,
    expected_scenes: int = 55,
) -> Verdict:
    try:
        scenes = audit_scenes(scene_text)
        statuses = audit_statuses(scene_text)
        rows = registry_rows(registry_text)
        needed = {
            frontend
            for row in rows
            for frontend in FRONTENDS
            if row[frontend].startswith("test:")
        }
        discovered = discover(tests, needed)
        capture_sets = captures or {
            frontend: capture_names(path) for frontend, path in CAPTURES.items()
        }
    except (OSError, ValueError, RuntimeError) as error:
        return Verdict(2, str(error))
    if len(scenes) != expected_scenes:
        return Verdict(
            1, f"the audit must name {expected_scenes} scenes, got {len(scenes)}"
        )
    if [row["scene"] for row in rows] != scenes:
        return Verdict(1, "registry scene set or order differs from the audit")
    problems: list[str] = []
    for row in rows:
        scene = row["scene"]
        if scene not in statuses:
            problems.append(f"{scene}: no audit status row")
            continue
        for frontend in FRONTENDS:
            delivered = frontend == "gpui" or statuses[scene][frontend] in DELIVERED
            evidence = row[frontend]
            if not delivered:
                if evidence != "-":
                    problems.append(f"{scene}/{frontend}: absent scene must carry -")
                continue
            if evidence == "-":
                problems.append(f"{scene}/{frontend}: delivered scene lacks evidence")
                continue
            kind, _, name = evidence.partition(":")
            if kind == "test" and name in discovered[frontend]:
                continue
            if kind == "capture" and name in capture_sets[frontend]:
                continue
            problems.append(f"{scene}/{frontend}: dangling {evidence}")
    if problems:
        return Verdict(1, "; ".join(problems[:8]))
    return Verdict(0, f"{len(rows)} scenes, every evidence anchor resolvable")


SELF_AUDIT = """
| | `a` | one | Aligned | Aligned | Aligned | — |
| | `b` | two | Unsupported | Aligned | Aligned | — |
差异索引
"""
SELF_REGISTRY = """scene\tgpui\ticed\ttui\tbevy
a\ttest:g::one\ttest:i::one\tcapture:t\tcapture:b
b\ttest:g::two\t-\tcapture:t\tcapture:b
"""
SELF_TESTS = {
    "gpui": {"g::one", "g::two"},
    "iced": {"i::one"},
    "tui": set(),
    "bevy": set(),
}
SELF_CAPTURES = {
    "gpui": set(),
    "iced": set(),
    "tui": {"t"},
    "bevy": {"b"},
}


def self_test() -> int:
    cases = [
        ("clean registry", SELF_REGISTRY, 0),
        ("missing scene", SELF_REGISTRY.splitlines()[0] + "\n", 1),
        ("dangling test", SELF_REGISTRY.replace("g::one", "g::gone"), 1),
        ("dangling capture", SELF_REGISTRY.replace("capture:t", "capture:x", 1), 1),
        (
            "unsupported carries evidence",
            SELF_REGISTRY.replace("\t-\tcapture:t", "\tcapture:t\tcapture:t"),
            1,
        ),
    ]
    failures = 0
    for name, registry, expected in cases:
        actual = evaluate(
            SELF_AUDIT, registry, SELF_TESTS, SELF_CAPTURES, expected_scenes=2
        ).code
        if actual != expected:
            failures += 1
            print(f"  FAIL {name}: expected {expected}, got {actual}")
    if failures:
        print(f"scene-registry self-test: {failures} failure(s)")
        return 1
    print("scene-registry self-test: ok")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--repo-root", type=Path, default=Path("."))
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    root = args.repo_root
    verdict = evaluate(
        (root / AUDIT).read_text(encoding="utf-8"),
        (root / REGISTRY).read_text(encoding="utf-8"),
    )
    prefix = {0: "PASS", 1: "FAIL", 2: "ERROR"}[verdict.code]
    print(f"scene-registry: {prefix} {verdict.message}")
    return verdict.code


if __name__ == "__main__":
    sys.exit(main())
