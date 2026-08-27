#!/usr/bin/env python3
"""Fail-closed validator for the Bevy Wayland/Niri capture matrix."""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import subprocess
import sys
from pathlib import Path

from validate_capture_evidence import EvidenceError, file_sha256, png_receipt
from validate_tui_evidence import visual_content_receipt

EXPECTED_APP_ID = "io.github.YellowWhiteBlackCat.TaskForestB"
MATRIX_FIELDS = {"name", "page", "window_size"}
MANIFEST_FIELDS = {
    "scenario", "page", "requested_window", "image", "markers", "windows",
    "action", "app_pid", "window_id", "width", "height", "bytes", "sha256", "status",
}


def read_tsv(path: Path) -> list[dict[str, str]]:
    with path.open(newline="", encoding="utf-8") as stream:
        rows = list(csv.DictReader(stream, delimiter="\t"))
    if not rows:
        raise EvidenceError(f"empty TSV: {path}")
    return rows


def parse_metadata(path: Path) -> dict[str, str]:
    values: dict[str, str] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        key, separator, value = line.partition("=")
        if not separator or not key or key in values:
            raise EvidenceError(f"invalid metadata line: {line!r}")
        values[key] = value
    required = {
        "run_id", "captured_at", "git_head", "worktree_sha256", "rust", "binary",
        "binary_sha256", "app_id", "capture_backend", "matrix", "scenario_count",
        "source_scope", "source_manifest_sha256", "command",
    }
    missing = required - values.keys()
    if missing:
        raise EvidenceError(f"metadata missing fields: {sorted(missing)}")
    if values["app_id"] != EXPECTED_APP_ID:
        raise EvidenceError(f"unexpected Bevy app id: {values['app_id']!r}")
    if values["source_scope"] != "bevy":
        raise EvidenceError(f"unexpected source scope: {values['source_scope']!r}")
    return values


def parse_window(value: str) -> tuple[int, int]:
    try:
        width, height = (int(part) for part in value.split("x", maxsplit=1))
    except ValueError as error:
        raise EvidenceError(f"invalid window size: {value!r}") from error
    if min(width, height) <= 0:
        raise EvidenceError(f"invalid window size: {value!r}")
    return width, height


def validate_source_manifest(path: Path, root: Path) -> None:
    root = root.resolve()
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        digest, separator, relative = line.partition("  ")
        if not separator or len(digest) != 64 or not relative:
            raise EvidenceError(f"invalid source manifest line {number}")
        source = (root / relative).resolve()
        try:
            source.relative_to(root)
        except ValueError as error:
            raise EvidenceError(f"source path escapes repository: {relative}") from error
        if not source.is_file() or file_sha256(source) != digest:
            raise EvidenceError(f"source provenance mismatch: {relative}")


def validate_window_receipt(path: Path, app_pid: str, window_id: str) -> None:
    windows = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(windows, list):
        raise EvidenceError(f"window receipt is not an array: {path}")
    matches = [
        item for item in windows
        if isinstance(item, dict)
        and item.get("app_id") == EXPECTED_APP_ID
        and str(item.get("pid")) == app_pid
        and str(item.get("id")) == window_id
    ]
    if len(matches) != 1:
        raise EvidenceError(f"window receipt exact match count is {len(matches)}: {path}")


def validate_niri_outputs(path: Path) -> None:
    outputs = json.loads(path.read_text(encoding="utf-8"))
    if isinstance(outputs, dict):
        output = outputs.get("winit")
    elif isinstance(outputs, list) and len(outputs) == 1:
        output = outputs[0]
    else:
        output = None
    if not isinstance(output, dict) or output.get("name") != "winit":
        raise EvidenceError("Niri output receipt does not identify winit")
    logical = output.get("logical")
    if not isinstance(logical, dict) or logical.get("scale") != 1:
        raise EvidenceError("Niri output receipt is not scale 1")


def validate_markers(path: Path, page: str) -> None:
    lines = [line for line in path.read_text(encoding="utf-8").splitlines() if line]
    frame = f"BEVY_CAPTURE_MARKER event=frame_ready mode=demo page={page}"
    target = f"BEVY_CAPTURE_MARKER event=target_ready mode=demo page={page}"
    if lines.count(frame) != 1 or lines.count(target) != 1:
        raise EvidenceError(f"marker pair is not exact for {page}: {lines!r}")


def validate_action(path: Path, window_id: str) -> None:
    text = path.read_text(encoding="utf-8")
    if "screenshot-window" not in text or window_id not in text:
        raise EvidenceError(f"screenshot action is not bound to window {window_id}: {path}")


