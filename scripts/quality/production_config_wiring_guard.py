#!/usr/bin/env python3
"""Fail-closed wiring guard: two hosts must keep enforcing the production-config gate.

``scripts/quality/production-config-check.sh`` closes a real blind spot: every
clippy stage and every nextest layer compiles with the dev-only ``test-support``
feature, so frontend code that only compiles when that feature is OFF (typically
a ``#[cfg(any(test, feature = "test-support"))]`` debug selector) had no local
gate at all. W19-A brought the check home, W20-A wired it into CI, W21-A made its
external deadline portable, and W24-C documented the rule.

Those rounds only proved the gate was wired *at that time*. A later refactor can
delete the CI step, move it to another job, mark it ``continue-on-error``,
comment it out, or drop the local stage — and every remaining signal stays
green: the helper script still exists and still passes when someone runs it by
hand. This guard turns "wired once" into a mechanical invariant over the two
hosts that must keep enforcing it:

``scripts/quality/local-gates.sh`` (standard tier)
    the stage named ``production-config`` must contain exactly one live
    ``run_stage production-config standard ... production-config-check.sh``
    invocation, placed after the quick-tier dispatch marker so the standard
    tier stays the tier that runs it.
``.github/workflows/ci.yml`` (``lint`` job)
    exactly one live ``run:`` step must invoke
    ``bash scripts/quality/production-config-check.sh`` and must not be
    neutralised with ``continue-on-error``.

The question this guard answers is "is the gate still enforced", not "is the
command text identical": flag-for-flag comparison belongs to the helper
script's own contract and to ``clippy_command_parity_guard.py``. Structural
drift is a finding (exit 1); anything the guard cannot statically locate — a
missing file, no ``lint`` job, an ambiguous second invocation, a missing stage
block — is a parse failure (exit 2), so a reformat can never be mistaken for a
passing gate.

Exit codes: 0 both hosts enforce the gate, 1 a host no longer does, 2 parse
failure. ``--json`` emits the same verdict machine-readably.

Usage::

    python3 scripts/quality/production_config_wiring_guard.py
    python3 scripts/quality/production_config_wiring_guard.py --json
    python3 scripts/quality/production_config_wiring_guard.py --self-test
"""

from __future__ import annotations

import argparse
import json
import re
import shlex
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

CI_WORKFLOW = ".github/workflows/ci.yml"
LOCAL_GATES = "scripts/quality/local-gates.sh"
GATE_SCRIPT = "scripts/quality/production-config-check.sh"
GATE_BASENAME = "production-config-check.sh"
CI_JOB = "lint"
LOCAL_STAGE = "production-config"
LOCAL_TIER = "standard"

CI_SITE = f"{CI_WORKFLOW} (job {CI_JOB})"
LOCAL_SITE = f"{LOCAL_GATES} ({LOCAL_TIER} tier, stage {LOCAL_STAGE})"

LINE_CONTINUATION = re.compile(r"\\\n[ \t]*")
BLOCK_SCALAR = re.compile(r"^[|>][+-]?[0-9]?$")
MAPPING_KEY = re.compile(
    r"^(?P<indent>[ \t]*)(?P<key>[A-Za-z0-9_.-]+):(?:[ \t]*(?P<value>.*?))?[ \t]*$"
)
LIST_ITEM = re.compile(r"^(?P<indent>[ \t]*)-[ \t]+(?P<rest>.*?)[ \t]*$")
COMMENT_ONLY = re.compile(r"^[ \t]*#")
JOBS_SECTION = re.compile(r"^jobs:[ \t]*(?:#.*)?$")
QUICK_DISPATCH = re.compile(r'^\s*\[\[\s*"\$tier"\s*==\s*"quick"\s*\]\]\s*&&\s*exit\b')
LOCAL_STAGE_OPEN = re.compile(rf"^if maybe {LOCAL_STAGE}; then\s*$")

