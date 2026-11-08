#!/usr/bin/env python3
"""One percent readout, one owner, one spelling (ADR-020).

A percentage is the same fact everywhere: the TUI Performance page, the Iced
device cards, the Bevy Performance page, the GPUI dashboards, the CSV export,
the accessibility snapshot and the desktop notification must not disagree
about how `63.25%` reads, and a missing observation must read the same way in
all of them.

The SPELLINGS live in the shared floor (`taskforest_core::core::percent`): a
level reads as a whole percent, a rate keeps one decimal. The ABSENCE-AWARE
readouts live in the presentation layer
(`taskmanager_shell::presentation::percent`): they turn an unobserved or
non-finite measurement into the shared dash instead of a fabricated zero.

A surface that formats a percentage on its own is a second authority: it can
drift from the owner and it puts the product's number format outside the
presentation layer. This check fails that shape — a format string that carries
both a `%` sign and a placeholder spells a percentage locally — plus the two
neighbouring forks: a surface-local percent formatter, and the generic
suffixed scalar used to spell a percent.

The two owner modules are exempt: they are the spellings themselves.
"""

from __future__ import annotations

import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CRATES = ROOT / "crates"

# The owners. The floor spells the percent; the presentation layer adds the
# absence rule on top of it.
CORE_OWNER = CRATES / "taskforest-core/src/core/percent.rs"
SHELL_OWNER = CRATES / "taskmanager-shell/src/presentation/percent.rs"
OWNER_MODULES = (CORE_OWNER, SHELL_OWNER)

# The floor's spellings and the presentation layer's absence-aware readouts.
EXPECTED_OWNER_SYMBOLS = (
    (CORE_OWNER, "pub fn percent_text("),
    (CORE_OWNER, "pub fn precise_percent_text("),
    (SHELL_OWNER, "pub fn observed_percentage"),
    (SHELL_OWNER, "pub fn observed_precise_percentage"),
)

# A surface may not own a percent formatter.
LOCAL_FORMATTERS = (
    "percent_text",
    "precise_percent_text",
    "observed_percentage",
    "observed_precise_percentage",
)

# Macros that take a format string. A literal inside one of them that carries
# both a `%` sign and a placeholder spells a percentage locally.
FORMAT_MACROS = (
    "format",
    "format_args",
    "print",
    "println",
    "eprint",
    "eprintln",
    "write",
    "writeln",
    "panic",
    "assert",
    "assert_eq",
    "assert_ne",
    "debug_assert",
    "debug_assert_eq",
    "debug_assert_ne",
    "unreachable",
    "todo",
    "unimplemented",
)

MACRO_CALL = re.compile(r"\b([a-z_][a-z0-9_]*)\s*!\s*\(")
STRING_LITERAL = re.compile(r'"((?:[^"\\]|\\.)*)"')
PERCENT_PLACEHOLDER = re.compile(r"%")
FORMAT_PLACEHOLDER = re.compile(r"\{")
LOCAL_DEFINITION = re.compile(r"\bfn\s+([a-z_][a-z0-9_]*)")
SCALAR_PERCENT_CALL = re.compile(r"scalar_text\s*\([^)]*\"%\"")

# A format string that carries a `%` sign for a reason other than a percent
# unit. Each entry names the fact, why it is not a percent, and the trigger
# that makes the entry stale — a reviewed exception, not a blanket exemption.
ALLOWED_PERCENT_SIGN_SITES = (
    {
        "path": "taskforest-core/src/core/history.rs",
        "literal": '"%{byte:02X}"',
        "reason": "URL percent-ENCODING of one scope byte (`%2D`), not a "
        "percent unit: the history scope id escapes bytes so distinct "
        "identities cannot collide on case-insensitive filesystems.",
        "trigger": "The scope encoding changes -> re-review whether the entry "
        "still describes a non-percent `%`.",
    },
)

BLOCK_COMMENT = re.compile(r"/\*.*?\*/", re.DOTALL)
LINE_COMMENT = re.compile(r"//[^\n]*")


def mask_comments(text: str) -> str:
    """Replace Rust comments with spaces, preserving offsets and lines."""
    text = BLOCK_COMMENT.sub(lambda m: re.sub(r"[^\n]", " ", m.group(0)), text)
    return LINE_COMMENT.sub(lambda m: " " * len(m.group(0)), text)


def surface_sources() -> list[Path]:
    """Every production source file that is not one of the owner modules."""
    files: list[Path] = []
    for crate in sorted(CRATES.iterdir()):
        src = crate / "src"
        if not src.is_dir():
            continue
        for path in sorted(src.rglob("*.rs")):
            if path in OWNER_MODULES:
                continue
            files.append(path)
    return files


