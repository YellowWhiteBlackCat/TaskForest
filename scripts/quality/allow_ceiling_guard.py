#!/usr/bin/env python3
"""Fail-closed guard: no bare allow/expect, anywhere.

`#[allow(dead_code)]` is not a decision: it hides production dead code from the
compiler, and this repository's charter forbids placeholder surface ("either it
is consumed or it is gone"). The same holds for every other bare lint allow —
it silences a signal that was trying to say something. The sanctioned mechanism
is the **scoped** form, which this guard deliberately cannot see because the
allow lives inside ``cfg_attr(...)``::

    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", allow(dead_code))]

    #![cfg_attr(target_env = "msvc", allow(linker_messages))]

Rule: a bare ``#[allow(...)]`` / ``#![allow(...)]`` / ``#[expect(...)]`` in a
workspace Rust source (``crates/**``, ``src/**``, ``tests/**``) is a violation.
The audited census is **zero**, so a new bare allow must edit ``CEILINGS`` in
this file in the same change — which makes it a reviewed decision rather than a
silent one. Scope it with ``cfg_attr``, fix the underlying cause, or delete the
item.

The guard strips line comments (so prose that mentions an attribute does not
count) and reads the multi-line shape rustfmt produces for a long lint list.
An attribute it cannot parse is a failure, never an under-count.

Exit codes: 0 no bare allow, 1 a violation, 2 parse failure (an attribute this
guard cannot read, or no source tree). ``--json`` emits the verdict
machine-readably.

Usage::

    python3 scripts/quality/allow_ceiling_guard.py
    python3 scripts/quality/allow_ceiling_guard.py --json
    python3 scripts/quality/allow_ceiling_guard.py --self-test
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path

SOURCE_ROOTS = ("crates", "src", "tests")

# The audited census is zero: every bare allow was either refactored away,
# scoped with `cfg_attr`, or deleted. A new bare allow must edit this table in
# the same change — which is the point: the addition is reviewed, not silent.
CEILINGS: dict[str, int] = {}

# A bare attribute occupies its own line (rustfmt's shape) or a short run of
# lines when the lint list is long. Anything else fails closed.
BARE_ATTRIBUTE = re.compile(
    r"^[ \t]*#!?\[(?P<kind>allow|expect)\((?P<lints>[^()]*)\)\][ \t]*$"
)
OPENED_ATTRIBUTE = re.compile(r"^[ \t]*#!?\[(allow|expect)\(")

EXIT_OK = 0
EXIT_VIOLATION = 1
EXIT_PARSE_FAILURE = 2


@dataclass
class Verdict:
    code: int
    message: str
    counts: dict[str, int] = field(default_factory=dict)
    ceilings: dict[str, int] = field(default_factory=dict)
    violations: list[str] = field(default_factory=list)

    def to_json(self) -> str:
        return json.dumps(
            {
                "verdict": {0: "pass", 1: "violation", 2: "parse_failure"}[self.code],
                "message": self.message,
                "counts": self.counts,
                "ceilings": self.ceilings,
                "violations": self.violations,
            },
            sort_keys=True,
        )


def scan_sources(
    text: str, source: str
) -> tuple[list[tuple[str, int, str]], str | None]:
    """Return ``(occurrences, parse_error)`` where each occurrence is
    ``(source, line, lint)``. Never raises and never under-counts."""
    found: list[tuple[str, int, str]] = []
    lines = text.splitlines()
    index = 0
    while index < len(lines):
        line = lines[index].split("//", 1)[0]
        number = index + 1
        match = BARE_ATTRIBUTE.match(line)
        if match is None:
            if OPENED_ATTRIBUTE.match(line) is None:
                index += 1
                continue
            # Accumulate a multi-line attribute until its parentheses balance.
            buffer = line
            while buffer.count("(") > buffer.count(")"):
                index += 1
                if index >= len(lines):
                    return found, f"{source}:{number}: unterminated allow/expect"
                buffer += " " + lines[index].split("//", 1)[0]
            match = BARE_ATTRIBUTE.match(buffer)
            if match is None:
                return found, f"{source}:{number}: unreadable allow/expect shape"
        for lint in (part.strip() for part in match.group("lints").split(",")):
            if lint:
                found.append((source, number, lint))
        index += 1
    return found, None


def evaluate(
    sources: list[tuple[str, str]], ceilings: dict[str, int] | None = None
) -> Verdict:
    limits = dict(ceilings if ceilings is not None else CEILINGS)
    totals: dict[str, int] = {}
    violations: list[str] = []
    for source, text in sources:
        occurrences, parse_error = scan_sources(text, source)
        if parse_error is not None:
            return Verdict(EXIT_PARSE_FAILURE, parse_error, totals, limits)
        for _, _, lint in occurrences:
            totals[lint] = totals.get(lint, 0) + 1

    for lint, count in sorted(totals.items()):
        ceiling = limits.get(lint)
        if ceiling is None:
            violations.append(
                f"bare `{lint}` × {count} is not in the audited census; scope it "
                f"with `cfg_attr(...)`, fix the underlying cause, delete the item, "
                f"or add `{lint}` to CEILINGS in "
                f"scripts/quality/allow_ceiling_guard.py in the same change"
            )
        elif count > ceiling:
            violations.append(
                f"`{lint}` count {count} exceeds the audited ceiling {ceiling}"
            )

    if violations:
        return Verdict(EXIT_VIOLATION, "; ".join(violations), totals, limits, violations)
    return Verdict(EXIT_OK, "no bare allow/expect in the workspace sources", totals, limits)


def run(repo_root: Path, as_json: bool) -> int:
    sources: list[tuple[str, str]] = []
    for name in SOURCE_ROOTS:
        root = repo_root / name
        if root.is_dir():
            sources.extend(
                (str(path.relative_to(repo_root)), path.read_text(encoding="utf-8"))
                for path in sorted(root.rglob("*.rs"))
            )
    if not sources:
        verdict = Verdict(
            EXIT_PARSE_FAILURE,
            f"no Rust sources under any of {', '.join(SOURCE_ROOTS)} in {repo_root}",
        )
    else:
        verdict = evaluate(sources)

    if as_json:
        print(verdict.to_json())
    else:
        prefix = {EXIT_OK: "PASS", EXIT_VIOLATION: "FAIL", EXIT_PARSE_FAILURE: "ERROR"}[
            verdict.code
        ]
        print(f"allow-ceiling guard: {prefix} {verdict.message}")
        for violation in verdict.violations[:10]:
            print(f"  - {violation}")
    return verdict.code


def self_test() -> int:
    """Prove the guard stays green on the scoped form and goes red on any bare
    allow, an unreadable attribute, or a ceiling that hides one."""
    cases: list[tuple[str, str, int]] = [
        (
            "scoped cfg_attr is invisible",
            '#[cfg_attr(test, allow(dead_code))]\nfn f() {}\n',
            EXIT_OK,
        ),
        (
            "scoped inner cfg_attr is invisible",
            '#![cfg_attr(target_env = "msvc", allow(linker_messages))]\n',
            EXIT_OK,
        ),
        (
            "prose is not an attribute",
            "// sprinkling `#[allow(clippy::type_complexity)]` everywhere\n",
            EXIT_OK,
        ),
        ("bare dead_code is banned", "#[allow(dead_code)]\nfn f() {}\n", EXIT_VIOLATION),
        ("inner dead_code is banned", "#![allow(dead_code)]\n", EXIT_VIOLATION),
        ("expect(dead_code) is banned", "#[expect(dead_code)]\n", EXIT_VIOLATION),
        ("bare unused_imports is banned", "#[allow(unused_imports)]\n", EXIT_VIOLATION),
        (
            "bare too_many_arguments is banned",
            "#[allow(clippy::too_many_arguments)]\n",
            EXIT_VIOLATION,
        ),
        (
            "multi-line bare attribute is readable and banned",
            "#[allow(\n    clippy::too_many_arguments,\n    clippy::collapsible_if\n)]\n",
            EXIT_VIOLATION,
        ),
        ("unterminated attribute", "#[allow(clippy::type_complexity\n", EXIT_PARSE_FAILURE),
    ]
    failures = 0
    for name, text, expected in cases:
        actual = evaluate([("<fixture>", text)]).code
        if actual != expected:
            failures += 1
            print(f"  FAIL {name}: expected exit {expected}, got {actual}")
    # A ceiling entry is the deliberate escape hatch and must still work.
    if evaluate([("<fixture>", "#[allow(legacy_lint)]\n")], {"legacy_lint": 1}).code != EXIT_OK:
        failures += 1
        print("  FAIL an audited ceiling must permit its kind")
    total = len(cases) + 1
    if failures:
        print(f"allow-ceiling guard self-test: FAIL ({total - failures}/{total} checks)")
        return 1
    print(f"allow-ceiling guard self-test: PASS ({total}/{total} checks)")
    return 0


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--json", action="store_true", help="emit the verdict as JSON")
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="prove the guard goes red on a bare allow and fails closed",
    )
    parser.add_argument(
        "--repo-root",
        type=Path,
        default=Path(__file__).resolve().parents[2],
        help="repository root that owns crates/, src/ and tests/",
    )
    args = parser.parse_args(argv)

    if args.self_test:
        return self_test()
    return run(args.repo_root, args.json)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
