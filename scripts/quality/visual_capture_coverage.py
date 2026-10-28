#!/usr/bin/env python3
"""Fail-closed coverage guard for the real GPUI/Iced/Bevy capture matrices."""

from __future__ import annotations

import argparse
import csv
import re
import sys
from pathlib import Path


class CoverageError(RuntimeError):
    """Raised when a reachable visual surface has no capture contract."""


MATRIX_RUNNERS = {
    "gpui": ("scripts/capture_scenarios.tsv", "scripts/capture-niri.sh"),
    "iced": ("scripts/capture_iced_scenarios.tsv", "scripts/capture-iced-matrix.sh"),
    "bevy": ("scripts/capture_bevy_scenarios.tsv", "scripts/capture-bevy.sh"),
    "tui": ("scripts/capture_tui_scenarios.tsv", "scripts/capture-tui-matrix.sh"),
}

# Declaration-only matrices are a fail-closed finding unless the owner records
# why no committed runner consumes the matrix yet. The exemption is printed on
# every run and must be removed when the runner lands.
DECLARATION_ONLY_EXEMPTIONS: dict[str, str] = {}


def read_tsv(path: Path) -> list[dict[str, str]]:
    with path.open(newline="", encoding="utf-8") as stream:
        rows = list(csv.DictReader(stream, delimiter="\t"))
    if not rows:
        raise CoverageError(f"empty matrix: {path}")
    return rows


def camel_to_kebab(value: str) -> str:
    return re.sub(r"(?<!^)([A-Z])", r"-\1", value).lower()


def top_page_tokens(path: Path) -> set[str]:
    source = path.read_text(encoding="utf-8")
    match = re.search(r"pub enum TopPage\s*\{(.*?)\n\}", source, re.S)
    if match is None:
        raise CoverageError(f"cannot locate TopPage enum: {path}")
    variants = set(re.findall(r"^\s*([A-Z][A-Za-z0-9_]*)\s*,", match.group(1), re.M))
    if not variants:
        raise CoverageError(f"TopPage enum has no variants: {path}")
    return {camel_to_kebab(variant) for variant in variants}


def gpui_capture_tokens(path: Path) -> set[str]:
    source = path.read_text(encoding="utf-8")
    tokens = set(re.findall(r'^\s*"([^"]+)"\s*=> Some\(Self::', source, re.M))
    if not tokens:
        raise CoverageError(f"capture scenario parser has no tokens: {path}")
    return tokens


def audit_doc_scenario_tokens(path: Path) -> set[str]:
    source = path.read_text(encoding="utf-8")
    tokens = set(re.findall(r"\|\s*`([a-z0-9-]+)`\s*\|", source))
    if not tokens:
        raise CoverageError(f"UI parity audit doc has no scenario tokens: {path}")
    return tokens


KNOWN_SCENE_STATUSES = {
    "Aligned",
    "AcceptedDifference",
    "Partial",
    "Unsupported",
    "Pending",
}


def parse_audit_scene_statuses(source: str) -> dict[str, dict[str, str]]:
    """Parse the audit table's per-frontend status cells.

    The status vocabulary table has two columns and is skipped; the scene table
    has seven. Footnote suffixes such as ``Partial[A]`` are stripped so the
    capture requirement sees only the status token.
    """
    statuses: dict[str, dict[str, str]] = {}
    for line in source.splitlines():
        if not line.startswith("|"):
            continue
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        if len(cells) != 7:
            continue
        token_match = re.search(r"`([a-z0-9-]+)`", cells[1])
        if token_match is None:
            continue
        token = token_match.group(1)
        decisions = {
            frontend: cells[index].split("[", 1)[0].strip()
            for frontend, index in (("iced", 3), ("bevy", 4), ("tui", 5))
        }
        for frontend, status in decisions.items():
            if status not in KNOWN_SCENE_STATUSES:
                raise CoverageError(
                    f"UI parity audit has unknown status {status!r} for {token}/{frontend}"
                )
        statuses[token] = decisions
    if not statuses:
        raise CoverageError("UI parity audit doc has no scene status rows")
    return statuses


def audit_scene_statuses(path: Path) -> dict[str, dict[str, str]]:
    return parse_audit_scene_statuses(path.read_text(encoding="utf-8"))


def required_capture_tokens(
    statuses: dict[str, dict[str, str]], frontend: str
) -> set[str]:
    """A typed absence does not need a capture row; delivered/partial scenes do."""
    return {
        token
        for token, decisions in statuses.items()
        if decisions[frontend] not in {"Unsupported", "Pending"}
    }


