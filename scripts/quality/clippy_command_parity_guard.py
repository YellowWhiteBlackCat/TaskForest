#!/usr/bin/env python3
"""Fail-closed parity guard: CI clippy command == local standard-gate clippy command.

Two consecutive rounds found the same class of drift between the CI `lint`
job (``.github/workflows/ci.yml``) and this repository's own standard gate
(``scripts/quality/local-gates.sh``, stage ``clippy``):

* W12-C found the structural-ratchet flags (``-W clippy::cognitive_complexity``,
  ``-W clippy::too_many_lines``) only in the local gate, so ``clippy.toml``'s
  48/650 thresholds never ran on CI (both lints are allow-by-default).
* W14-B aligned ci.yml but could only leave a comment ("Keep this command
  identical to the local gate") because ``scripts/**`` was out of its boundary.

This guard turns that comment into a mechanical invariant: it parses the single
CI clippy ``run`` step and the workspace-scope clippy stage, extracts the
*clippy configuration*, and fails when they differ.

Compared dimensions (order- and whitespace-insensitive, compared as multisets):

``packages``
    ``--workspace`` (or its alias ``--all``) versus ``-p`` package lists.
``targets``
    ``--all-targets`` / ``--lib`` / ``--bins`` / ``--examples`` / ``--tests``
    / ``--benches``.
``features``
    every ``--features`` value (comma/space split) plus ``--all-features``.
``feature_flags``
    ``--no-default-features``.
``lints``
    the flags after the ``--`` separator, normalised across equivalent
    spellings (``-Dwarnings`` == ``-D warnings``, ``--deny=warnings`` ==
    ``-D warnings``) and compared as a multiset.
``other``
    any remaining flag, so an unrecognised option cannot drift silently.

Deliberately *not* compared: lock hygiene (``--locked`` in CI versus the local
gate's ``${LOCK_ARGS[@]}``, which may legally be empty in the documented
dev-phase fallback), the surrounding env/``RUSTFLAGS``, and anything before
``cargo clippy``. A dynamic ``${...[@]}`` expansion inside the compared
workspace branch is a parse failure (exit 2), never silently equal.

Exit codes: 0 consistent, 1 drift, 2 parse failure (missing/ambiguous step,
unparsable command). ``--json`` emits the same verdict machine-readably.

Usage::

    python3 scripts/quality/clippy_command_parity_guard.py
    python3 scripts/quality/clippy_command_parity_guard.py --json
    python3 scripts/quality/clippy_command_parity_guard.py --self-test
"""

from __future__ import annotations

import argparse
import json
import re
import shlex
import sys
from collections import Counter
from dataclasses import dataclass
from pathlib import Path

CI_WORKFLOW = ".github/workflows/ci.yml"
LOCAL_GATES = "scripts/quality/local-gates.sh"

LINE_CONTINUATION = re.compile(r"\\\n[ \t]*")
DYNAMIC_ARRAY = re.compile(r"^\$\{[A-Za-z_][A-Za-z0-9_]*\[@\]\}$")

# Shell lint-level spellings that mean the same rustc level.
LINT_LEVELS = {
    "-D": "-D",
    "-W": "-W",
    "-A": "-A",
    "-F": "-F",
    "--deny": "-D",
    "--warn": "-W",
    "--allow": "-A",
    "--forbid": "-F",
}
LONG_LINT_LEVELS = {
    "--deny=": "-D",
    "--warn=": "-W",
    "--allow=": "-A",
    "--forbid=": "-F",
}

# Invocation hygiene that does not change what clippy lints. `--locked` is a
# fixed CI flag while the local gate expands "${LOCK_ARGS[@]}" and may drop it
# in the documented dev-phase fallback, so it must not count as drift.
IGNORED_FIXED = frozenset({"--locked", "--frozen", "--offline"})
IGNORED_DYNAMIC = frozenset({"${LOCK_ARGS[@]}"})