# A `run_stage` invocation is the only shape that records the stage in
# results.tsv and increments the failure counter; a direct call would bypass it.
LOCAL_CODE_DETAIL = {
    "missing": (
        f"no live invocation of {GATE_BASENAME} anywhere in the file: the local gate no "
        "longer enforces the production configuration"
    ),
    "empty": "an empty command was scanned",
    "incomplete": "the run_stage invocation is incomplete",
    "not_run_stage": (
        "the helper is invoked without run_stage, so its result reaches neither results.tsv "
        "nor the failure counter"
    ),
    "wrong_stage": f"the helper is routed through a stage other than `{LOCAL_STAGE}`",
    "wrong_tier": f"the helper is routed through tier `{{tier}}`, not `{LOCAL_TIER}`",
    "swallowed": "the invocation is combined with `||`/`&`, so a failure no longer fails the stage",
    "outside_stage": (
        f"the invocation sits outside the `if maybe {LOCAL_STAGE}; then` stage block, so "
        "`--only` selection and failure attribution no longer cover it"
    ),
    "before_quick_dispatch": (
        "the invocation sits before the quick-tier dispatch, so the guard cannot establish "
        f"that the {LOCAL_TIER} tier is the one running it"
    ),
}


class ParseError(Exception):
    """The guard cannot statically locate or compare one of the wiring sites."""


# ---------------------------------------------------------------------------
# Verdict model
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class WiringSite:
    """One host that must invoke the production-config helper."""

    label: str
    site: str
    ok: bool
    location: str
    command: str
    code: str
    detail: str
    notes: tuple[str, ...] = ()

    def as_dict(self) -> dict[str, object]:
        return {
            "label": self.label,
            "site": self.site,
            "ok": self.ok,
            "location": self.location,
            "command": self.command,
            "code": self.code,
            "detail": self.detail,
            "notes": list(self.notes),
        }


@dataclass(frozen=True)
class Report:
    local: WiringSite
    ci: WiringSite

    def sites(self) -> tuple[WiringSite, WiringSite]:
        return (self.local, self.ci)

    def failures(self) -> list[WiringSite]:
        return [site for site in self.sites() if not site.ok]


def verdict(report: Report) -> int:
    """0 = both hosts enforce the gate, 1 = at least one no longer does."""
    return 1 if report.failures() else 0


# ---------------------------------------------------------------------------
# Shared text helpers
# ---------------------------------------------------------------------------


def strip_yaml_inline_comment(value: str) -> str:
    """Drop a YAML ``#`` comment from a scalar, respecting quotes."""
    quote: str | None = None
    for index, char in enumerate(value):
        if quote is not None:
            if char == quote:
                quote = None
        elif char in ("'", '"'):
            quote = char
        elif char == "#" and (index == 0 or value[index - 1] in " \t"):
            return value[:index].rstrip()
    return value


def unquote_yaml_scalar(value: str) -> str:
    value = value.strip()
    if len(value) >= 2 and value[0] == value[-1] and value[0] in ("'", '"'):
        return value[1:-1]
    return value


def tokenize_shell(text: str) -> list[str]:
    """Tokenize one logical shell command, joining backslash continuations."""
    joined = LINE_CONTINUATION.sub(" ", text)
    try:
        return shlex.split(joined)
    except ValueError as error:
        raise ParseError(f"cannot tokenize shell command ({error}): {text!r}") from error


def single_line(command: str) -> str:
    return " ".join(command.split())


def indentation(line: str) -> int:
    return len(line) - len(line.lstrip(" "))


def invokes_gate(tokens: list[str]) -> bool:
    """True when a token names the helper (path, path-prefixed, or bare name)."""
    return any(
        token in (GATE_SCRIPT, GATE_BASENAME) or token.endswith("/" + GATE_SCRIPT)
        for token in tokens
    )


# ---------------------------------------------------------------------------
# Minimal YAML reader (stdlib only, fail-closed)
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class JobBlock:
    key: str
    line: int
    indent: int
    start: int
    end: int


@dataclass(frozen=True)
class Step:
    line: int
    name: str | None
    run: str | None
    continue_on_error: str | None


def yaml_job_blocks(lines: list[str]) -> list[JobBlock]:
    """Return every top-level job as a line range, without a YAML dependency."""
    jobs_index: int | None = None
    for index, line in enumerate(lines):
        if JOBS_SECTION.match(line):
            jobs_index = index
            break
    if jobs_index is None:
        raise ParseError(f"{CI_WORKFLOW}: no top-level `jobs:` section")

    entries: list[tuple[str, int, int, int]] = []
    job_indent: int | None = None
    jobs_end = len(lines)
    for index in range(jobs_index + 1, len(lines)):
        line = lines[index]
        if not line.strip() or COMMENT_ONLY.match(line):
            continue
        indent = indentation(line)
        if indent == 0:
            jobs_end = index
            break
        if job_indent is None:
            job_indent = indent
        if indent < job_indent:
            raise ParseError(f"{CI_WORKFLOW}:{index + 1}: unexpected dedent under `jobs:`")
        if indent != job_indent:
            continue
        match = MAPPING_KEY.match(line)
        if match is None:
            raise ParseError(f"{CI_WORKFLOW}:{index + 1}: job key is not a mapping")
        entries.append((match.group("key"), index + 1, indent, index))

    if not entries:
        raise ParseError(f"{CI_WORKFLOW}: `jobs:` declares no job")
    blocks: list[JobBlock] = []
    for position, (key, line_number, indent, start) in enumerate(entries):
        end = entries[position + 1][3] if position + 1 < len(entries) else jobs_end
        blocks.append(JobBlock(key=key, line=line_number, indent=indent, start=start, end=end))
    return blocks


