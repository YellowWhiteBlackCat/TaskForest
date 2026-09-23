#!/usr/bin/env python3
"""Fail-closed guard: owner types are imported, never spelled inline.

The charter is explicit: *"Import owner types with `use` at the module
boundary. Do not introduce long qualified paths as a substitute for an import,
especially after a core type migration."* An alias (`use X as Y`) is equally
forbidden (that is `rust_surface_guard`'s RUST-SURFACE-001/002), so the only
legal way to name another workspace crate's type, function or constant is a
plain `use taskmanager_owner::path::{Item};` at the module boundary.

This guard counts the remaining inline occurrences of a workspace owner path
(``taskmanager_<crate>::`` outside a `use` declaration, outside comments and
outside string literals) and holds each crate to the audited census in
``CEILINGS``:

* a count **above** its ceiling fails — new inline paths are forbidden;
* a crate with no entry whose count is non-zero fails — a new crate must be
  registered consciously;
* lowering a ceiling needs no edit here.

The census is **empty**: the 7016 audited sites were all imported at the module
boundary in the W40 cleanup, so any inline owner path is now a failure and
there is no ceiling left to raise.

Exit codes: 0 within the census, 1 a violation, 2 parse failure (no source
tree). ``--json`` emits the verdict machine-readably.

Usage::

    python3 scripts/quality/inline_path_guard.py
    python3 scripts/quality/inline_path_guard.py --json
    python3 scripts/quality/inline_path_guard.py --self-test
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path

SOURCE_ROOTS = ("crates", "src", "tests")

# An owner path is a workspace crate name followed by `::`. The lookbehind keeps
# a path fragment inside a longer identifier from matching.
OWNER_PATH = re.compile(r"(?<![A-Za-z_:])taskmanager_[a-z_]+::")

# A `use` declaration (including `pub use` / `pub(crate) use`) is the legal form
# and is never counted; so is any text inside a string literal, which is data
# rather than a type reference.
USE_DECL = re.compile(r"^(?:pub(?:\([^)]*\))?\s+)?use\s")
STRING_LITERAL = re.compile(r'r#*"(?:[^"]|"(?!#))*"#*|"(?:[^"\\]|\\.)*"')

# The audited census. Every inline owner path has been imported at the module
# boundary, so the census is empty: a new inline path fails, and there is no
# ceiling to raise. Re-adding an entry is a conscious, reviewed decision.
CEILINGS: dict[str, int] = {}

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


def count_inline_paths(text: str) -> int:
    """Count owner paths outside `use` declarations, comments and strings."""
    total = 0
    in_use = False
    for raw in text.splitlines():
        line = raw.strip()
        if line.startswith("//"):
            continue
        if USE_DECL.match(line):
            in_use = not line.endswith(";")
            continue
        if in_use:
            if line.endswith(";"):
                in_use = False
            continue
        if OWNER_PATH.search(STRING_LITERAL.sub('""', raw)):
            total += 1
    return total


def owner_of(path: str) -> str:
    """The census key for a workspace-relative source path."""
    parts = path.split("/")
    if parts[0] == "crates" and len(parts) > 1:
        return parts[1]
    return parts[0]


def evaluate(
    sources: list[tuple[str, str]], ceilings: dict[str, int] | None = None
) -> Verdict:
    limits = dict(ceilings if ceilings is not None else CEILINGS)
    counts: dict[str, int] = {}
    for path, text in sources:
        owner = owner_of(path)
        counts[owner] = counts.get(owner, 0) + count_inline_paths(text)

    violations: list[str] = []
    for owner, count in sorted(counts.items()):
        if count == 0:
            continue
        ceiling = limits.get(owner)
        if ceiling is None:
            violations.append(
                f"{owner}: {count} inline owner path(s) with no audited census; "
                f"import the owner type with `use` or register the crate in "
                f"CEILINGS in the same change"
            )
        elif count > ceiling:
            violations.append(
                f"{owner}: {count} inline owner path(s) exceeds the audited "
                f"census {ceiling}; import the owner type with `use` instead of "
                f"spelling it inline"
            )

    if violations:
        return Verdict(EXIT_VIOLATION, "; ".join(violations), counts, limits, violations)
    return Verdict(EXIT_OK, "every owner path is within its audited census", counts, limits)


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
        print(f"inline-path guard: {prefix} {verdict.message}")
        for violation in verdict.violations[:10]:
            print(f"  - {violation}")
    return verdict.code


def self_test() -> int:
    """Prove the guard goes red on a new inline owner path and stays green on
    the import form, a comment and a lowered census."""
    cases: list[tuple[str, str, int]] = [
        ("import form is clean", "use taskmanager_core::core::Alerts;\n", EXIT_OK),
        (
            "public re-export is clean",
            "pub use taskmanager_platform_linux::NativePlatformRuntime;\n",
            EXIT_OK,
        ),
        (
            "scoped re-export is clean",
            "pub(crate) use taskmanager_core::core::Alerts;\n",
            EXIT_OK,
        ),
        ("comment is not a path use", "// see taskmanager_core::core::Alerts\n", EXIT_OK),
        (
            "string literal is not a path use",
            'let m = "taskmanager_core::core::Alerts";\n',
            EXIT_OK,
        ),
        (
            "inline path raises the count",
            "fn f(x: &taskmanager_theme::Theme) {}\n",
            EXIT_VIOLATION,
        ),
        (
            "inline call raises the count",
            "fn f() { taskmanager_shell::presentation::missing_value(); }\n",
            EXIT_VIOLATION,
        ),
        (
            "unregistered crate is a violation",
            "fn f() { taskmanager_brand_new::thing(); }\n",
            EXIT_VIOLATION,
        ),
    ]
    failures = 0
    for name, text, expected in cases:
        actual = evaluate([("crates/fixture/src/lib.rs", text)]).code
        if actual != expected:
            failures += 1
            print(f"  FAIL {name}: expected exit {expected}, got {actual}")
    # The census mechanism still works when a crate is explicitly registered.
    if evaluate([("crates/fixture/src/lib.rs", "taskmanager_a::b\n")], {"fixture": 1}).code != EXIT_OK:
        failures += 1
        print("  FAIL an audited census must permit its count")
    if evaluate([("crates/fixture/src/lib.rs", "taskmanager_a::b\ntaskmanager_c::d\n")], {"fixture": 1}).code != EXIT_VIOLATION:
        failures += 1
        print("  FAIL a count above the census must fail")
    total = len(cases) + 2
    if failures:
        print(f"inline-path guard self-test: FAIL ({total - failures}/{total} checks)")
        return 1
    print(f"inline-path guard self-test: PASS ({total}/{total} checks)")
    return 0


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--json", action="store_true", help="emit the verdict as JSON")
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="prove the guard goes red on a new inline owner path",
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