def validate_current_worktree(root: Path, values: dict[str, str]) -> None:
    head = subprocess.run(
        ["git", "rev-parse", "HEAD"], cwd=root, check=True, text=True,
        capture_output=True, timeout=15,
    ).stdout.strip()
    status = subprocess.run(
        ["git", "status", "--short"], cwd=root, check=True, text=True,
        capture_output=True, timeout=15,
    ).stdout.encode()
    if head != values["git_head"]:
        raise EvidenceError("capture git head differs from current worktree")
    if hashlib.sha256(status).hexdigest() != values["worktree_sha256"]:
        raise EvidenceError("capture worktree status differs from current worktree")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--matrix", type=Path)
    parser.add_argument("--manifest", type=Path)
    parser.add_argument("--run-dir", type=Path)
    parser.add_argument("--metadata", type=Path)
    parser.add_argument("--source-manifest", type=Path)
    parser.add_argument("--niri-outputs", type=Path)
    parser.add_argument("--receipt", type=Path)
    parser.add_argument("--repo-root", type=Path)
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--current-worktree", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        assert parse_window("720x480") == (720, 480)
        try:
            parse_window("0x480")
        except EvidenceError:
            print("Bevy capture validator self-test: PASS")
            return 0
        print("Bevy capture validator self-test: FAIL", file=sys.stderr)
        return 1
    required_args = (
        args.matrix, args.manifest, args.run_dir, args.metadata, args.source_manifest,
        args.niri_outputs, args.receipt, args.repo_root, args.binary,
    )
    if any(value is None for value in required_args):
        parser.error("all capture paths are required unless --self-test is used")
    try:
        matrix = read_tsv(args.matrix)
        if set(matrix[0]) != MATRIX_FIELDS:
            raise EvidenceError(f"unexpected matrix fields: {sorted(matrix[0])}")
        manifest = read_tsv(args.manifest)
        if set(manifest[0]) != MANIFEST_FIELDS:
            raise EvidenceError(f"unexpected manifest fields: {sorted(manifest[0])}")
        values = parse_metadata(args.metadata)
        validate_niri_outputs(args.niri_outputs)
        if int(values["scenario_count"]) != len(matrix) or len(manifest) != len(matrix):
            raise EvidenceError("scenario count or manifest completeness mismatch")
        if [row["scenario"] for row in manifest] != [row["name"] for row in matrix]:
            raise EvidenceError("manifest order differs from matrix")
        validate_source_manifest(args.source_manifest, args.repo_root)
        if file_sha256(args.source_manifest) != values["source_manifest_sha256"]:
            raise EvidenceError("source manifest hash differs from metadata")
        if file_sha256(args.binary) != values["binary_sha256"]:
            raise EvidenceError("binary hash differs from metadata")
        if args.current_worktree:
            validate_current_worktree(args.repo_root, values)
        receipt: list[dict[str, object]] = []
        run_dir = args.run_dir.resolve()
        for expected, row in zip(matrix, manifest, strict=True):
            if set(row) != MANIFEST_FIELDS or row["status"] != "ok":
                raise EvidenceError(f"scenario is not accepted: {row.get('scenario')}")
            if row["page"] != expected["page"] or row["requested_window"] != expected["window_size"]:
                raise EvidenceError(f"scenario identity mismatch: {row['scenario']}")
            requested_width, requested_height = parse_window(expected["window_size"])
            paths = {
                key: (run_dir / row[key]).resolve()
                for key in ("image", "markers", "windows", "action")
            }
            for key, path in paths.items():
                try:
                    path.relative_to(run_dir)
                except ValueError as error:
                    raise EvidenceError(f"{key} escapes run dir") from error
                if not path.is_file():
                    raise EvidenceError(f"missing {key}: {path}")
            image = png_receipt(paths["image"])
            visual = visual_content_receipt(paths["image"])
            if image.width < requested_width or image.height < requested_height:
                raise EvidenceError(f"image is smaller than requested: {row['scenario']}")
            if visual.visible_pixels < image.width * image.height // 4:
                raise EvidenceError(f"image is mostly transparent: {row['scenario']}")
            if str(image.width) != row["width"] or str(image.height) != row["height"]:
                raise EvidenceError(f"image dimensions differ: {row['scenario']}")
            if str(image.size) != row["bytes"] or image.sha256 != row["sha256"]:
                raise EvidenceError(f"image hash/size differs: {row['scenario']}")
            validate_markers(paths["markers"], row["page"])
            validate_window_receipt(paths["windows"], row["app_pid"], row["window_id"])
            validate_action(paths["action"], row["window_id"])
            receipt.append({
                "scenario": row["scenario"], "page": row["page"],
                "image": row["image"], "width": image.width,
                "height": image.height, "sha256": image.sha256,
            })
        args.receipt.write_text(
            json.dumps({"frontend": "bevy", "app_id": EXPECTED_APP_ID,
                        "scenario_count": len(receipt), "artifacts": receipt}, indent=2)
            + "\n",
            encoding="utf-8",
        )
        print(f"Bevy capture validation: PASS ({len(receipt)} scenarios) -> {args.receipt}")
        return 0
    except (EvidenceError, OSError, ValueError, json.JSONDecodeError, subprocess.SubprocessError) as error:
        print(f"Bevy capture validation: FAIL: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