def find_job(blocks: list[JobBlock], key: str) -> JobBlock:
    matches = [block for block in blocks if block.key == key]
    if not matches:
        declared = ", ".join(block.key for block in blocks)
        raise ParseError(f"{CI_WORKFLOW}: no job named '{key}' (jobs: {declared})")
    if len(matches) > 1:
        raise ParseError(f"{CI_WORKFLOW}: `{key}` is declared {len(matches)} times")
    return matches[0]


def resolve_scalar(lines: list[str], index: int, value: str, field_indent: int) -> str:
    """Resolve an inline scalar or a ``|``/``>`` block scalar at ``index``."""
    value = strip_yaml_inline_comment(value)
    if not BLOCK_SCALAR.match(value.strip()):
        return unquote_yaml_scalar(value)
    body: list[str] = []
    for cursor in range(index + 1, len(lines)):
        line = lines[cursor]
        if not line.strip():
            body.append("")
            continue
        if indentation(line) <= field_indent:
            break
        body.append(line[indentation(line) :])
    return "\n".join(body).strip("\n")


def parse_step(lines: list[str], start: int, end: int, item_indent: int) -> Step:
    """Parse the top-level fields of one step list item."""
    dash = LIST_ITEM.match(lines[start])
    if dash is None:
        raise ParseError(f"{CI_WORKFLOW}:{start + 1}: malformed step item")

    candidates: list[tuple[int, int, str, str]] = []
    rest = dash.group("rest")
    if MAPPING_KEY.match(" " * (item_indent + 2) + rest):
        key, _, remainder = rest.partition(":")
        candidates.append((item_indent + 2, start, key, remainder))
    for index in range(start + 1, end):
        line = lines[index]
        if not line.strip() or COMMENT_ONLY.match(line):
            continue
        match = MAPPING_KEY.match(line)
        if match is not None:
            candidates.append(
                (indentation(line), index, match.group("key"), match.group("value") or "")
            )
    if not candidates:
        return Step(line=start + 1, name=None, run=None, continue_on_error=None)

    field_indent = min(indent for indent, _, _, _ in candidates)
    fields = [
        (index, key, value)
        for indent, index, key, value in candidates
        if indent == field_indent
    ]
    name: str | None = None
    run: str | None = None
    continue_on_error: str | None = None
    for index, key, value in fields:
        resolved = resolve_scalar(lines, index, value, field_indent)
        if key == "name":
            name = resolved
        elif key == "run":
            run = resolved
        elif key == "continue-on-error":
            continue_on_error = resolved
    return Step(line=start + 1, name=name, run=run, continue_on_error=continue_on_error)


def parse_job_steps(lines: list[str], job: JobBlock) -> list[Step]:
    """Return the steps of one job, in file order."""
    steps_index: int | None = None
    steps_indent = 0
    for index in range(job.start, job.end):
        match = MAPPING_KEY.match(lines[index])
        if match is None:
            continue
        indent = indentation(lines[index])
        if match.group("key") == "steps" and indent > job.indent:
            steps_index = index
            steps_indent = indent
            break
    if steps_index is None:
        raise ParseError(f"{CI_WORKFLOW}:{job.line}: job '{job.key}' has no `steps:` key")

    item_indent: int | None = None
    starts: list[int] = []
    list_end = job.end
    for index in range(steps_index + 1, job.end):
        line = lines[index]
        if not line.strip() or COMMENT_ONLY.match(line):
            continue
        match = LIST_ITEM.match(line)
        if match is not None and indentation(line) > steps_indent:
            indent = indentation(line)
            if item_indent is None:
                item_indent = indent
            if indent == item_indent:
                starts.append(index)
            continue
        if item_indent is None:
            if indentation(line) <= steps_indent:
                list_end = index
                break
            continue
        if indentation(line) <= item_indent:
            list_end = index
            break
    if not starts:
        return []
    steps: list[Step] = []
    for position, start in enumerate(starts):
        end = starts[position + 1] if position + 1 < len(starts) else list_end
        steps.append(parse_step(lines, start, end, item_indent or 0))
    return steps