# One `run:` step, inline (`- run: cmd`) or as a plain key (`run: cmd` /
# `run: |` block scalar). Comments and other keys never match.
RUN_STEP = re.compile(r"^(?P<indent>[ \t]*)(?:-[ \t]+)?run:[ \t]*(?P<value>.*?)[ \t]*$")
BLOCK_SCALAR = re.compile(r"^[|>][+-]?[0-9]?$")
CLIPPY_INVOCATION = re.compile(r"\bcargo\s+clippy\b")
STAGE_OPEN = re.compile(r"^if maybe clippy; then\s*$")
STAGE_COMMAND = re.compile(r"^\s*run_stage clippy\s")

DIMENSIONS = ("packages", "targets", "features", "feature_flags", "lints", "other")


class ParseError(Exception):
    """The guard cannot statically locate or compare a clippy command."""


# ---------------------------------------------------------------------------
# Configuration model
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class ClippyConfig:
    """The comparable clippy configuration of one command."""

    source: str
    command: str
    packages: frozenset[str]
    targets: frozenset[str]
    features: frozenset[str]
    feature_flags: frozenset[str]
    lints: tuple[str, ...]
    other: tuple[str, ...]

    def as_dict(self) -> dict[str, object]:
        return {
            "source": self.source,
            "command": self.command,
            "packages": sorted(self.packages),
            "targets": sorted(self.targets),
            "features": sorted(self.features),
            "feature_flags": sorted(self.feature_flags),
            "lints": list(self.lints),
            "other": list(self.other),
        }


@dataclass(frozen=True)
class Difference:
    """One dimension that differs between CI and the local gate."""

    dimension: str
    ci_only: tuple[str, ...]
    local_only: tuple[str, ...]

    def as_dict(self) -> dict[str, object]:
        return {
            "dimension": self.dimension,
            "ci_only": list(self.ci_only),
            "local_only": list(self.local_only),
        }


# ---------------------------------------------------------------------------
# Text helpers
# ---------------------------------------------------------------------------


def tokenize_shell(text: str) -> list[str]:
    """Tokenize one logical shell command, joining backslash continuations.

    ``shlex`` keeps ``"${NAME[@]}"`` as a single token, which is exactly what
    the dynamic-expansion check below needs.
    """
    joined = LINE_CONTINUATION.sub(" ", text)
    try:
        return shlex.split(joined)
    except ValueError as error:
        raise ParseError(f"cannot tokenize shell command ({error}): {text!r}") from error


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


def split_features(value: str) -> list[str]:
    return [item for item in re.split(r"[,\s]+", value.strip()) if item]


def normalize_lints(tokens: list[str]) -> tuple[str, ...]:
    """Normalise post-``--`` lint flags and return them as a sorted multiset."""
    normalized: list[str] = []
    index = 0
    while index < len(tokens):
        token = tokens[index]
        if token in LINT_LEVELS:
            if index + 1 >= len(tokens):
                raise ParseError(f"dangling lint level flag '{token}'")
            normalized.append(f"{LINT_LEVELS[token]} {tokens[index + 1]}")
            index += 2
            continue
        matched: str | None = None
        for prefix, level in LONG_LINT_LEVELS.items():
            if token.startswith(prefix):
                matched = f"{level} {token[len(prefix):]}"
                break
        if matched is not None:
            normalized.append(matched)
        elif len(token) > 2 and token[0] == "-" and token[1] in "DWAF" and token[2] != "-":
            normalized.append(f"-{token[1]} {token[2:]}")
        else:
            normalized.append(token)
        index += 1
    return tuple(sorted(normalized))


def _excess(own: Counter[str], other: Counter[str]) -> tuple[str, ...]:
    excess: list[str] = []
    for value, count in own.items():
        excess.extend([value] * max(0, count - other.get(value, 0)))
    return tuple(sorted(excess))


# ---------------------------------------------------------------------------
# Command parsers
# ---------------------------------------------------------------------------