def local_percent_strings(source: str) -> list[tuple[int, str]]:
    """Return the percent-spelling format strings in one masked source."""
    masked = mask_comments(source)
    offenders: list[tuple[int, str]] = []
    for literal in STRING_LITERAL.finditer(masked):
        text = literal.group(1)
        if not PERCENT_PLACEHOLDER.search(text):
            continue
        if not FORMAT_PLACEHOLDER.search(text):
            continue
        # The enclosing statement decides whether this literal is a format
        # string: a dpkg query or a CSS snippet carries `%` and `{` too, but
        # neither is a Rust format string.
        start = max(
            masked.rfind(";", 0, literal.start()),
            masked.rfind("{", 0, literal.start()),
            masked.rfind("}", 0, literal.start()),
        )
        opening = masked[start : literal.start()]
        if not any(m.group(1) in FORMAT_MACROS for m in MACRO_CALL.finditer(opening)):
            continue
        line = masked.count("\n", 0, literal.start()) + 1
        offenders.append((line, literal.group(0)))
    return offenders


class PercentReadoutOwnerTests(unittest.TestCase):
    def test_owner_exports_one_spelling_per_precision(self):
        for path, symbol in EXPECTED_OWNER_SYMBOLS:
            with self.subTest(module=path.name, symbol=symbol):
                self.assertIn(symbol, path.read_text(encoding="utf-8"))

    def test_no_surface_spells_a_percentage_locally(self):
        offenders: list[str] = []
        for path in surface_sources():
            source = path.read_text(encoding="utf-8")
            for line, literal in local_percent_strings(source):
                if self._allowed(path, literal):
                    continue
                offenders.append(
                    f"{path.relative_to(CRATES).as_posix()}:{line}: {literal}"
                )
        self.assertEqual(
            offenders,
            [],
            "percent readouts must come from the owner "
            "(taskforest_core::core::percent / "
            "taskmanager_shell::presentation::percent):\n" + "\n".join(offenders),
        )

    def _allowed(self, path: Path, literal: str) -> bool:
        relative = path.relative_to(CRATES).as_posix()
        return any(
            entry["path"] == relative and entry["literal"] == literal
            for entry in ALLOWED_PERCENT_SIGN_SITES
        )

    def test_the_allowlist_stays_a_reviewed_entry(self):
        self.assertEqual(len(ALLOWED_PERCENT_SIGN_SITES), 1)
        for entry in ALLOWED_PERCENT_SIGN_SITES:
            with self.subTest(path=entry["path"]):
                self.assertTrue(entry["reason"].strip())
                self.assertTrue(entry["trigger"].strip())
                path = CRATES / entry["path"]
                self.assertTrue(path.is_file(), path)
                self.assertIn(entry["literal"], path.read_text(encoding="utf-8"))

    def test_no_surface_declares_a_local_percent_formatter(self):
        offenders: list[str] = []
        for path in surface_sources():
            source = mask_comments(path.read_text(encoding="utf-8"))
            for number, line in enumerate(source.splitlines(), start=1):
                for definition in LOCAL_DEFINITION.finditer(line):
                    if definition.group(1) in LOCAL_FORMATTERS:
                        offenders.append(
                            f"{path.relative_to(CRATES).as_posix()}:{number}: "
                            f"{line.strip()}"
                        )
        self.assertEqual(
            offenders,
            [],
            "a surface may not declare its own percent formatter:\n"
            + "\n".join(offenders),
        )

    def test_no_surface_spells_a_percent_through_the_generic_scalar(self):
        offenders: list[str] = []
        for path in surface_sources():
            source = mask_comments(path.read_text(encoding="utf-8"))
            for number, line in enumerate(source.splitlines(), start=1):
                if SCALAR_PERCENT_CALL.search(line):
                    offenders.append(
                        f"{path.relative_to(CRATES).as_posix()}:{number}: {line.strip()}"
                    )
        self.assertEqual(
            offenders,
            [],
            "the generic suffixed scalar must not spell a percent:\n"
            + "\n".join(offenders),
        )

    def test_a_fork_mutation_fails(self):
        source = (CRATES / "taskmanager-iced/src/ui/fan.rs").read_text(encoding="utf-8")
        forked = source.replace(
            't("fan.pwm")',
            't("fan.pwm")',
        )
        mutated = (
            forked.replace(
                "Some(percent_text(pwm))",
                'Some(format!("{pwm:.0}%"))',
            )
        )
        self.assertNotEqual(forked, mutated)
        self.assertTrue(local_percent_strings(mutated))


if __name__ == "__main__":
    unittest.main(verbosity=2)