def commented_references(lines: list[str], start: int, end: int) -> list[int]:
    """1-based line numbers where comment-only text names the helper."""
    return [
        index + 1
        for index in range(start, end)
        if COMMENT_ONLY.match(lines[index]) and GATE_BASENAME in lines[index]
    ]


# ---------------------------------------------------------------------------
# CI wiring site
# ---------------------------------------------------------------------------


def _ci_site(
    ok: bool,
    location: str,
    command: str,
    code: str,
    detail: str,
    notes: tuple[str, ...],
) -> WiringSite:
    return WiringSite(
        label="ci",
        site=CI_SITE,
        ok=ok,
        location=location,
        command=single_line(command),
        code=code,
        detail=detail,
        notes=notes,
    )


def parse_ci_wiring(text: str) -> WiringSite:
    """Check the one canonical production-config step of the CI ``lint`` job."""
    lines = text.splitlines()
    job = find_job(yaml_job_blocks(lines), CI_JOB)
    steps = parse_job_steps(lines, job)
    notes = tuple(
        f"{CI_WORKFLOW}:{number} names the helper only in a comment"
        for number in commented_references(lines, job.start, job.end)
    )

    matched: list[tuple[Step, list[str]]] = []
    for step in steps:
        if step.run is None:
            continue
        tokens = tokenize_shell(step.run)
        if invokes_gate(tokens):
            matched.append((step, tokens))
    if len(matched) > 1:
        locations = ", ".join(f"line {step.line}" for step, _ in matched)
        raise ParseError(
            f"{CI_WORKFLOW}: job '{CI_JOB}' has {len(matched)} steps invoking "
            f"{GATE_SCRIPT} ({locations}); the guard pins exactly one canonical step"
        )
    if not matched:
        return _ci_site(
            ok=False,
            location=f"{CI_WORKFLOW} (job {CI_JOB})",
            command="",
            code="missing",
            detail=(
                f"job '{CI_JOB}' has no live `run:` step invoking {GATE_SCRIPT}; the CI "
                "prod-config companion no longer enforces the feature-off configuration"
            ),
            notes=notes,
        )

    step, tokens = matched[0]
    location = f"{CI_WORKFLOW}:{step.line} (job {CI_JOB})"
    if step.continue_on_error is not None:
        raw = unquote_yaml_scalar(step.continue_on_error).lower()
        if raw != "false":
            return _ci_site(
                ok=False,
                location=location,
                command=step.run or "",
                code="continue_on_error",
                detail=(
                    f"the step is marked `continue-on-error: {step.continue_on_error.strip()}`; "
                    "a failing product configuration would no longer fail the lint job"
                ),
                notes=notes,
            )
    if "||" in tokens or "&" in tokens:
        return _ci_site(
            ok=False,
            location=location,
            command=step.run or "",
            code="swallowed",
            detail="the command is combined with `||`/`&`, so a failure no longer fails the step",
            notes=notes,
        )
    return _ci_site(
        ok=True,
        location=location,
        command=step.run or "",
        code="ok",
        detail="",
        notes=notes,
    )


# ---------------------------------------------------------------------------
# Local-gates wiring site
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class LocalInvocation:
    line: int
    command: str
    code: str


def logical_command_at(lines: list[str], index: int) -> tuple[int, str]:
    """Return ``(start_index, text)`` for the command spanning ``index``."""
    start = index
    for candidate in range(index - 1, -1, -1):
        if not lines[candidate].rstrip().endswith("\\"):
            break
        start = candidate
    end = index
    for candidate in range(index, len(lines) - 1):
        if not lines[candidate].rstrip().endswith("\\"):
            break
        end = candidate + 1
    return start, "\n".join(lines[start : end + 1])


def classify_local_invocation(tokens: list[str]) -> str:
    """Classify a live invocation by the shape run_stage is allowed to take."""
    if not tokens:
        return "empty"
    if tokens[0] != "run_stage":
        return "not_run_stage"
    if len(tokens) < 3:
        return "incomplete"
    if tokens[1] != LOCAL_STAGE:
        return "wrong_stage"
    if tokens[2] != LOCAL_TIER:
        return "wrong_tier"
    if "||" in tokens or "&" in tokens:
        return "swallowed"
    return "ok"