def _parse_clippy_command(source: str, command: str) -> ClippyConfig:
    tokens = tokenize_shell(command)
    start: int | None = None
    for index in range(len(tokens) - 1):
        if tokens[index] == "cargo" and tokens[index + 1] == "clippy":
            start = index + 2
            break
    if start is None:
        raise ParseError(f"{source}: no `cargo clippy` invocation in: {command}")

    args = tokens[start:]
    packages: set[str] = set()
    targets: set[str] = set()
    features: set[str] = set()
    feature_flags: set[str] = set()
    other: list[str] = []
    index = 0
    lints: tuple[str, ...] | None = None
    while index < len(args):
        token = args[index]
        if token == "--":
            lints = normalize_lints(args[index + 1 :])
            break
        if DYNAMIC_ARRAY.match(token):
            if token not in IGNORED_DYNAMIC:
                raise ParseError(
                    f"{source}: dynamic expansion {token} cannot be compared statically"
                )
        elif token in IGNORED_FIXED:
            pass
        elif token in ("--workspace", "--all"):
            packages.add("<workspace>")
        elif token in ("-p", "--package"):
            index += 1
            if index >= len(args):
                raise ParseError(f"{source}: `{token}` is missing its package name")
            packages.add(f"-p {args[index]}")
        elif token.startswith("--package="):
            packages.add(f"-p {token.split('=', 1)[1]}")
        elif token.startswith("-p") and len(token) > 2:
            packages.add(f"-p {token[2:]}")
        elif token in ("--all-targets", "--lib", "--bins", "--examples", "--tests", "--benches"):
            targets.add(token)
        elif token == "--features":
            index += 1
            if index >= len(args):
                raise ParseError(f"{source}: `--features` is missing its value")
            features.update(split_features(args[index]))
        elif token.startswith("--features="):
            features.update(split_features(token.split("=", 1)[1]))
        elif token == "--all-features":
            features.add("<all-features>")
        elif token == "--no-default-features":
            feature_flags.add("--no-default-features")
        else:
            other.append(token)
        index += 1
    if lints is None:
        raise ParseError(f"{source}: clippy command has no `--` separator before lint flags")

    return ClippyConfig(
        source=source,
        command=command,
        packages=frozenset(packages),
        targets=frozenset(targets),
        features=frozenset(features),
        feature_flags=frozenset(feature_flags),
        lints=lints,
        other=tuple(sorted(other)),
    )


def yaml_run_steps(text: str) -> list[tuple[int, str]]:
    """Return ``(line_number, command)`` for every ``run:`` step in the YAML."""
    lines = text.splitlines()
    steps: list[tuple[int, str]] = []
    index = 0
    while index < len(lines):
        line = lines[index]
        match = RUN_STEP.match(line)
        if match is None:
            index += 1
            continue
        line_number = index + 1
        value = strip_yaml_inline_comment(match.group("value"))
        if BLOCK_SCALAR.match(value):
            key_indent = line.index("run:")
            body: list[str] = []
            cursor = index + 1
            while cursor < len(lines):
                candidate = lines[cursor]
                if not candidate.strip():
                    body.append("")
                    cursor += 1
                    continue
                leading = len(candidate) - len(candidate.lstrip(" "))
                if leading <= key_indent:
                    break
                body.append(candidate[leading:])
                cursor += 1
            steps.append((line_number, "\n".join(body)))
            index = cursor
        else:
            steps.append((line_number, unquote_yaml_scalar(value)))
            index += 1
    return steps


def parse_ci_clippy(text: str) -> ClippyConfig:
    """Parse the one canonical cargo-clippy ``run`` step of ci.yml."""
    clippy_steps = [
        (line_number, command)
        for line_number, command in yaml_run_steps(text)
        if CLIPPY_INVOCATION.search(command)
    ]
    if not clippy_steps:
        raise ParseError(f"{CI_WORKFLOW}: no `run:` step invokes cargo clippy")
    if len(clippy_steps) > 1:
        locations = ", ".join(f"line {number}" for number, _ in clippy_steps)
        raise ParseError(
            f"{CI_WORKFLOW}: {len(clippy_steps)} cargo clippy run steps ({locations}); "
            "the guard pins exactly one canonical step"
        )
    line_number, command = clippy_steps[0]
    return _parse_clippy_command(f"{CI_WORKFLOW}:{line_number}", command)


