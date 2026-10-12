#!/usr/bin/env python3
"""Drive and validate the real TUI terminal-profile capture matrix."""

from __future__ import annotations

import argparse
import csv
import hashlib
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path


UNSET = "<unset>"
MATRIX = Path(__file__).with_name("capture_tui_terminal_profiles.tsv")
SIGNAL_COLUMNS = {
    "term": "TERM",
    "colorterm": "COLORTERM",
    "no_color": "NO_COLOR",
    "lc_all": "LC_ALL",
    "lc_ctype": "LC_CTYPE",
    "lc_messages": "LC_MESSAGES",
    "lang": "LANG",
    "glyph_override": "TM_TUI_GLYPHS",
}
EXPECTED = {
    "zh-dumb": ("zh", "ascii", "monochrome"),
    "zh-explicit-ascii": ("zh", "ascii", "ansi256"),
    "zh-nonutf-locale": ("zh", "unicode", "ansi256"),
    "zh-missing-locale": ("zh", "unicode", "ansi256"),
    "color-16": ("en", "unicode", "ansi16"),
    "color-256": ("en", "unicode", "ansi256"),
    "color-truecolor": ("en", "unicode", "truecolor"),
    "color-no-color": ("en", "unicode", "monochrome"),
}
FIELDS = (
    "name",
    *SIGNAL_COLUMNS,
    "expected_language",
    "expected_glyphs",
    "expected_color",
)
PROFILE_MARKER = re.compile(
    r"TUI_CAPTURE_MARKER event=terminal_profile "
    r"language=(en|zh) glyphs=(ascii|unicode) "
    r"color=(truecolor|ansi256|ansi16|monochrome)"
)


class MatrixError(RuntimeError):
    """Raised when the terminal matrix is incomplete or ambiguous."""


def read_profiles(path: Path = MATRIX) -> dict[str, dict[str, str]]:
    with path.open(newline="", encoding="utf-8") as stream:
        reader = csv.DictReader(stream, delimiter="\t")
        if tuple(reader.fieldnames or ()) != FIELDS:
            raise MatrixError(f"unexpected terminal matrix header: {reader.fieldnames!r}")
        rows: dict[str, dict[str, str]] = {}
        for number, row in enumerate(reader, start=2):
            if None in row or any(value is None for value in row.values()):
                raise MatrixError(f"terminal matrix row {number} has the wrong column count")
            name = row["name"]
            if not name or name in rows:
                raise MatrixError(f"terminal matrix row {number} has a blank or duplicate name")
            if any("\n" in value or "\r" in value for value in row.values()):
                raise MatrixError(f"terminal matrix row {number} contains a line break")
            for column in SIGNAL_COLUMNS:
                if not row[column]:
                    raise MatrixError(
                        f"terminal matrix row {number} must explicitly set {column} "
                        f"or use {UNSET!r}"
                    )
            expected = EXPECTED.get(name)
            actual = (
                row["expected_language"],
                row["expected_glyphs"],
                row["expected_color"],
            )
            if expected is None or actual != expected:
                raise MatrixError(f"terminal matrix row {name!r} has an invalid expected profile")
            rows[name] = row

    missing = sorted(EXPECTED.keys() - rows.keys())
    extra = sorted(rows.keys() - EXPECTED.keys())
    if missing or extra:
        raise MatrixError(f"terminal matrix coverage differs: missing={missing}, extra={extra}")
    for name, row in rows.items():
        require_input(name, row)
    return rows


