#!/usr/bin/env python3
"""Check that workspace crate production sources do not contain test bodies.

Production Rust sources keep only short path mounts; test bodies live in the
per-crate ``tests/common``, ``tests/headless`` and ``tests/gui`` layout.
"""

from __future__ import annotations

import argparse
import re
import sys
from dataclasses import dataclass
from pathlib import Path

TEST_MARKER = re.compile(
    r"^\s*(?:#\[\s*cfg\(\s*test\s*\)\s*\]|#\[\s*(?:[\w:]+::)?test\b|mod\s+tests\s*(?:\{|;))"
)


@dataclass(frozen=True)
class Violation:
    path: str
    line: int
    reason: str


def source_roots(repository: Path) -> list[Path]:
    roots = [repository / "src"]
    crates = repository / "crates"
    if crates.is_dir():
        roots.extend(
            package / "src"
            for package in sorted(crates.iterdir())
            if (package / "Cargo.toml").is_file()
        )
    return roots


def crate_roots(repository: Path, crates: list[str] | None = None) -> list[Path]:
    all_roots = [
        package
        for package in sorted((repository / "crates").iterdir())
        if (package / "Cargo.toml").is_file()
    ] if (repository / "crates").is_dir() else []
    if crates is None:
        return all_roots
    wanted = set(crates)
    missing = wanted - {package.name for package in all_roots}
    if missing:
        raise SystemExit(f"unknown crate(s) for --crate: {sorted(missing)}")
    return [package for package in all_roots if package.name in wanted]


def scan(repository: Path, crates: list[str] | None = None) -> list[Violation]:
    violations: list[Violation] = []
    scoped = crates is not None
    crate_filter = set(crates) if crates is not None else None
    for root in source_roots(repository):
        if not root.is_dir():
            continue
        if scoped:
            # Scope mode keeps only the named crates' production sources; the
            # repository-level src/ root stays out of a crate-scoped run.
            crate_name = root.parent.name if root.parent.name != repository.name else None
            if crate_name is None or crate_name not in crate_filter:
                continue
        for path in sorted(root.rglob("*.rs")):
            relative = path.relative_to(repository).as_posix()
            if "tests/" in f"{relative}/":
                violations.append(Violation(relative, 1, "test source directory is under src/"))
                continue
            lines = path.read_text(encoding="utf-8").splitlines()
            for index, raw_line in enumerate(lines):
                number = index + 1
                # A short path-mounted unit-test declaration is the only
                # transitional exception; its body must live under tests/.
                if raw_line.strip() == "#[cfg(test)]" and any(
                    "../tests/" in lines[offset]
                    for offset in range(index + 1, min(index + 3, len(lines)))
                ):
                    continue
                if raw_line.strip() == "mod tests;" and any(
                    "../tests/" in lines[offset]
                    for offset in range(max(0, index - 2), index)
                ):
                    continue
                if TEST_MARKER.match(raw_line):
                    violations.append(
                        Violation(relative, number, "inline test marker in production source")
                    )
    allowed_test_entries = {"common.rs", "headless.rs", "gui.rs"}
    allowed_test_directories = {"common", "headless", "gui"}
    for package in crate_roots(repository, crates):
        test_root = package / "tests"
        if not test_root.is_dir():
            continue
        for path in sorted(test_root.rglob("*.rs")):
            relative_to_tests = path.relative_to(test_root)
            if len(relative_to_tests.parts) == 1:
                if relative_to_tests.name in allowed_test_entries:
                    continue
            elif relative_to_tests.parts[0] in allowed_test_directories:
                continue
            relative = path.relative_to(repository).as_posix()
            violations.append(
                Violation(
                    relative,
                    1,
                    "test source must be under tests/common, tests/headless, or tests/gui",
                )
            )
    return violations


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mode", choices=("report", "enforce"), default="enforce")
    parser.add_argument("--repo-root", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument(
        "--crate",
        action="append",
        default=None,
        help="limit the scan to the named workspace crate(s); "
        "repeatable. Unset scans the whole workspace.",
    )
    parser.add_argument("--self-test", action="store_true")
    return parser.parse_args()


def self_test() -> None:
    samples = {
        "inline.rs": "#[cfg(test)]\nmod tests {}\n",
        "clean.rs": "pub fn production() {}\n",
    }
    for name, text in samples.items():
        found = [line for line in text.splitlines() if TEST_MARKER.match(line)]
        if (name == "inline.rs") != bool(found):
            raise RuntimeError(f"marker self-test failed for {name}")


def main() -> int:
    args = parse_args()
    if args.self_test:
        self_test()
        print("Test layout guard self-test: PASS")
        return 0
    repository = args.repo_root.resolve()
    violations = scan(repository, args.crate)
    try:
        scope = f" crates={','.join(sorted(args.crate))}" if args.crate else " workspace"
        print(f"Test layout guard: violations={len(violations)} mode={args.mode}{scope}")
        for violation in violations:
            print(f" - {violation.path}:{violation.line}: {violation.reason}")
    except BrokenPipeError:
        return 0
    return int(args.mode == "enforce" and bool(violations))


if __name__ == "__main__":
    sys.exit(main())