def local_gate_clippy_commands(text: str) -> list[str]:
    """Return the logical commands of `run_stage clippy` inside the stage block."""
    lines = text.splitlines()
    start: int | None = None
    for index, line in enumerate(lines):
        if STAGE_OPEN.match(line):
            start = index
            break
    if start is None:
        raise ParseError(f"{LOCAL_GATES}: clippy stage (`if maybe clippy; then`) not found")
    block: list[str] = []
    cursor = start + 1
    while cursor < len(lines) and lines[cursor] != "fi":
        block.append(lines[cursor])
        cursor += 1
    if cursor >= len(lines):
        raise ParseError(f"{LOCAL_GATES}: clippy stage has no closing `fi`")

    commands: list[str] = []
    index = 0
    while index < len(block):
        if STAGE_COMMAND.match(block[index]):
            logical = block[index].strip()
            while logical.endswith("\\") and index + 1 < len(block):
                index += 1
                logical = logical[:-1].rstrip() + " " + block[index].strip()
            commands.append(logical)
        index += 1
    if not commands:
        raise ParseError(f"{LOCAL_GATES}: clippy stage has no `run_stage clippy` command")
    return commands


def parse_local_gate_clippy(text: str) -> ClippyConfig:
    """Parse the workspace-scope clippy stage of local-gates.sh."""
    workspace: list[str] = []
    for command in local_gate_clippy_commands(text):
        if "--workspace" in tokenize_shell(command):
            workspace.append(command)
    if not workspace:
        raise ParseError(
            f"{LOCAL_GATES}: clippy stage has no workspace-scope (`--workspace`) command"
        )
    if len(workspace) > 1:
        raise ParseError(
            f"{LOCAL_GATES}: clippy stage has {len(workspace)} workspace-scope commands; "
            "cannot pick one canonical command"
        )
    return _parse_clippy_command(f"{LOCAL_GATES}: clippy stage", workspace[0])


# ---------------------------------------------------------------------------
# Verdict
# ---------------------------------------------------------------------------


def compare(ci: ClippyConfig, local: ClippyConfig) -> list[Difference]:
    differences: list[Difference] = []
    for dimension in DIMENSIONS:
        ci_counts = Counter(getattr(ci, dimension))
        local_counts = Counter(getattr(local, dimension))
        if ci_counts == local_counts:
            continue
        differences.append(
            Difference(
                dimension=dimension,
                ci_only=_excess(ci_counts, local_counts),
                local_only=_excess(local_counts, ci_counts),
            )
        )
    return differences


def verdict(differences: list[Difference]) -> int:
    """0 = identical, 1 = drift (parse failures are exceptions, mapped to 2)."""
    return 1 if differences else 0


def collect(repository: Path) -> tuple[ClippyConfig, ClippyConfig, list[Difference]]:
    ci_path = repository / CI_WORKFLOW
    local_path = repository / LOCAL_GATES
    for path in (ci_path, local_path):
        if not path.is_file():
            raise ParseError(f"missing required file: {path}")
    ci = parse_ci_clippy(ci_path.read_text(encoding="utf-8"))
    local = parse_local_gate_clippy(local_path.read_text(encoding="utf-8"))
    return ci, local, compare(ci, local)


# ---------------------------------------------------------------------------
# Output
# ---------------------------------------------------------------------------


def payload(
    ci: ClippyConfig | None,
    local: ClippyConfig | None,
    differences: list[Difference],
    error: str | None = None,
) -> dict[str, object]:
    if error is not None:
        status = "parse_error"
    elif differences:
        status = "fail"
    else:
        status = "pass"
    return {
        "status": status,
        "ci": ci.as_dict() if ci is not None else None,
        "local": local.as_dict() if local is not None else None,
        "differences": [difference.as_dict() for difference in differences],
        "error": error,
    }