def require_parity_scenarios(
    required: set[str], rows: list[dict[str, str]], frontend: str
) -> None:
    missing = sorted(required - {row["name"] for row in rows})
    if missing:
        raise CoverageError(f"{frontend} parity scenarios lack dedicated capture rows: {missing}")


def gpui_device_names(path: Path) -> set[str]:
    source = path.read_text(encoding="utf-8")
    names = set(re.findall(r'Some\("([^"]+)"\).*=> SelectedDevice::', source))
    names.discard("nic")
    names.discard("power")
    names.add("cpu")
    names.add("network")
    if not names:
        raise CoverageError(f"GPUI initial_selected has no device vocabulary: {path}")
    return names


def iced_page_names(path: Path) -> set[str]:
    source = path.read_text(encoding="utf-8")
    names = set(re.findall(r'AppPage::[A-Za-z0-9_]+\s*=>\s*"([^"]+)"', source))
    if not names:
        raise CoverageError(f"Iced page_name has no pages: {path}")
    return names


def iced_device_names(path: Path) -> set[str]:
    source = path.read_text(encoding="utf-8")
    names = set(
        re.findall(
            r'PerfDevice::[A-Za-z0-9_]+(?:\([^)]*\))?\s*=>\s*"([^"]+)"',
            source,
        )
    )
    if not names:
        raise CoverageError(f"Iced device_name has no devices: {path}")
    return names


def bevy_page_names(path: Path) -> set[str]:
    source = path.read_text(encoding="utf-8")
    match = re.search(r"pub\(crate\) enum Page\s*\{(.*?)\n\}", source, re.S)
    if match is None:
        raise CoverageError(f"cannot locate Bevy Page enum: {path}")
    variants = set(re.findall(r"^\s*([A-Z][A-Za-z0-9_]*)\s*,", match.group(1), re.M))
    if not variants:
        raise CoverageError(f"Bevy Page enum has no variants: {path}")
    return {camel_to_kebab(variant) for variant in variants}


def require_compact(rows: list[dict[str, str]], field: str, values: set[str], label: str) -> None:
    compact = {row[field] for row in rows if row.get("window_size") == "720x480"}
    missing = sorted(values - compact)
    if missing:
        raise CoverageError(f"{label} lacks compact 720x480 coverage: {missing}")


def function_string_literals(path: Path, functions: tuple[str, ...]) -> set[str]:
    """String literals inside the named `fn`s, used to recover the capture
    router's recognized token set from the app source."""
    source = path.read_text(encoding="utf-8")
    tokens: set[str] = set()
    for function in functions:
        match = re.search(rf"fn {function}\b.*?\n\}}", source, re.S)
        if match is None:
            raise CoverageError(f"cannot locate fn {function} in {path}")
        tokens |= set(re.findall(r'"([^"]+)"', match.group(0)))
    return tokens


def bevy_capture_router_tokens(path: Path) -> set[str]:
    """Tokens the Bevy capture router accepts on `TM_BEVY_CAPTURE_PAGE`."""
    tokens = function_string_literals(
        path, ("capture_scenario_target", "capture_page", "capture_wants_service_logs")
    )
    if not tokens:
        raise CoverageError(f"Bevy capture router has no tokens: {path}")
    return tokens


def iced_capture_router_tokens(root: Path) -> set[str]:
    """Tokens the Iced capture router accepts on `TM_ICED_CAPTURE_DEVICE`."""
    tokens = function_string_literals(
        root / "crates/taskmanager-iced/src/app/capture_fixtures.rs",
        ("capture_device_from_name", "capture_page_from_name"),
    )
    tokens |= function_string_literals(
        root / "crates/taskmanager-iced/src/app/capture_state.rs",
        (
            "apply_capture_target",
            "apply_capture_surface_and_process",
            "apply_capture_user_and_process_targets",
            "apply_capture_hardware_and_perf",
        ),
    )
    capture_source = root / "crates/taskmanager-iced/src/capture.rs"
    tokens |= function_string_literals(capture_source, ("persisted_history_requested",))
    tokens |= set(
        re.findall(
            r'const [A-Z0-9_]+: &str = "([^"]+)"',
            capture_source.read_text(encoding="utf-8"),
        )
    )
    if not tokens:
        raise CoverageError("Iced capture router has no tokens")
    return tokens