def require_input(name: str, row: dict[str, str]) -> None:
    """Fail closed if a named acceptance case no longer drives its signal."""
    term = row["term"]
    color_term = row["colorterm"]
    no_color = row["no_color"]
    glyph = row["glyph_override"]
    lang = row["lang"]
    messages = row["lc_messages"]
    selected_language = next(
        (
            row[key]
            for key in ("lc_all", "lc_messages", "lang")
            if row[key] != UNSET
        ),
        UNSET,
    )
    selected_terminal_locale = next(
        (
            row[key]
            for key in ("lc_all", "lc_ctype", "lang")
            if row[key] != UNSET
        ),
        UNSET,
    )
    if not selected_language.lower().startswith(row["expected_language"]):
        raise MatrixError(f"{name} locale inputs do not select its declared UI language")
    if row["expected_glyphs"] == "unicode" and name.startswith("color-") and not any(
        token in selected_terminal_locale.lower() for token in ("utf8", "utf-8")
    ):
        raise MatrixError(f"{name} must use a UTF-8 locale for its Unicode glyph profile")
    if name == "zh-dumb" and not (
        term == "dumb"
        and lang.lower().startswith("zh")
        and glyph == UNSET
        and color_term == UNSET
        and no_color == UNSET
    ):
        raise MatrixError("zh-dumb must exercise TERM=dumb with Zh language and no override")
    if name == "zh-explicit-ascii" and not (
        "256color" in term
        and lang.lower().startswith("zh")
        and glyph.lower() == "ascii"
        and no_color == UNSET
        and color_term == UNSET
    ):
        raise MatrixError("zh-explicit-ascii must exercise a Zh ASCII override")
    if name == "zh-nonutf-locale" and not (
        lang.lower().startswith("zh")
        and "utf" not in lang.lower()
        and row["lc_all"] == UNSET
        and row["lc_ctype"] == UNSET
        and glyph == UNSET
        and "256color" in term
        and no_color == UNSET
        and color_term == UNSET
    ):
        raise MatrixError("zh-nonutf-locale must use a non-UTF-8 LANG without an override")
    if name == "zh-missing-locale" and not (
        messages.lower().startswith("zh")
        and all(row[key] == UNSET for key in ("lc_all", "lc_ctype", "lang"))
        and glyph == UNSET
        and "256color" in term
        and no_color == UNSET
        and color_term == UNSET
    ):
        raise MatrixError("zh-missing-locale must select Zh through LC_MESSAGES only")
    if name == "color-16" and not (
        term == "xterm" and color_term == UNSET and no_color == UNSET
    ):
        raise MatrixError("color-16 must use xterm without COLORTERM or NO_COLOR")
    if name == "color-256" and not (
        "256color" in term and color_term == UNSET and no_color == UNSET
    ):
        raise MatrixError("color-256 must use a 256-color TERM without overrides")
    if name == "color-truecolor" and not (
        color_term.lower() in {"truecolor", "24bit", "direct"}
        and no_color == UNSET
        and term != "dumb"
    ):
        raise MatrixError("color-truecolor must declare COLORTERM truecolor")
    if name == "color-no-color" and not (
        no_color != UNSET and term != "dumb"
    ):
        raise MatrixError("color-no-color must set NO_COLOR on a color-capable TERM")


def self_test() -> None:
    rows = read_profiles()
    if set(rows) != set(EXPECTED):
        raise AssertionError("terminal matrix self-test did not load all required cases")
    malformed = dict(rows["zh-dumb"])
    malformed["term"] = "xterm"
    try:
        require_input("zh-dumb", malformed)
    except MatrixError:
        pass
    else:
        raise AssertionError("a dumb terminal case without its ASCII input was accepted")
    try:
        validate_selection(rows, ["missing-profile"])
    except MatrixError:
        pass
    else:
        raise AssertionError("an unrecognized capture profile was accepted")
    with tempfile.TemporaryDirectory() as temporary:
        repo = Path(temporary)
        binary = repo / "target" / "debug" / "taskforest-t"
        binary.parent.mkdir(parents=True)
        binary.write_bytes(b"prebuilt TUI self-test")
        binary.chmod(0o755)
        if validate_prebuilt_binary(repo, Path("target/debug/taskforest-t")) != binary:
            raise AssertionError("the canonical prebuilt TUI binary was not resolved")
        marker_path = repo / "markers.txt"
        marker_path.write_text(
            "TUI_CAPTURE_MARKER event=terminal_profile "
            "language=zh glyphs=unicode color=ansi256\n",
            encoding="utf-8",
        )
        if read_terminal_profile(marker_path) != ("zh", "unicode", "ansi256"):
            raise AssertionError("the resolved terminal profile marker was not parsed")
    print(f"TUI terminal-profile matrix self-test: PASS ({len(rows)} rows)")


def validate_selection(
    rows: dict[str, dict[str, str]], selected: list[str]
) -> list[dict[str, str]]:
    names = selected or list(EXPECTED)
    if len(names) != len(set(names)):
        raise MatrixError("terminal profile selection contains duplicate names")
    missing = [name for name in names if name not in rows]
    if missing:
        raise MatrixError(f"unknown terminal profile scenarios: {missing}")
    return [rows[name] for name in names]


def check_environment(row: dict[str, str]) -> None:
    mismatches = []
    for column, variable in SIGNAL_COLUMNS.items():
        actual = os.environ.get(variable, UNSET)
        if actual != row[column]:
            mismatches.append(f"{variable}={actual!r} expected {row[column]!r}")
    if mismatches:
        raise MatrixError(
            f"environment for {row['name']!r} differs from its canonical row: "
            + "; ".join(mismatches)
        )