def render_values(values: tuple[str, ...]) -> str:
    return " ".join(values) if values else "(none)"


# ---------------------------------------------------------------------------
# Self-test
# ---------------------------------------------------------------------------

SYNTHETIC_CI = "\n".join(
    [
        "jobs:",
        "  lint:",
        "    steps:",
        "      - uses: actions/checkout@v7",
        "      - run: cargo clippy --locked --workspace --all-targets "
        "--features test-support -- -D warnings "
        "-W clippy::cognitive_complexity -W clippy::too_many_lines",
    ]
)

# Same configuration, different spelling: flag order, extra whitespace, an
# attached `-Dwarnings`, and a YAML block scalar instead of the inline step.
SYNTHETIC_CI_EQUIVALENT = "\n".join(
    [
        "jobs:",
        "  lint:",
        "    steps:",
        "      - name: clippy",
        "        run: |",
        "          cargo  clippy  --workspace --all-targets --features test-support \\",
        "            -- -W clippy::too_many_lines -Dwarnings "
        "-W clippy::cognitive_complexity",
    ]
)

SYNTHETIC_CI_MISSING_RATCHET = "\n".join(
    [
        "jobs:",
        "  lint:",
        "    steps:",
        "      - run: cargo clippy --locked --workspace --all-targets "
        "--features test-support -- -D warnings -W clippy::cognitive_complexity",
    ]
)

SYNTHETIC_CI_EXTRA_FEATURE = "\n".join(
    [
        "jobs:",
        "  lint:",
        "    steps:",
        "      - run: cargo clippy --locked --workspace --all-targets "
        "--features test-support,extra -- -D warnings "
        "-W clippy::cognitive_complexity -W clippy::too_many_lines",
    ]
)

SYNTHETIC_CI_PACKAGE_SCOPED = "\n".join(
    [
        "jobs:",
        "  lint:",
        "    steps:",
        "      - run: cargo clippy --locked -p taskmanager-core --all-targets "
        "--features test-support -- -D warnings "
        "-W clippy::cognitive_complexity -W clippy::too_many_lines",
    ]
)

SYNTHETIC_CI_NO_CLIPPY = "\n".join(
    [
        "jobs:",
        "  lint:",
        "    steps:",
        "      - run: cargo test --locked --workspace",
    ]
)

SYNTHETIC_CI_TWO_CLIPPY = "\n".join(
    [
        "jobs:",
        "  lint:",
        "    steps:",
        "      - run: cargo clippy --locked --workspace -- -D warnings",
        "      - run: cargo clippy --locked -p taskmanager-core -- -D warnings",
    ]
)

SYNTHETIC_LOCAL = "\n".join(
    [
        "if maybe clippy; then",
        '    if [[ "$scope" == "all" ]]; then',
        '        run_stage clippy standard cargo clippy "${LOCK_ARGS[@]}" '
        "--workspace --all-targets --features test-support -- \\",
        "            -D warnings -W clippy::cognitive_complexity -W clippy::too_many_lines",
        "    else",
        '        run_stage clippy standard cargo clippy "${LOCK_ARGS[@]}" '
        '"${SCOPE_CLIPPY_ARGS[@]}" --all-targets "${SCOPE_FEATURE_ARGS[@]}" -- \\',
        "            -D warnings -W clippy::cognitive_complexity -W clippy::too_many_lines",
        "    fi",
        "fi",
    ]
)

SYNTHETIC_LOCAL_NO_STAGE = "\n".join(
    [
        "if maybe nextest-core; then",
        "    run_stage nextest-core standard cargo nextest run --locked --workspace",
        "fi",
    ]
)

# A dynamic expansion other than LOCK_ARGS inside the compared branch must not
# be waved through as "equal".
SYNTHETIC_LOCAL_DYNAMIC_SCOPE = "\n".join(
    [
        "if maybe clippy; then",
        '    run_stage clippy standard cargo clippy "${SCOPE_CLIPPY_ARGS[@]}" '
        "--workspace --all-targets --features test-support -- "
        "-D warnings -W clippy::cognitive_complexity -W clippy::too_many_lines",
        "fi",
    ]
)