def local_invocations(lines: list[str]) -> list[LocalInvocation]:
    """Every live command line that names the helper, in file order."""
    seen: set[int] = set()
    invocations: list[LocalInvocation] = []
    for index, line in enumerate(lines):
        if GATE_BASENAME not in line or COMMENT_ONLY.match(line):
            continue
        start, command = logical_command_at(lines, index)
        if start in seen:
            continue
        seen.add(start)
        tokens = tokenize_shell(command)
        if not invokes_gate(tokens):
            continue
        invocations.append(
            LocalInvocation(
                line=start + 1,
                command=command,
                code=classify_local_invocation(tokens),
            )
        )
    return invocations


def _local_site(
    ok: bool,
    location: str,
    command: str,
    code: str,
    detail: str,
    notes: tuple[str, ...],
) -> WiringSite:
    return WiringSite(
        label="local",
        site=LOCAL_SITE,
        ok=ok,
        location=location,
        command=single_line(command),
        code=code,
        detail=detail,
        notes=notes,
    )


def parse_local_wiring(text: str) -> WiringSite:
    """Check the standard-tier ``production-config`` stage of local-gates.sh."""
    lines = text.splitlines()
    quick_index: int | None = None
    for index, line in enumerate(lines):
        if QUICK_DISPATCH.match(line):
            quick_index = index
            break
    if quick_index is None:
        raise ParseError(
            f"{LOCAL_GATES}: quick-tier dispatch marker "
            '(`[[ "$tier" == "quick" ]] && exit`) not found'
        )
    open_index: int | None = None
    for index, line in enumerate(lines):
        if LOCAL_STAGE_OPEN.match(line):
            open_index = index
            break
    if open_index is None:
        raise ParseError(f"{LOCAL_GATES}: stage `if maybe {LOCAL_STAGE}; then` not found")
    close_index: int | None = None
    for index in range(open_index + 1, len(lines)):
        if lines[index] == "fi":
            close_index = index
            break
    if close_index is None:
        raise ParseError(f"{LOCAL_GATES}: stage `if maybe {LOCAL_STAGE}; then` has no closing `fi`")
    if open_index < quick_index:
        raise ParseError(
            f"{LOCAL_GATES}: the {LOCAL_STAGE} stage sits before the quick-tier dispatch; "
            "the guard cannot establish which tier runs it"
        )

    notes = tuple(
        f"{LOCAL_GATES}:{number} names the helper only in a comment"
        for number in commented_references(lines, 0, len(lines))
    )
    invocations = local_invocations(lines)
    if not invocations:
        return _local_site(
            ok=False,
            location=f"{LOCAL_GATES} (standard tier)",
            command="",
            code="missing",
            detail=LOCAL_CODE_DETAIL["missing"],
            notes=notes,
        )
    qualifying = [item for item in invocations if item.code == "ok"]
    if len(qualifying) > 1:
        locations = ", ".join(f"line {item.line}" for item in qualifying)
        raise ParseError(
            f"{LOCAL_GATES}: {len(qualifying)} live run_stage invocations of {GATE_SCRIPT} "
            f"({locations}); the guard pins exactly one"
        )
    if not qualifying:
        item = invocations[0]
        detail = LOCAL_CODE_DETAIL.get(item.code, item.code)
        if item.code == "wrong_tier":
            tokens = tokenize_shell(item.command)
            detail = LOCAL_CODE_DETAIL["wrong_tier"].format(tier=tokens[2])
        return _local_site(
            ok=False,
            location=f"{LOCAL_GATES}:{item.line}",
            command=item.command,
            code=item.code,
            detail=detail,
            notes=notes,
        )

    item = qualifying[0]
    index = item.line - 1
    if index < quick_index:
        return _local_site(
            ok=False,
            location=f"{LOCAL_GATES}:{item.line}",
            command=item.command,
            code="before_quick_dispatch",
            detail=LOCAL_CODE_DETAIL["before_quick_dispatch"],
            notes=notes,
        )
    if not open_index < index < close_index:
        return _local_site(
            ok=False,
            location=f"{LOCAL_GATES}:{item.line}",
            command=item.command,
            code="outside_stage",
            detail=LOCAL_CODE_DETAIL["outside_stage"],
            notes=notes,
        )
    return _local_site(
        ok=True,
        location=f"{LOCAL_GATES}:{item.line}",
        command=item.command,
        code="ok",
        detail="",
        notes=notes,
    )


