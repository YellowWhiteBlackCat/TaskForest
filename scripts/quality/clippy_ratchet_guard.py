#!/usr/bin/env python3
"""Fail-closed ratchet guard: the workspace structural thresholds may only fall.

``clippy_command_parity_guard.py`` proves that the CI and local clippy
*commands* still carry the structural-ratchet flags
(``-W clippy::cognitive_complexity``, ``-W clippy::too_many_lines``). It
deliberately does not read ``clippy.toml``, so nothing stopped a later change
from raising ``cognitive-complexity-threshold`` / ``too-many-lines-threshold``
back above the audited ceiling and silently re-opening the hole the flags were
added to close (W12-C found the flags missing on CI; W14-B aligned the
commands).

This guard pins the audited ceiling and fails when ``clippy.toml`` exceeds it:

``cognitive-complexity-threshold <= 44``
``too-many-lines-threshold <= 500``

Lowering a threshold needs no change here. Raising one is a conscious act: it
must edit this pin in the same change, so the regression is visible in review
rather than silent. A missing key or a non-integer value is a parse failure
(exit 2), never an implicit pass.

Exit codes: 0 within the ceiling, 1 above it, 2 parse failure (missing file or
key, duplicate key, non-integer value). ``--json`` emits the same verdict
machine-readably.

Usage::

    python3 scripts/quality/clippy_ratchet_guard.py
    python3 scripts/quality/clippy_ratchet_guard.py --json
    python3 scripts/quality/clippy_ratchet_guard.py --self-test
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path

CLIPPY_TOML = "clippy.toml"

# The audited ceiling. Lowering a threshold never needs an edit here; raising
# one must edit this table in the same change, which is the whole point.
CEILINGS: dict[str, int] = {
    "cognitive-complexity-threshold": 44,
    "too-many-lines-threshold": 500,
}

# `key = 44` / `key = 44  # comment`. clippy.toml is a flat key/value file.
KEY_LINE = re.compile(r"^[ \t]*(?P<key>[A-Za-z0-9_-]+)[ \t]*=[ \t]*(?P<value>.*?)[ \t]*$")

EXIT_OK = 0
EXIT_ABOVE_CEILING = 1
EXIT_PARSE_FAILURE = 2


@dataclass
class Verdict:
    code: int
    message: str
    thresholds: dict[str, int] = field(default_factory=dict)
    ceilings: dict[str, int] = field(default_factory=dict)

    def to_json(self) -> str:
        return json.dumps(
            {
                "verdict": {0: "pass", 1: "above_ceiling", 2: "parse_failure"}[self.code],
                "message": self.message,
                "thresholds": self.thresholds,
                "ceilings": self.ceilings,
            },
            sort_keys=True,
        )


def evaluate(text: str, ceilings: dict[str, int] | None = None) -> Verdict:
    """Judge one ``clippy.toml`` body against the pinned ceiling.

    Never raises: every failure mode is a typed verdict so the caller can map
    it to an exit code without a traceback.
    """
    limits = dict(ceilings if ceilings is not None else CEILINGS)

    seen: dict[str, int] = {}
    for number, raw_line in enumerate(text.splitlines(), start=1):
        line = raw_line.split("#", 1)[0].strip()
        if not line:
            continue
        match = KEY_LINE.match(line)
        if match is None:
            continue
        key = match.group("key")
        if key not in limits:
            continue
        value = match.group("value").strip()
        try:
            parsed = int(value)
        except ValueError:
            return Verdict(
                EXIT_PARSE_FAILURE,
                f"{CLIPPY_TOML}:{number}: `{key}` is not an integer (`{value}`)",
                seen,
                limits,
            )
        if key in seen:
            return Verdict(
                EXIT_PARSE_FAILURE,
                f"{CLIPPY_TOML}:{number}: `{key}` is declared twice, so the ceiling is ambiguous",
                seen,
                limits,
            )
        seen[key] = parsed

    for key, ceiling in limits.items():
        if key not in seen:
            return Verdict(
                EXIT_PARSE_FAILURE,
                f"{CLIPPY_TOML}: `{key}` is missing; the ratchet cannot be verified",
                seen,
                limits,
            )
        if seen[key] > ceiling:
            return Verdict(
                EXIT_ABOVE_CEILING,
                f"{CLIPPY_TOML}: `{key} = {seen[key]}` exceeds the audited ceiling "
                f"{ceiling}. Raising a structural threshold must edit CEILINGS in "
                f"scripts/quality/clippy_ratchet_guard.py in the same change, so the "
                f"regression is reviewed instead of silent.",
                seen,
                limits,
            )

    return Verdict(
        EXIT_OK,
        "thresholds are within the audited ceiling",
        seen,
        limits,
    )


def run(repo_root: Path, as_json: bool) -> int:
    path = repo_root / CLIPPY_TOML
    try:
        text = path.read_text(encoding="utf-8")
    except OSError as error:
        verdict = Verdict(EXIT_PARSE_FAILURE, f"cannot read {CLIPPY_TOML}: {error}")
    else:
        verdict = evaluate(text)

    if as_json:
        print(verdict.to_json())
    else:
        prefix = {EXIT_OK: "PASS", EXIT_ABOVE_CEILING: "FAIL", EXIT_PARSE_FAILURE: "ERROR"}[
            verdict.code
        ]
        print(f"clippy-ratchet guard: {prefix} {verdict.message}")
    return verdict.code


def self_test() -> int:
    """Prove the guard is not a rubber stamp: it must go red on a raise and
    fail closed on an unreadable shape."""
    cases: list[tuple[str, str, int]] = [
        ("at the ceiling", "cognitive-complexity-threshold = 44\ntoo-many-lines-threshold = 500\n", EXIT_OK),
        ("below the ceiling", "cognitive-complexity-threshold = 40\ntoo-many-lines-threshold = 420\n", EXIT_OK),
        ("comments and blank lines", "# note\n\ncognitive-complexity-threshold = 44  # inline\ntoo-many-lines-threshold = 500\n", EXIT_OK),
        ("raised complexity", "cognitive-complexity-threshold = 45\ntoo-many-lines-threshold = 500\n", EXIT_ABOVE_CEILING),
        ("raised lines", "cognitive-complexity-threshold = 44\ntoo-many-lines-threshold = 650\n", EXIT_ABOVE_CEILING),
        ("legacy ceiling", "cognitive-complexity-threshold = 48\ntoo-many-lines-threshold = 650\n", EXIT_ABOVE_CEILING),
        ("missing key", "cognitive-complexity-threshold = 44\n", EXIT_PARSE_FAILURE),
        ("non-integer", "cognitive-complexity-threshold = many\ntoo-many-lines-threshold = 500\n", EXIT_PARSE_FAILURE),
        ("duplicate key", "cognitive-complexity-threshold = 44\ncognitive-complexity-threshold = 40\ntoo-many-lines-threshold = 500\n", EXIT_PARSE_FAILURE),
        ("empty file", "", EXIT_PARSE_FAILURE),
    ]
    failures = 0
    for name, text, expected in cases:
        actual = evaluate(text).code
        if actual != expected:
            failures += 1
            print(f"  FAIL {name}: expected exit {expected}, got {actual}")
    total = len(cases)
    if failures:
        print(f"clippy-ratchet guard self-test: FAIL ({total - failures}/{total} checks)")
        return 1
    print(f"clippy-ratchet guard self-test: PASS ({total}/{total} checks)")
    return 0


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--json", action="store_true", help="emit the verdict as JSON")
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="prove the guard goes red on a raised threshold and fails closed",
    )
    parser.add_argument(
        "--repo-root",
        type=Path,
        default=Path(__file__).resolve().parents[2],
        help="repository root that owns clippy.toml",
    )
    args = parser.parse_args(argv)

    if args.self_test:
        return self_test()
    return run(args.repo_root, args.json)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