def require_recognized_column(
    rows: list[dict[str, str]], column: str, recognized: set[str], label: str
) -> None:
    """The capture driver passes one matrix column to the app router. A row
    whose behavioral column is unrecognized would silently capture a different
    surface, so check that column — not only the row label."""
    unrecognized = sorted({row[column] for row in rows} - recognized)
    if unrecognized:
        raise CoverageError(
            f"{label} capture {column} values are not recognized by the router: {unrecognized}"
        )


def matrix_runner_verdict(matrix_name: str, runner_source: str) -> bool:
    """Whether a committed runner actually loops the declared matrix.

    Mentioning the matrix path is not enough: a runner that only prints the
    matrix name (or captures a single environment-selected frame) must not
    satisfy the check.
    """
    return matrix_name in runner_source and "while IFS=" in runner_source


def runner_coverage(
    frontend: str,
    matrix_name: str,
    runner_source: str,
    exemptions: dict[str, str],
) -> tuple[str, str | None]:
    """Classify one matrix as consumed, explicitly exempt, or a failure."""
    if matrix_runner_verdict(matrix_name, runner_source):
        return "runner", None
    if frontend in exemptions:
        return "exempt", exemptions[frontend]
    raise CoverageError(
        f"{frontend} capture matrix {matrix_name} is declaration-only: "
        "no committed runner loops it"
    )