def read_terminal_profile(markers_path: Path) -> tuple[str, str, str]:
    markers = markers_path.read_text(encoding="utf-8")
    lines = [
        line
        for line in markers.splitlines()
        if line.startswith("TUI_CAPTURE_MARKER event=terminal_profile ")
    ]
    if len(lines) != 1:
        raise MatrixError(
            f"expected exactly one resolved terminal profile marker, found {len(lines)}"
        )
    match = PROFILE_MARKER.fullmatch(lines[0])
    if match is None:
        raise MatrixError(f"malformed resolved terminal profile marker: {lines[0]!r}")
    return match.groups()


def validate_prebuilt_binary(repo: Path, binary: Path) -> Path:
    resolved = binary if binary.is_absolute() else repo / binary
    resolved = resolved.resolve()
    expected = (repo / "target" / "debug" / "taskforest-t").resolve()
    if resolved != expected or not resolved.is_file() or not os.access(resolved, os.X_OK):
        raise MatrixError(
            "matrix requires an executable prebuilt target/debug/taskforest-t binary"
        )
    return resolved


def run_profiles(repo: Path, selected: list[str], prebuilt_binary: Path) -> int:
    rows = read_profiles()
    chosen = validate_selection(rows, selected)
    binary = validate_prebuilt_binary(repo, prebuilt_binary)
    digest = hashlib.sha256(binary.read_bytes()).hexdigest()
    capture_script = repo / "scripts" / "capture-tui.sh"
    for row in chosen:
        environment = os.environ.copy()
        # Python's locale coercion can add LC_CTYPE while starting the capture
        # supervisor for profiles whose LANG is intentionally non-UTF-8.
        environment["PYTHONCOERCECLOCALE"] = "0"
        for column, variable in SIGNAL_COLUMNS.items():
            environment.pop(variable, None)
            if row[column] != UNSET:
                environment[variable] = row[column]
        environment.update(
            {
                "TM_TUI_CAPTURE_TERMINAL_SCENARIO": row["name"],
                "TM_TUI_CAPTURE_PAGE": "performance",
                "TM_TUI_CAPTURE_DEVICE": "cpu",
                "TM_TUI_CAPTURE_COLUMNS": "120",
                "TM_TUI_CAPTURE_LINES": "36",
                "TM_TUI_CAPTURE_SKIN": "gnome-dark",
                "TM_TUI_CAPTURE_PREBUILT_BINARY": str(binary),
            }
        )
        environment.pop("TM_TUI_CAPTURE_SCENE", None)
        environment.pop("TM_TUI_CAPTURE_SOURCE_FAILURE", None)
        print(
            f"TUI terminal-profile capture: {row['name']} binary_sha256={digest}",
            flush=True,
        )
        try:
            result = subprocess.run(
                ["bash", str(capture_script)],
                cwd=repo,
                env=environment,
                timeout=1800,
            )
        except subprocess.TimeoutExpired:
            print(
                f"TUI terminal-profile capture timed out: {row['name']} (1800s)",
                file=sys.stderr,
            )
            return 124
        if result.returncode != 0:
            print(
                f"TUI terminal-profile capture failed: {row['name']} "
                f"(exit {result.returncode})",
                file=sys.stderr,
            )
            return result.returncode or 1
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("scenarios", nargs="*", help="profile names; default runs the full matrix")
    parser.add_argument("--repo-root", type=Path, default=Path.cwd())
    parser.add_argument("--prebuilt-binary", type=Path)
    parser.add_argument("--list", action="store_true", help="list canonical profile names")
    parser.add_argument("--check-environment", metavar="SCENARIO")
    parser.add_argument("--read-marker", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    try:
        if args.self_test:
            self_test()
            return 0
        rows = read_profiles()
        if args.read_marker is not None:
            if args.scenarios or args.list or args.check_environment or args.prebuilt_binary:
                raise MatrixError("--read-marker does not accept other selections")
            print("\t".join(read_terminal_profile(args.read_marker)))
            return 0
        if args.check_environment:
            if args.scenarios or args.list or args.prebuilt_binary:
                raise MatrixError("--check-environment does not accept other selections")
            row = validate_selection(rows, [args.check_environment])[0]
            check_environment(row)
            return 0
        if args.list:
            if args.scenarios or args.prebuilt_binary:
                raise MatrixError("--list does not accept capture arguments")
            print("\n".join(rows))
            return 0
        if args.prebuilt_binary is None:
            raise MatrixError(
                "matrix capture requires --prebuilt-binary target/debug/taskforest-t; "
                "build it once before running the sequential captures"
            )
        return run_profiles(args.repo_root.resolve(), args.scenarios, args.prebuilt_binary)
    except (MatrixError, OSError) as error:
        print(f"TUI terminal-profile matrix: FAIL: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
