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

    tui_matrix = read_tsv(root / "scripts/capture_tui_scenarios.tsv")
    for frontend, rows in [("Iced", iced_matrix), ("Bevy", bevy_matrix), ("TUI", tui_matrix)]:
        require_parity_scenarios(audit_tokens, rows, frontend)
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
    print(
        "visual capture coverage: PASS "
        f"(GPUI {summary['gpui_rows']} rows/{summary['gpui_capture_scenarios']} scenarios/"
        f"{len(summary['gpui_devices'])} performance devices; "
        f"Iced {summary['iced_rows']} rows/{len(summary['iced_devices'])} performance devices; "
        f"Bevy {summary['bevy_rows']} rows/{len(summary['bevy_pages'])} pages; "
        f"TUI {summary['tui_rows']} rows/{len(summary['tui_devices'])} performance devices)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