# ---------------------------------------------------------------------------
# Collection and output
# ---------------------------------------------------------------------------


def collect(repository: Path) -> Report:
    """Read the three files the wiring contract spans."""
    sources = {
        "ci workflow": repository / CI_WORKFLOW,
        "local gate": repository / LOCAL_GATES,
        "helper script": repository / GATE_SCRIPT,
    }
    for label, path in sources.items():
        if not path.is_file():
            raise ParseError(f"missing required file ({label}): {path}")
    return Report(
        local=parse_local_wiring(sources["local gate"].read_text(encoding="utf-8")),
        ci=parse_ci_wiring(sources["ci workflow"].read_text(encoding="utf-8")),
    )


def payload(report: Report | None, error: str | None = None) -> dict[str, object]:
    if error is not None:
        status = "parse_error"
    elif report is not None and report.failures():
        status = "fail"
    else:
        status = "pass"
    failures = report.failures() if report is not None else []
    return {
        "status": status,
        "sites": [site.as_dict() for site in (report.sites() if report is not None else ())],
        "differences": [
            {
                "site": site.site,
                "label": site.label,
                "code": site.code,
                "location": site.location,
                "detail": site.detail,
            }
            for site in failures
        ],
        "error": error,
    }


# ---------------------------------------------------------------------------
# Self-test
# ---------------------------------------------------------------------------

SYNTHETIC_CI_OK = "\n".join(
    [
        "jobs:",
        "  policy:",
        "    steps:",
        "      - run: timeout --kill-after=10s 300s bash scripts/quality/local-gates.sh quick",
        "  lint:",
        "    steps:",
        "      - uses: actions/checkout@v7",
        "      - name: clippy",
        "        run: cargo clippy --locked --workspace --all-targets "
        "--features test-support -- -D warnings",
        "      # Production-config companion to the clippy step above.",
        "      - name: production-config",
        "        run: >-",
        "          timeout --kill-after=30s 1800s bash",
        "          scripts/quality/production-config-check.sh",
        "  docs:",
        "    steps:",
        "      # - run: bash scripts/quality/production-config-check.sh",
        '      - run: echo "scripts/quality/production-config-check.sh is documented"',
    ]
)

SYNTHETIC_CI_COMMENTED_OUT = "\n".join(
    [
        "jobs:",
        "  lint:",
        "    steps:",
        "      - run: cargo clippy --locked --workspace -- -D warnings",
        "      # - run: timeout --kill-after=30s 1800s bash "
        "scripts/quality/production-config-check.sh",
    ]
)

SYNTHETIC_CI_CONTINUE_ON_ERROR = "\n".join(
    [
        "jobs:",
        "  lint:",
        "    steps:",
        "      - run: cargo clippy --locked --workspace -- -D warnings",
        "      - continue-on-error: true",
        "        run: timeout --kill-after=30s 1800s bash "
        "scripts/quality/production-config-check.sh",
    ]
)

SYNTHETIC_CI_FALSE_CONTINUE_ON_ERROR = "\n".join(
    [
        "jobs:",
        "  lint:",
        "    steps:",
        "      - continue-on-error: false",
        "        run: ./scripts/quality/production-config-check.sh",
    ]
)

SYNTHETIC_CI_NO_LINT_JOB = "\n".join(
    [
        "jobs:",
        "  policy:",
        "    steps:",
        "      - run: bash scripts/quality/local-gates.sh quick",
    ]
)

SYNTHETIC_CI_NO_JOBS = "\n".join(["name: CI", "on: [push]"])

SYNTHETIC_CI_TWO_INVOCATIONS = "\n".join(
    [
        "jobs:",
        "  lint:",
        "    steps:",
        "      - run: bash scripts/quality/production-config-check.sh",
        "      - run: timeout 60 bash scripts/quality/production-config-check.sh gpui",
    ]
)

SYNTHETIC_CI_LINT_WITHOUT_STEPS = "\n".join(
    ["jobs:", "  lint:", "    runs-on: ubuntu-latest"]
)

