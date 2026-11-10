#!/usr/bin/env python3
"""One dashboard metric count, one seam, one owner.

The dashboard shows the product's timeline metrics. WHICH metrics exist is a
semantic fact owned by the application layer (`TimelineMetric::ALL`); HOW MANY
metric cards a shape may draw, and WHERE a paged window may start, is a
presentation fact owned by the shared contract
(`taskmanager_ui_contract::presentation::dashboard`).

The four shapes re-derived that count before the seam existed: GPUI carried a
`MAX_METRIC_GROUPS = 4`, Iced a `MAX_CARDS = 4`, Bevy a `.min(4)` cap and a
`.min(3)` paging bound. A count restated in a frontend is a second authority:
it drifts the moment the product defines a new metric, and it drifts the
moment a shape pages by a different step.

This check fails that shape:

* a frontend that declares its own metric-card cap constant;
* a frontend that derives the count beside the seam (`ALL.len()`);
* a frontend that caps a metric count with a bare number;
* a frontend that renders the metric window without consuming the seam.

The seam module itself is exempt: it is the owner.
"""

from __future__ import annotations

import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CRATES = ROOT / "crates"
LOCAL_GATES = ROOT / "scripts/quality/local-gates.sh"

# The four graphical/text frontends that render the dashboard metric window.
FRONTEND_CRATES = (
    "taskmanager-gpui",
    "taskmanager-iced",
    "taskmanager-bevy-ui",
    "taskmanager-tui",
)

# The owner: the shared presentation seam for the dashboard metric window.
SEAM_RELATIVE = "taskmanager-ui-contract/src/presentation/dashboard.rs"
SEAM_MODULE = CRATES / SEAM_RELATIVE

# The seam's published surface. A shape spends the verdict through these.
EXPECTED_OWNER_SYMBOLS = (
    "pub const DASHBOARD_METRIC_COUNT",
    "pub fn max_metric_groups",
    "pub struct DashboardMetricWindow",
    "pub enum MetricPaging",
)

# The seam's import path, spelled the way a frontend spells it.
SEAM_IMPORT = "presentation::dashboard"

BLOCK_COMMENT = re.compile(r"/\*.*?\*/", re.DOTALL)
LINE_COMMENT = re.compile(r"//[^\n]*")
STRING_LITERAL = re.compile(r'"((?:[^"\\]|\\.)*)"')

# A local cap constant: `const MAX_CARDS: usize = 4;` and its neighbours.
CAP_CONSTANT = re.compile(
    r"\b(?:const|static)\s+([A-Z][A-Z0-9_]*)\s*:\s*usize\s*=\s*(\d+)\s*;",
)
# The name must NAME a metric/dashboard card cap, not any integer.
CAP_NAME = re.compile(
    r"MAX_(?:METRICS?|CARDS?|DASHBOARD_CARDS?)|"
    r"(?:METRICS?|CARDS?|DASHBOARD_CARDS?)_(?:COUNT|CAP|MAX|LIMIT)",
    re.IGNORECASE,
)

# The count derived beside the seam.
COUNT_DERIVATION = re.compile(r"TimelineMetric::ALL\s*\.\s*len\s*\(\s*\)")

# A bare number capping a count-shaped value.
LITERAL_CAP = re.compile(r"\.\s*(min|max|clamp)\s*\(([^)]*)\)")
COUNT_WORD = re.compile(r"count|cards?|metrics?", re.IGNORECASE)


def mask_comments(text: str) -> str:
    """Replace Rust comments with spaces, preserving offsets and lines."""
    text = BLOCK_COMMENT.sub(lambda m: re.sub(r"[^\n]", " ", m.group(0)), text)
    return LINE_COMMENT.sub(lambda m: " " * len(m.group(0)), text)


def code_only(source: str) -> str:
    """Comments and string literals removed, line and column offsets kept."""
    masked = STRING_LITERAL.sub(lambda m: re.sub(r"[^\n]", " ", m.group(0)), mask_comments(source))
    return masked


def frontend_sources() -> list[Path]:
    """Every production source file of the four frontends."""
    files: list[Path] = []
    for crate in FRONTEND_CRATES:
        src = CRATES / crate / "src"
        if not src.is_dir():
            continue
        files.extend(sorted(src.rglob("*.rs")))
    return files


def in_scope(path: Path, masked: str) -> bool:
    """Whether a file belongs to the dashboard metric family."""
    if "dashboard" in path.as_posix().lower():
        return True
    if "TimelineMetric" in masked:
        return True
    return SEAM_IMPORT in masked


def bare_number_cap_sites(masked: str) -> list[str]:
    """A bare number that caps a COUNT, not a measurement in another domain.

    A line is in the count family only when the clamped value names a count
    (`count`, `cards`, `metrics`, `rows` of the metric window) AND the number
    is not a measurement scale (`sample_count.max(2)` keeps a chart readable;
    `clamp(0.0, ceiling)` keeps an alert threshold inside its domain).
    """
    offenders: list[str] = []
    for number, line in enumerate(masked.splitlines(), start=1):
        for match in LITERAL_CAP.finditer(line):
            arguments = match.group(2)
            if not re.search(r"\b\d+\b", arguments):
                continue
            before = line[: match.start()]
            if not COUNT_WORD.search(before):
                continue
            # A number bound on a measurement scale is not a metric count:
            # those lines name a height, a chart sample or a threshold.
            if re.search(
                r"\b(?:height|width|threshold|chart|ratio|percent|volume|level)\b|(?:sample_?count)\b",
                before,
                re.IGNORECASE,
            ):
                continue
            # A bound that already names a shared constant is not bare.
            if "DASHBOARD_METRIC_COUNT" in arguments:
                continue
            offenders.append(f"{number}: {line.strip()}")
    return offenders


