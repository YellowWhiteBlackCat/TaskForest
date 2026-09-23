#!/usr/bin/env python3
"""Fail-closed guard: dead code is scoped or gone, never silenced by a bare allow.

`#[allow(dead_code)]` is not a decision. It hides production dead code from the
compiler, and this repository's charter forbids placeholder surface ("either it
is consumed or it is gone"). The sanctioned mechanism for a helper that only
tests consume is a **test-scoped** gate, e.g.::

    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", allow(dead_code))]

That form is deliberately invisible to this guard: the allow lives *inside*
``cfg_attr(...)``, so it cannot silence a production build.

Two rules, both fail-closed:

1. ``dead_code`` is **banned** in any bare ``#[allow(...)]`` / ``#![allow(...)]``
   / ``#[expect(...)]`` attribute under ``crates/**``. Delete the item or scope
   it to tests.
2. Every other lint kind is a **ratchet**: its bare count may not exceed the
   audited ceiling below, and a kind with no entry is a failure, so a new allow
   kind must be a conscious edit to this table rather than a silent addition.

Exit codes: 0 within the rules, 1 a violation, 2 parse failure (an attribute
this guard cannot read, or no ``crates`` tree). ``--json`` emits the verdict
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

CRATES_DIR = "crates"

# The audited ceiling per lint kind. Lowering one needs no edit here; adding a
# kind (or raising a count) must edit this table in the same change.
#
# The counts are the audited census of *real* attributes: the guard strips line
# comments, so prose that mentions an attribute (e.g. the note in
# `taskmanager-ui/src/lib.rs`) does not inflate them. `too_many_arguments`
# includes the multi-line attribute in `taskmanager-bevy-ui/src/drain.rs`.
CEILINGS: dict[str, int] = {
    "clippy::too_many_arguments": 32,
    "clippy::type_complexity": 2,
    "clippy::collapsible_if": 1,
    "clippy::explicit_auto_deref": 1,
    "clippy::large_enum_variant": 1,
    "unused_imports": 1,
    "non_upper_case_globals": 1,
    "rustdoc::private_intra_doc_links": 1,
    "linker_messages": 1,
}

# The banned kind: silencing dead code is never the answer.
BANNED_KINDS: frozenset[str] = frozenset({"dead_code"})

# A bare attribute occupies its own line (rustfmt's shape). An `allow`/`expect`
# opened but not closed on the same line is a shape this guard cannot read, so
# it fails closed rather than under-counting.
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


def scan_sources(text: str, source: str) -> tuple[dict[str, int], list[str], str | None]:
    """Return ``(counts, violations, parse_error)`` for one source body.

    Handles both the single-line shape rustfmt produces for a short attribute
    and the multi-line shape it produces when the lint list exceeds the line
    width. Anything else is a parse failure (the guard never under-counts).
    """
    counts: dict[str, int] = {}
    violations: list[str] = []
    lines = text.splitlines()
    index = 0
    while index < len(lines):
        line = lines[index].split("//", 1)[0]
        start = index
        number = start + 1
        match = BARE_ATTRIBUTE.match(line)
        if match is None:
            if OPENED_ATTRIBUTE.match(line) is None:
                index += 1
                continue
            # Accumulate the multi-line attribute until its parentheses balance.
            buffer = line
            while buffer.count("(") > buffer.count(")"):
                index += 1
                if index >= len(lines):
                    return counts, violations, f"{source}:{number}: unterminated allow/expect"
                buffer += " " + lines[index].split("//", 1)[0]
            match = BARE_ATTRIBUTE.match(buffer)
            if match is None:
                return counts, violations, f"{source}:{number}: unreadable allow/expect shape"
        for lint in (part.strip() for part in match.group("lints").split(",")):
            if not lint:
                continue
            counts[lint] = counts.get(lint, 0) + 1
            if lint in BANNED_KINDS:
                violations.append(
                    f"{source}:{number}: `{lint}` must be deleted or scoped to tests, "
                    f"never silenced by a bare allow"
                )
        index += 1
    return counts, violations, None


def evaluate(sources: list[tuple[str, str]], ceilings: dict[str, int] | None = None) -> Verdict:
    limits = dict(ceilings if ceilings is not None else CEILINGS)
    totals: dict[str, int] = {}
    violations: list[str] = []
    for source, text in sources:
        counts, found, parse_error = scan_sources(text, source)
        if parse_error is not None:
            return Verdict(EXIT_PARSE_FAILURE, parse_error, totals, limits)
        violations.extend(found)
        for lint, count in counts.items():
            totals[lint] = totals.get(lint, 0) + count

    if violations:
        return Verdict(
            EXIT_VIOLATION,
            f"{len(violations)} bare allow(s) violate the dead-code rule",
            totals,
            limits,
            violations,
        )

    for lint, count in sorted(totals.items()):
        ceiling = limits.get(lint)
        if ceiling is None:
            violations.append(
                f"`{lint}` has no audited ceiling; add it to CEILINGS in "
                f"scripts/quality/allow_ceiling_guard.py in the same change"
            )
        elif count > ceiling:
            violations.append(
                f"`{lint}` count {count} exceeds the audited ceiling {ceiling}; "
                f"remove the allow or raise the ceiling in the same change"
            )

    if violations:
        return Verdict(EXIT_VIOLATION, "; ".join(violations), totals, limits, violations)
    return Verdict(EXIT_OK, "no bare allow violates the rules", totals, limits)


def run(repo_root: Path, as_json: bool) -> int:
    root = repo_root / CRATES_DIR
    if not root.is_dir():
        verdict = Verdict(EXIT_PARSE_FAILURE, f"no {CRATES_DIR}/ tree under {repo_root}")
    else:
        sources = [
            (str(path.relative_to(repo_root)), path.read_text(encoding="utf-8"))
            for path in sorted(root.rglob("*.rs"))
        ]
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
    """Prove the guard goes red on a new bare dead_code, on a raised count and
    on an unknown kind, and stays green on the sanctioned scoped form."""
    cases: list[tuple[str, str, int]] = [
        ("scoped cfg_attr is invisible", '#[cfg_attr(test, allow(dead_code))]\nfn f() {}\n', EXIT_OK),
        ("at the ceiling", "#[allow(clippy::type_complexity)]\n", EXIT_OK),
        ("below the ceiling", "#[allow(non_upper_case_globals)]\n", EXIT_OK),
        ("bare dead_code is banned", "#[allow(dead_code)]\nfn f() {}\n", EXIT_VIOLATION),
        ("inner dead_code is banned", "#![allow(dead_code)]\n", EXIT_VIOLATION),
        ("expect(dead_code) is banned", "#[expect(dead_code)]\n", EXIT_VIOLATION),
        (
            "raised count",
            "#[allow(clippy::type_complexity)]\n" * 4,
            EXIT_VIOLATION,
        ),
        ("unknown kind", "#[allow(clippy::needless_range_loop)]\n", EXIT_VIOLATION),
        (
            "multi-line attribute is readable",
            "#[allow(\n    clippy::type_complexity,\n    non_upper_case_globals\n)]\n",
            EXIT_OK,
        ),
        ("unterminated attribute", "#[allow(clippy::type_complexity\n", EXIT_PARSE_FAILURE),
    ]
    failures = 0
    for name, text, expected in cases:
        actual = evaluate([("<fixture>", text)]).code
        if actual != expected:
            failures += 1
            print(f"  FAIL {name}: expected exit {expected}, got {actual}")
    total = len(cases)
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
        help="prove the guard goes red on a new bare allow and fails closed",
    )
    parser.add_argument(
        "--repo-root",
        type=Path,
        default=Path(__file__).resolve().parents[2],
        help="repository root that owns crates/",
    )
    args = parser.parse_args(argv)

    if args.self_test:
        return self_test()
    return run(args.repo_root, args.json)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