SYNTHETIC_LOCAL_OK = "\n".join(
    [
        '[[ "$tier" == "quick" ]] && exit "$((failures > 0))"',
        "",
        "# ---- standard ----",
        "if maybe production-config; then",
        "    production_config_frontends=(gpui iced tui bevy)",
        '    case "$scope" in',
        "    all) ;;",
        '    gpui | iced | tui | bevy) production_config_frontends=("$scope") ;;',
        "    core)",
        "        production_config_frontends=()",
        '        echo "SKIP production-config (scope=core: no frontend product crate)"',
        "        record production-config standard SKIP 0",
        "        ;;",
        "    esac",
        "    if [[ ${#production_config_frontends[@]} -gt 0 ]]; then",
        "        run_stage production-config standard bash "
        "scripts/quality/production-config-check.sh \\",
        '            "${production_config_frontends[@]}"',
        "    fi",
        "fi",
    ]
)

SYNTHETIC_LOCAL_COMMENTED_OUT = "\n".join(
    [
        '[[ "$tier" == "quick" ]] && exit "$((failures > 0))"',
        "if maybe production-config; then",
        "    # run_stage production-config standard bash "
        "scripts/quality/production-config-check.sh \\",
        '    #     "${production_config_frontends[@]}"',
        "fi",
    ]
)

SYNTHETIC_LOCAL_WRONG_TIER = "\n".join(
    [
        '[[ "$tier" == "quick" ]] && exit "$((failures > 0))"',
        "if maybe production-config; then",
        "    run_stage production-config quick bash production-config-check.sh",
        "fi",
    ]
)

SYNTHETIC_LOCAL_DIRECT_CALL = "\n".join(
    [
        '[[ "$tier" == "quick" ]] && exit "$((failures > 0))"',
        "if maybe production-config; then",
        "    bash scripts/quality/production-config-check.sh gpui iced tui bevy",
        "fi",
    ]
)

SYNTHETIC_LOCAL_SWALLOWED = "\n".join(
    [
        '[[ "$tier" == "quick" ]] && exit "$((failures > 0))"',
        "if maybe production-config; then",
        "    run_stage production-config standard bash "
        "scripts/quality/production-config-check.sh || true",
        "fi",
    ]
)

SYNTHETIC_LOCAL_OUTSIDE_STAGE = "\n".join(
    [
        '[[ "$tier" == "quick" ]] && exit "$((failures > 0))"',
        "if maybe clippy; then",
        "    run_stage production-config standard bash "
        "scripts/quality/production-config-check.sh",
        "fi",
        "if maybe production-config; then",
        "    true",
        "fi",
    ]
)

SYNTHETIC_LOCAL_NO_STAGE = "\n".join(
    [
        '[[ "$tier" == "quick" ]] && exit "$((failures > 0))"',
        "if maybe clippy; then",
        "    run_stage clippy standard cargo clippy --workspace -- -D warnings",
        "fi",
    ]
)

SYNTHETIC_LOCAL_NO_QUICK_DISPATCH = "\n".join(
    [
        "if maybe production-config; then",
        "    run_stage production-config standard bash "
        "scripts/quality/production-config-check.sh",
        "fi",
    ]
)


def _expect_parse_error(label: str, function, text: str) -> None:
    try:
        function(text)
    except ParseError:
        return
    raise AssertionError(f"{label}: expected a ParseError (exit code 2), got a parsed site")