def validate(root: Path) -> dict[str, object]:
    gpui_matrix = read_tsv(root / "scripts/capture_scenarios.tsv")
    iced_matrix = read_tsv(root / "scripts/capture_iced_scenarios.tsv")
    bevy_matrix = read_tsv(root / "scripts/capture_bevy_scenarios.tsv")

    gpui_pages = {row["page"] for row in gpui_matrix}
    required_gpui_pages = top_page_tokens(
        root / "crates/taskmanager-gpui/src/gpui_app/root/navigation.rs"
    )
    missing_gpui_pages = sorted(required_gpui_pages - gpui_pages)
    if missing_gpui_pages:
        raise CoverageError(f"GPUI top pages lack Niri coverage: {missing_gpui_pages}")

    gpui_scenarios = {row["scenario"] for row in gpui_matrix} - {"standard"}
    required_scenarios = gpui_capture_tokens(
        root / "crates/taskmanager-gpui/src/gpui_app/root/capture/scenarios.rs"
    )
    missing_scenarios = sorted(required_scenarios - gpui_scenarios)
    if missing_scenarios:
        raise CoverageError(f"GPUI capture scenarios lack matrix rows: {missing_scenarios}")

    audit_tokens = audit_doc_scenario_tokens(root / "docs/UI_PARITY_AUDIT.md")
    scene_statuses = audit_scene_statuses(root / "docs/UI_PARITY_AUDIT.md")
    missing_statuses = sorted(audit_tokens - scene_statuses.keys())
    if missing_statuses:
        raise CoverageError(
            f"UI parity audit statuses lack scenario tokens: {missing_statuses}"
        )
    missing_from_audit = sorted(required_scenarios - audit_tokens)
    if missing_from_audit:
        raise CoverageError(
            f"docs/UI_PARITY_AUDIT.md lacks GPUI scenarios: {missing_from_audit}"
        )
    extra_in_audit = sorted(audit_tokens - required_scenarios)
    if extra_in_audit:
        raise CoverageError(
            f"docs/UI_PARITY_AUDIT.md contains unrecognized scenario tokens: {extra_in_audit}"
        )
    require_compact(gpui_matrix, "page", required_gpui_pages, "GPUI top pages")
    required_gpui_devices = gpui_device_names(
        root / "crates/taskmanager-gpui/src/gpui_app/root/dispatch.rs"
    )
    gpui_devices = {
        row["device"] for row in gpui_matrix if row["page"] == "performance"
    }
    missing_gpui_devices = sorted(required_gpui_devices - gpui_devices)
    if missing_gpui_devices:
        raise CoverageError(
            f"GPUI Performance left rail devices lack Niri coverage: {missing_gpui_devices}"
        )
    require_compact(
        [row for row in gpui_matrix if row["page"] == "performance"],
        "device",
        required_gpui_devices,
        "GPUI Performance devices",
    )

    iced_devices = {row["device"] for row in iced_matrix}
    required_iced_pages = iced_page_names(root / "crates/taskmanager-iced/src/capture.rs")
    missing_iced_pages = sorted((required_iced_pages - {"performance"}) - iced_devices)
    if missing_iced_pages:
        raise CoverageError(f"Iced pages lack Niri coverage: {missing_iced_pages}")
    required_iced_devices = iced_device_names(root / "crates/taskmanager-iced/src/capture.rs")
    missing_iced_devices = sorted(required_iced_devices - iced_devices)
    if missing_iced_devices:
        raise CoverageError(f"Iced Performance devices lack Niri coverage: {missing_iced_devices}")
    require_compact(
        iced_matrix,
        "device",
        (required_iced_pages - {"performance"}) | required_iced_devices,
        "Iced pages/devices",
    )
    require_recognized_column(
        iced_matrix,
        "device",
        iced_capture_router_tokens(root),
        "Iced",
    )

    required_bevy_pages = bevy_page_names(root / "crates/taskmanager-bevy-ui/src/app.rs")
    required_bevy_pages = {
        {"processes": "applications", "sessions": "users"}.get(page, page)
        for page in required_bevy_pages
    }
    bevy_pages = {row["page"] for row in bevy_matrix}
    missing_bevy_pages = sorted(required_bevy_pages - bevy_pages)
    if missing_bevy_pages:
        raise CoverageError(f"Bevy pages lack Wayland coverage: {missing_bevy_pages}")
    require_compact(bevy_matrix, "page", required_bevy_pages, "Bevy pages")
    require_recognized_column(
        bevy_matrix,
        "page",
        bevy_capture_router_tokens(root / "crates/taskmanager-bevy-ui/src/capture.rs"),
        "Bevy",
    )

    tui_matrix = read_tsv(root / "scripts/capture_tui_scenarios.tsv")
    runner_status: dict[str, str] = {}
    exemptions: list[tuple[str, str]] = []
    for frontend, (matrix_name, runner_path) in MATRIX_RUNNERS.items():
        runner = root / runner_path
        runner_source = runner.read_text(encoding="utf-8") if runner.is_file() else ""
        status, reason = runner_coverage(
            frontend, matrix_name, runner_source, DECLARATION_ONLY_EXEMPTIONS
        )
        runner_status[frontend] = status
        if reason is not None:
            exemptions.append((frontend, reason))
    for frontend, rows in [("Iced", iced_matrix), ("Bevy", bevy_matrix), ("TUI", tui_matrix)]:
        require_parity_scenarios(
            required_capture_tokens(scene_statuses, frontend.lower()), rows, frontend
        )
    tui_pages = {row["page"] for row in tui_matrix}
    required_tui_pages = {
        "applications", "performance", "services", "system", "startup", "users", "app-history"
    }
    missing_tui_pages = sorted(required_tui_pages - tui_pages)
    if missing_tui_pages:
        raise CoverageError(f"TUI pages lack coverage: {missing_tui_pages}")
    required_tui_devices = {"cpu", "memory", "disk", "network", "gpu", "battery", "fan"}
    tui_devices = {row["device"] for row in tui_matrix if row["page"] == "performance"}
    missing_tui_devices = sorted(required_tui_devices - tui_devices)
    if missing_tui_devices:
        raise CoverageError(f"TUI Performance devices lack coverage: {missing_tui_devices}")
    compact_tui = {row["page"] for row in tui_matrix if row.get("lines") == "16"}
    missing_tui_compact = sorted(required_tui_pages - compact_tui)
    if missing_tui_compact:
        raise CoverageError(f"TUI pages lack compact 54x16 coverage: {missing_tui_compact}")

    return {
        "gpui_rows": len(gpui_matrix),
        "gpui_capture_scenarios": len(required_scenarios),
        "gpui_devices": sorted(required_gpui_devices),
        "iced_rows": len(iced_matrix),
        "iced_devices": sorted(required_iced_devices),
        "bevy_rows": len(bevy_matrix),
        "bevy_pages": sorted(required_bevy_pages),
        "tui_rows": len(tui_matrix),
        "tui_devices": sorted(required_tui_devices),
        "runner_status": runner_status,
        "exemptions": exemptions,
    }