class DashboardMetricOwnerTests(unittest.TestCase):
    def test_the_seam_publishes_its_surface(self):
        self.assertTrue(SEAM_MODULE.is_file(), SEAM_MODULE)
        source = SEAM_MODULE.read_text(encoding="utf-8")
        for symbol in EXPECTED_OWNER_SYMBOLS:
            with self.subTest(symbol=symbol):
                self.assertIn(symbol, source)

    def test_the_quick_gate_runs_this_check(self):
        """An unwired owner test is a dead test, not evidence."""
        self.assertTrue(LOCAL_GATES.is_file(), LOCAL_GATES)
        source = LOCAL_GATES.read_text(encoding="utf-8")
        self.assertIn(
            "scripts/quality/test_dashboard_metric_owner.py",
            source,
            "the dashboard-metric owner test must be wired into "
            "local-gates.sh quick",
        )

    def test_no_frontend_declares_its_own_metric_cap(self):
        offenders: list[str] = []
        for path in frontend_sources():
            masked = code_only(path.read_text(encoding="utf-8"))
            if not in_scope(path, masked):
                continue
            for number, line in enumerate(masked.splitlines(), start=1):
                for match in CAP_CONSTANT.finditer(line):
                    name = match.group(1)
                    if not CAP_NAME.search(name):
                        continue
                    offenders.append(
                        f"{path.relative_to(CRATES).as_posix()}:{number}: {name}"
                    )
        self.assertEqual(
            offenders,
            [],
            "the dashboard metric cap must come from the shared seam "
            f"({SEAM_RELATIVE}):\n" + "\n".join(offenders),
        )

    def test_no_frontend_derives_the_count_beside_the_seam(self):
        offenders: list[str] = []
        for path in frontend_sources():
            masked = code_only(path.read_text(encoding="utf-8"))
            for number, line in enumerate(masked.splitlines(), start=1):
                if COUNT_DERIVATION.search(line):
                    offenders.append(
                        f"{path.relative_to(CRATES).as_posix()}:{number}: "
                        f"{line.strip()}"
                    )
        self.assertEqual(
            offenders,
            [],
            "the metric count has one seam; a frontend may not count the "
            "metric set beside it:\n" + "\n".join(offenders),
        )

    def test_no_frontend_caps_a_metric_count_with_a_bare_number(self):
        offenders: list[str] = []
        for path in frontend_sources():
            masked = code_only(path.read_text(encoding="utf-8"))
            if not in_scope(path, masked):
                continue
            prefix = path.relative_to(CRATES).as_posix()
            offenders.extend(
                f"{prefix}:{site}" for site in bare_number_cap_sites(masked)
            )
        self.assertEqual(
            offenders,
            [],
            "a metric count is capped by the shared seam, never by a bare "
            "number:\n" + "\n".join(offenders),
        )

    def test_every_frontend_that_shows_metrics_consumes_the_seam(self):
        offenders: list[str] = []
        for crate in FRONTEND_CRATES:
            src = CRATES / crate / "src"
            if not src.is_dir():
                continue
            mentions = any(
                "TimelineMetric" in path.read_text(encoding="utf-8")
                for path in src.rglob("*.rs")
            )
            if not mentions:
                continue
            consumes = any(
                SEAM_IMPORT in path.read_text(encoding="utf-8")
                for path in src.rglob("*.rs")
            )
            if not consumes:
                offenders.append(crate)
        self.assertEqual(
            offenders,
            [],
            "a frontend that renders the metric window must spend the shared "
            f"seam ({SEAM_IMPORT}):\n" + "\n".join(offenders),
        )

    def test_the_detector_bites_a_forked_cap(self):
        source = "const MAX_CARDS: usize = 4;\n"
        found = [
            match.group(1)
            for match in CAP_CONSTANT.finditer(code_only(source))
            if CAP_NAME.search(match.group(1))
        ]
        self.assertEqual(found, ["MAX_CARDS"])

    def test_the_detector_bites_a_bare_cap(self):
        source = "let count = (rows * columns).min(4);\n"
        self.assertEqual(len(bare_number_cap_sites(code_only(source))), 1)

    def test_the_detector_leaves_other_domains_alone(self):
        for shape in (
            "let rows = visible.min(4);",
            "let count = admitted.cards().min(DASHBOARD_METRIC_COUNT);",
            "let height = box.clamp(1.0, 4.0);",
            "let sample_count = sample_count.max(2);",
            "rule.threshold = rule.threshold.clamp(0.0, 4.0);",
            "// a comment that mentions count.min(4)",
            'let label = "count.min(4)";',
        ):
            with self.subTest(shape=shape):
                self.assertEqual(bare_number_cap_sites(code_only(shape)), [], shape)


if __name__ == "__main__":
    unittest.main(verbosity=2)