def self_test() -> int:
    # Both hosts wired: the folded block scalar, the decoy in another job, and
    # the backslash continuation all parse to a green verdict.
    report = Report(
        local=parse_local_wiring(SYNTHETIC_LOCAL_OK),
        ci=parse_ci_wiring(SYNTHETIC_CI_OK),
    )
    assert not report.failures(), "a fully wired pair must pass"
    assert verdict(report) == 0, "a fully wired pair must map to exit code 0"

    # Deleting the CI step goes red and keeps the commented-out reference as a note.
    commented = parse_ci_wiring(SYNTHETIC_CI_COMMENTED_OUT)
    assert not commented.ok and commented.code == "missing"
    assert commented.notes, "a commented-out reference must be reported as a note"
    assert "comment" in commented.notes[0]

    # An explicit `continue-on-error: false` is wiring; anything else is not.
    neutralised = parse_ci_wiring(SYNTHETIC_CI_CONTINUE_ON_ERROR)
    assert not neutralised.ok and neutralised.code == "continue_on_error"
    assert parse_ci_wiring(SYNTHETIC_CI_FALSE_CONTINUE_ON_ERROR).ok

    # Local drift: commented out, wrong tier, bypassing run_stage, swallowed, moved.
    commented_local = parse_local_wiring(SYNTHETIC_LOCAL_COMMENTED_OUT)
    assert not commented_local.ok and commented_local.code == "missing"
    assert commented_local.notes, "a commented-out local reference must be reported as a note"
    wrong_tier = parse_local_wiring(SYNTHETIC_LOCAL_WRONG_TIER)
    assert not wrong_tier.ok and wrong_tier.code == "wrong_tier"
    assert "quick" in wrong_tier.detail
    direct = parse_local_wiring(SYNTHETIC_LOCAL_DIRECT_CALL)
    assert not direct.ok and direct.code == "not_run_stage"
    swallowed = parse_local_wiring(SYNTHETIC_LOCAL_SWALLOWED)
    assert not swallowed.ok and swallowed.code == "swallowed"
    outside = parse_local_wiring(SYNTHETIC_LOCAL_OUTSIDE_STAGE)
    assert not outside.ok and outside.code == "outside_stage"

    # Anything the guard cannot locate fails closed at parse time (exit code 2).
    _expect_parse_error("ci without a lint job", parse_ci_wiring, SYNTHETIC_CI_NO_LINT_JOB)
    _expect_parse_error("ci without jobs", parse_ci_wiring, SYNTHETIC_CI_NO_JOBS)
    _expect_parse_error(
        "ci with two invocations", parse_ci_wiring, SYNTHETIC_CI_TWO_INVOCATIONS
    )
    _expect_parse_error(
        "lint job without steps", parse_ci_wiring, SYNTHETIC_CI_LINT_WITHOUT_STEPS
    )
    _expect_parse_error("local without the stage", parse_local_wiring, SYNTHETIC_LOCAL_NO_STAGE)
    _expect_parse_error(
        "local without the quick dispatch",
        parse_local_wiring,
        SYNTHETIC_LOCAL_NO_QUICK_DISPATCH,
    )

    # A missing file is a parse failure, never a silent pass. The partial tree
    # also proves the helper script itself is part of the contract.
    with tempfile.TemporaryDirectory(prefix="wiring-guard-") as directory:
        root = Path(directory)
        try:
            collect(root)
        except ParseError:
            pass
        else:
            raise AssertionError("collect must fail closed when a required file is missing")
        (root / ".github/workflows").mkdir(parents=True)
        (root / ".github/workflows/ci.yml").write_text(SYNTHETIC_CI_OK, encoding="utf-8")
        (root / "scripts/quality").mkdir(parents=True)
        (root / "scripts/quality/local-gates.sh").write_text(
            SYNTHETIC_LOCAL_OK, encoding="utf-8"
        )
        try:
            collect(root)
        except ParseError:
            pass
        else:
            raise AssertionError("collect must fail closed when the helper is missing")

    print("production-config-wiring guard self-test: PASS")
    return 0


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--repo-root",
        type=Path,
        default=Path(__file__).resolve().parents[2],
        help="repository root (defaults to the checkout this script lives in)",
    )
    parser.add_argument("--json", action="store_true", help="emit the verdict as JSON")
    parser.add_argument(
        "--self-test", action="store_true", help="prove the guard on synthetic text"
    )
    args = parser.parse_args(argv)

    if args.self_test:
        return self_test()

    try:
        report = collect(args.repo_root.resolve())
    except (OSError, ParseError) as error:
        if args.json:
            print(json.dumps(payload(None, str(error)), indent=2))
        else:
            print(f"production-config-wiring: PARSE ERROR: {error}", file=sys.stderr)
        return 2

    code = verdict(report)
    if args.json:
        print(json.dumps(payload(report), indent=2))
        return code

    if code == 0:
        print("production-config-wiring: PASS (both hosts enforce the production-config gate)")
        for site in report.sites():
            print(f"  {site.label:5s} {site.location}")
            print(f"        {site.command}")
        return 0

    print(
        f"production-config-wiring: FAIL "
        f"({len(report.failures())} of 2 wiring site(s) no longer enforced)"
    )
    for site in report.sites():
        status = "ok" if site.ok else site.code.upper()
        print(f"  {site.label:5s} {site.location} [{status}]")
        if site.detail:
            print(f"        {site.detail}")
    print("  differences:")
    for site in report.failures():
        print(f"    {site.site} [{site.code}] {site.detail}")
    for site in report.sites():
        for note in site.notes:
            print(f"  note: {note}")
    return 1


if __name__ == "__main__":
    sys.exit(main())
