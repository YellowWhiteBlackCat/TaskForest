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

    return {
        "gpui_rows": len(gpui_matrix),
        "gpui_capture_scenarios": len(required_scenarios),
        "gpui_devices": sorted(required_gpui_devices),
        "iced_rows": len(iced_matrix),
        "iced_devices": sorted(required_iced_devices),
        "bevy_rows": len(bevy_matrix),
        "bevy_pages": sorted(required_bevy_pages),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, default=Path.cwd())
    args = parser.parse_args()
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
        f"Bevy {summary['bevy_rows']} rows/{len(summary['bevy_pages'])} pages)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