def self_test() -> int:
    required = {"process-selection", "process-tree-confirm"}
    complete = [{"name": token} for token in required]
    require_parity_scenarios(required, complete, "fixture")
    incomplete = [{"name": "process-selection"}] + [{"name": f"unrelated-{i}"} for i in range(100)]
    for rows in [incomplete, [{"name": "process-selection"}] * 100, []]:
        try:
            require_parity_scenarios(required, rows, "fixture")
        except CoverageError as error:
            if "process-tree-confirm" not in str(error):
                raise AssertionError("missing semantic scenario must be named") from error
        else:
            raise AssertionError("row count cannot replace semantic coverage")
    sample_statuses = parse_audit_scene_statuses(
        "| 分组 | GPUI 场景 Token | 场景定义 | Iced | Bevy | TUI | 差异/证据 |\n"
        "|---|---|---|:---:|:---:|:---:|---|\n"
        "| x | `about` | d | Aligned | Aligned | Unsupported[S] | [S] |\n"
        "| x | `apps` | d | AcceptedDifference[D] | Aligned | Partial[A] | [D] |\n"
    )
    if required_capture_tokens(sample_statuses, "tui") != {"apps"}:
        raise AssertionError("Unsupported/Pending scenes must not require a capture row")
    if required_capture_tokens(sample_statuses, "iced") != {"about", "apps"}:
        raise AssertionError("delivered scenes must require a capture row")
    try:
        parse_audit_scene_statuses(
            "| x | `about` | d | Aligned | Aligned | NotAStatus | - |\n"
        )
    except CoverageError as error:
        if "NotAStatus" not in str(error):
            raise AssertionError("an unknown status must be named") from error
    else:
        raise AssertionError("an unknown status must fail closed")
    recognized = {"cpu", "memory"}
    require_recognized_column([{"device": "cpu"}, {"device": "memory"}], "device", recognized, "fixture")
    try:
        require_recognized_column(
            [{"device": "cpu"}, {"device": "gpu"}], "device", recognized, "fixture"
        )
    except CoverageError as error:
        if "gpu" not in str(error):
            raise AssertionError("unrecognized column value must be named") from error
    else:
        raise AssertionError("an unrecognized capture column must fail closed")

    consumed = "MATRIX=scripts/capture_scenarios.tsv\nwhile IFS=$'\\t' read -r name; do :; done\n"
    declaration_only = "printf 'matrix=scripts/capture_tui_scenarios.tsv\\n'\n"
    if not matrix_runner_verdict("scripts/capture_scenarios.tsv", consumed):
        raise AssertionError("a runner looping the matrix must be recognized")
    if matrix_runner_verdict("scripts/capture_tui_scenarios.tsv", declaration_only):
        raise AssertionError("a declaration-only matrix must not count as consumed")
    if matrix_runner_verdict("scripts/capture_scenarios.tsv", "while IFS= read -r name; do :; done"):
        raise AssertionError("a loop over another matrix must not count as consumed")
    try:
        runner_coverage(
            "tui",
            "scripts/capture_tui_scenarios.tsv",
            declaration_only,
            {},
        )
    except CoverageError as error:
        if "declaration-only" not in str(error):
            raise AssertionError("a declaration-only matrix must be named") from error
    else:
        raise AssertionError("a declaration-only matrix must fail closed")
    status, reason = runner_coverage(
        "tui",
        "scripts/capture_tui_scenarios.tsv",
        declaration_only,
        {"tui": "runner not committed yet"},
    )
    if (status, reason) != ("exempt", "runner not committed yet"):
        raise AssertionError("an explicit declaration-only exemption must be reported")
    print("visual capture coverage self-test: PASS")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, default=Path.cwd())
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    try:
        summary = validate(args.repo_root.resolve())
    except (CoverageError, OSError, KeyError) as error:
        print(f"visual capture coverage: FAIL: {error}", file=sys.stderr)
        return 1
    runners = ", ".join(
        f"{frontend}={status}" for frontend, status in summary["runner_status"].items()
    )
    print(
        "visual capture coverage: PASS "
        f"(GPUI {summary['gpui_rows']} rows/{summary['gpui_capture_scenarios']} scenarios/"
        f"{len(summary['gpui_devices'])} performance devices; "
        f"Iced {summary['iced_rows']} rows/{len(summary['iced_devices'])} performance devices; "
        f"Bevy {summary['bevy_rows']} rows/{len(summary['bevy_pages'])} pages; "
        f"TUI {summary['tui_rows']} rows/{len(summary['tui_devices'])} performance devices; "
        f"matrix runners: {runners})"
    )
    for frontend, reason in summary["exemptions"]:
        print(
            f"visual capture coverage: WARN {frontend} matrix is declaration-only: {reason}",
            file=sys.stderr,
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
