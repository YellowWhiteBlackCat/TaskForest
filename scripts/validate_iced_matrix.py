#!/usr/bin/env python3
"""Fail-closed validator for one sequential Iced/Niri evidence matrix."""

from __future__ import annotations

import argparse
import csv
import json
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

from validate_capture_evidence import EvidenceError, file_sha256, png_receipt
from validate_tui_evidence import visual_content_receipt


EXPECTED_APP_ID = "io.github.YellowWhiteBlackCat.TaskForestI"
MATRIX_FIELDS = {"name", "device", "window_size"}
MANIFEST_FIELDS = {
    "scenario",
    "device",
    "requested_window",
    "image",
    "markers",
    "windows",
    "action",
    "app_pid",
    "window_id",
    "width",
    "height",
    "bytes",
    "sha256",
    "status",
}
REQUIRED_METADATA = {
    "run_id",
    "captured_at",
    "git_head",
    "worktree",
    "rust",
    "niri",
    "binary",
    "binary_sha256",
    "app_id",
    "capture_scope",
    "matrix",
    "scenario_count",
    "nested_output_logical",
    "source_scope",
    "source_manifest_sha256",
    "command",
}


def read_tsv(path: Path) -> list[dict[str, str]]:
    with path.open(newline="", encoding="utf-8") as stream:
        rows = list(csv.DictReader(stream, delimiter="\t"))
    if not rows:
        raise EvidenceError(f"empty TSV matrix: {path}")
    return rows


def parse_metadata(path: Path) -> dict[str, str]:
    values: dict[str, str] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        key, separator, value = line.partition("=")
        if not separator or not key or key in values:
            raise EvidenceError(f"invalid metadata line: {line!r}")
        values[key] = value
    missing = REQUIRED_METADATA - values.keys()
    if missing:
        raise EvidenceError(f"metadata missing fields: {sorted(missing)}")
    if values["app_id"] != EXPECTED_APP_ID:
        raise EvidenceError(f"unexpected Iced app id: {values['app_id']!r}")
    return values


def parse_window(value: str) -> tuple[int, int]:
    try:
        width, height = value.split("x", maxsplit=1)
        parsed = int(width), int(height)
    except ValueError as error:
        raise EvidenceError(f"invalid requested window size: {value!r}") from error
    if min(parsed) <= 0:
        raise EvidenceError(f"invalid requested window size: {value!r}")
    return parsed


def current(*command: str, cwd: Path) -> str:
    return subprocess.run(
        command, cwd=cwd, check=True, text=True, capture_output=True, timeout=15
    ).stdout.strip()


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
    if not isinstance(logical.get("width"), int) or not isinstance(
        logical.get("height"), int
    ):
        raise EvidenceError("Niri output receipt has no logical dimensions")


def validate_markers(path: Path, device: str) -> None:
    lines = [line for line in path.read_text(encoding="utf-8").splitlines() if line]
    page = (
        "services"
        if device == "service-details"
        else device
        if device in {"applications", "services", "startup", "users", "system", "app-history"}
        else "performance"
    )
    frame = f"ICED_CAPTURE_MARKER event=frame_ready mode=demo page={page}"
    target = (
        f"ICED_CAPTURE_MARKER event=target_ready mode=demo page={page} "
        f"device={device}"
    )
    if lines.count(frame) != 1:
        raise EvidenceError(f"expected one frame marker in {path}, got {lines!r}")
    if lines.count(target) != 1:
        raise EvidenceError(f"expected one target marker for {device!r} in {path}")


def validate_window(path: Path, app_pid: str, window_id: str) -> None:
    windows = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(windows, list):
        raise EvidenceError(f"window receipt is not an array: {path}")
    matches = [
        window
        for window in windows
        if isinstance(window, dict)
        and window.get("app_id") == EXPECTED_APP_ID
        and str(window.get("pid")) == app_pid
        and str(window.get("id")) == window_id
    ]
    if len(matches) != 1:
        raise EvidenceError(f"window receipt has {len(matches)} exact matches: {path}")