def _expect_parse_error(label: str, function, text: str) -> None:
    try:
        function(text)
    except ParseError:
        return
    raise AssertionError(f"{label}: expected a ParseError (exit code 2), got a parsed command")


def self_test() -> int:
    local = parse_local_gate_clippy(SYNTHETIC_LOCAL)
    ci = parse_ci_clippy(SYNTHETIC_CI)

    # Identical configuration passes, including the exit-code mapping.
    assert not compare(ci, local), "identical commands must compare equal"
    assert verdict([]) == 0, "parity must map to exit code 0"

    # Flag order, spacing, attached values, and block scalars are not drift.
    equivalent = parse_ci_clippy(SYNTHETIC_CI_EQUIVALENT)
    assert not compare(equivalent, local), "flag reordering/spacing must not count as drift"

    # One missing `-W` ratchet goes red, with the exact local-only value shown.
    drift = compare(parse_ci_clippy(SYNTHETIC_CI_MISSING_RATCHET), local)
    assert drift, "a dropped -W ratchet must be reported as drift"
    assert verdict(drift) == 1, "drift must map to exit code 1"
    assert [item.dimension for item in drift] == ["lints"]
    assert drift[0].ci_only == (), "the missing flag is CI-only, not local-only"
    assert drift[0].local_only == ("-W clippy::too_many_lines",)

    # Feature and package-scope drift are reported on their own dimensions.
    feature_drift = compare(parse_ci_clippy(SYNTHETIC_CI_EXTRA_FEATURE), local)
    assert [item.dimension for item in feature_drift] == ["features"]
    assert feature_drift[0].ci_only == ("extra",)
    package_drift = compare(parse_ci_clippy(SYNTHETIC_CI_PACKAGE_SCOPED), local)
    assert [item.dimension for item in package_drift] == ["packages"]
    assert package_drift[0].ci_only == ("-p taskmanager-core",)
    assert package_drift[0].local_only == ("<workspace>",)

    # Missing clippy steps and ambiguous structures fail closed at parse time.
    _expect_parse_error("ci without clippy", parse_ci_clippy, SYNTHETIC_CI_NO_CLIPPY)
    _expect_parse_error("ci with two clippy steps", parse_ci_clippy, SYNTHETIC_CI_TWO_CLIPPY)
    _expect_parse_error(
        "local without clippy stage", parse_local_gate_clippy, SYNTHETIC_LOCAL_NO_STAGE
    )
    _expect_parse_error(
        "local with dynamic scope expansion",
        parse_local_gate_clippy,
        SYNTHETIC_LOCAL_DYNAMIC_SCOPE,
    )

    print("clippy-command-parity guard self-test: PASS")
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
        ci, local, differences = collect(args.repo_root.resolve())
    except (OSError, ParseError) as error:
        if args.json:
            print(json.dumps(payload(None, None, [], str(error)), indent=2))
        else:
            print(f"clippy-command-parity: PARSE ERROR: {error}", file=sys.stderr)
        return 2

    code = verdict(differences)
    if args.json:
        print(json.dumps(payload(ci, local, differences), indent=2))
        return code

    if code == 0:
        print("clippy-command-parity: PASS (CI lint job clippy == local standard gate clippy)")
        print(f"  ci:    {ci.command}")
        print(f"  local: {local.command}")
        return 0

    print(f"clippy-command-parity: FAIL ({len(differences)} dimension(s) drifted)")
    print(f"  ci:    {ci.command}")
    print(f"  local: {local.command}")
    for difference in differences:
        print(f"  {difference.dimension}:")
        print(f"    ci-only:    {render_values(difference.ci_only)}")
        print(f"    local-only: {render_values(difference.local_only)}")
    return 1


if __name__ == "__main__":
    sys.exit(main())