def validate_action(path: Path, window_id: str) -> None:
    action = path.read_text(encoding="utf-8")
    if f"window_id={window_id}" not in action:
        raise EvidenceError(f"action is not bound to window {window_id}: {path}")
    if "screenshot-window" not in action:
        raise EvidenceError(f"action receipt is missing screenshot-window: {path}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--matrix", type=Path, required=True)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--run-dir", type=Path, required=True)
    parser.add_argument("--metadata", type=Path, required=True)
    parser.add_argument("--source-manifest", type=Path, required=True)
    parser.add_argument("--niri-outputs", type=Path, required=True)
    parser.add_argument("--receipt", type=Path, required=True)
    parser.add_argument("--repo-root", type=Path, required=True)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--current-worktree", action="store_true")
    args = parser.parse_args()

    try:
        matrix = read_tsv(args.matrix)
        if set(matrix[0]) != MATRIX_FIELDS:
            raise EvidenceError(f"unexpected matrix columns: {sorted(matrix[0])}")
        manifest = read_tsv(args.manifest)
        if set(manifest[0]) != MANIFEST_FIELDS:
            raise EvidenceError(f"unexpected manifest columns: {sorted(manifest[0])}")
        values = parse_metadata(args.metadata)
        if values["source_scope"] != "iced":
            raise EvidenceError(f"unexpected source scope: {values['source_scope']!r}")
        if int(values["scenario_count"]) != len(matrix):
            raise EvidenceError("metadata scenario count does not match matrix")
        if len(manifest) != len(matrix):
            raise EvidenceError("manifest is not a complete matrix")
        if [row["scenario"] for row in manifest] != [row["name"] for row in matrix]:
            raise EvidenceError("manifest scenario order differs from matrix")
        if [row["device"] for row in manifest] != [row["device"] for row in matrix]:
            raise EvidenceError("manifest device order differs from matrix")

        validate_niri_outputs(args.niri_outputs)
        validate_source_manifest(args.source_manifest, args.repo_root)
        if file_sha256(args.source_manifest) != values["source_manifest_sha256"]:
            raise EvidenceError("source manifest hash differs from metadata")
        if file_sha256(args.binary) != values["binary_sha256"]:
            raise EvidenceError("captured binary hash differs from metadata")

        artifacts: list[dict[str, object]] = []
        for expected, row in zip(matrix, manifest, strict=True):
            if set(row) != MANIFEST_FIELDS:
                raise EvidenceError(f"unexpected manifest fields for {row.get('scenario')}")
            if row["status"] != "ok":
                raise EvidenceError(f"scenario is not accepted: {row['scenario']}")
            if row["device"] != expected["device"] or row["requested_window"] != expected["window_size"]:
                raise EvidenceError(f"manifest identity mismatch: {row['scenario']}")
            requested_width, requested_height = parse_window(expected["window_size"])
            run_dir = args.run_dir.resolve()
            image = (run_dir / row["image"]).resolve()
            for path_value, label in (
                (image, "image"),
                ((run_dir / row["markers"]).resolve(), "markers"),
                ((run_dir / row["windows"]).resolve(), "windows"),
                ((run_dir / row["action"]).resolve(), "action"),
            ):
                try:
                    path_value.relative_to(run_dir)
                except ValueError as error:
                    raise EvidenceError(f"{label} escapes run directory") from error
                if not path_value.is_file():
                    raise EvidenceError(f"missing {label} for {row['scenario']}: {path_value}")

            image_receipt = png_receipt(image)
            visual = visual_content_receipt(image)
            if image_receipt.width < requested_width or image_receipt.height < requested_height:
                raise EvidenceError(
                    f"{row['scenario']} image {image_receipt.width}x{image_receipt.height} "
                    f"is smaller than requested {requested_width}x{requested_height}"
                )
            if visual.visible_pixels < image_receipt.width * image_receipt.height // 4:
                raise EvidenceError(f"{row['scenario']} image is mostly transparent")
            if str(image_receipt.width) != row["width"] or str(image_receipt.height) != row["height"]:
                raise EvidenceError(f"manifest dimensions differ for {row['scenario']}")
            if str(image_receipt.size) != row["bytes"] or image_receipt.sha256 != row["sha256"]:
                raise EvidenceError(f"manifest image receipt differs for {row['scenario']}")
            validate_markers(run_dir / row["markers"], row["device"])
            validate_window(run_dir / row["windows"], row["app_pid"], row["window_id"])
            validate_action(run_dir / row["action"], row["window_id"])
            artifacts.append(
                {
                    "scenario": row["scenario"],
                    "device": row["device"],
                    "requested_window": row["requested_window"],
                    "image": row["image"],
                    "width": image_receipt.width,
                    "height": image_receipt.height,
                    "bytes": image_receipt.size,
                    "sha256": image_receipt.sha256,
                    "visible_pixels": visual.visible_pixels,
                    "unique_colors": visual.unique_colors,
                    "luminance_span": visual.luminance_span,
                }
            )

        if args.current_worktree:
            root = args.repo_root.resolve()
            head = current("git", "rev-parse", "--short=12", "HEAD", cwd=root)
            rust = current("rustc", "-V", cwd=root)
            state = "dirty" if current("git", "status", "--porcelain", cwd=root) else "clean"
            if (head, rust, state) != (values["git_head"], values["rust"], values["worktree"]):
                raise EvidenceError("capture provenance differs from current worktree")

        payload = {
            "schema_version": 1,
            "status": "pass",
            "validated_at": datetime.now(timezone.utc).isoformat(timespec="seconds"),
            "run_id": values["run_id"],
            "git_head": values["git_head"],
            "worktree": values["worktree"],
            "rust": values["rust"],
            "niri": values["niri"],
            "binary_sha256": values["binary_sha256"],
            "source_scope": values["source_scope"],
            "source_manifest_sha256": values["source_manifest_sha256"],
            "scenario_count": len(artifacts),
            "artifacts": artifacts,
        }
        args.receipt.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
        print(f"Iced matrix validation: PASS ({len(artifacts)} sequential scenarios)")
        return 0
    except (
        EvidenceError,
        OSError,
        ValueError,
        subprocess.CalledProcessError,
        json.JSONDecodeError,
    ) as error:
        print(f"Iced matrix validation: FAIL: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
