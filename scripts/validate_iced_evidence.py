#!/usr/bin/env python3
"""Fail-closed validator for one real Iced/Niri pixel evidence bundle."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

from validate_capture_evidence import EvidenceError, file_sha256, png_receipt
from validate_tui_evidence import visual_content_receipt


EXPECTED_APP_ID = "io.github.YellowWhiteBlackCat.TaskForestI"
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
    "capture_device",
    "stack",
    "nested_output_logical",
    "window_id",
    "source_scope",
    "source_manifest_sha256",
    "command",
}


def metadata(path: Path) -> dict[str, str]:
    values: dict[str, str] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        key, separator, value = line.partition("=")
        if not separator or not key or key in values:
            raise EvidenceError(f"invalid metadata line: {line!r}")
        values[key] = value
    if missing := REQUIRED_METADATA - values.keys():
        raise EvidenceError(f"metadata missing fields: {sorted(missing)}")
    if values["app_id"] != EXPECTED_APP_ID:
        raise EvidenceError(f"unexpected Iced app id: {values['app_id']!r}")
    if values["source_scope"] != "iced":
        raise EvidenceError(f"unexpected source scope: {values['source_scope']!r}")
    if values["capture_device"] not in {
        "cpu",
        "memory",
        "disk",
        "network",
        "gpu",
        "battery",
        "fan",
    }:
        raise EvidenceError(f"unexpected Iced capture device: {values['capture_device']!r}")
    return values


def current(*command: str, cwd: Path) -> str:
    return subprocess.run(
        command, cwd=cwd, check=True, text=True, capture_output=True, timeout=15
    ).stdout.strip()


def validate_source_manifest(path: Path, root: Path) -> None:
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        digest, separator, relative = line.partition("  ")
        if not separator or len(digest) != 64 or not relative:
            raise EvidenceError(f"invalid source manifest line {number}")
        source = (root / relative).resolve()
        try:
            source.relative_to(root.resolve())
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


def validate_window(path: Path, values: dict[str, str], app_pid: str | None) -> None:
    windows = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(windows, list):
        raise EvidenceError("Niri window receipt is not an array")
    matches = [
        window
        for window in windows
        if isinstance(window, dict)
        and window.get("app_id") == EXPECTED_APP_ID
        and str(window.get("id")) == values["window_id"]
        and (app_pid is None or str(window.get("pid")) == app_pid)
    ]
    if len(matches) != 1:
        raise EvidenceError(
            f"Niri window receipt has {len(matches)} exact app/window matches"
        )


def validate_markers(path: Path) -> None:
    lines = [line for line in path.read_text(encoding="utf-8").splitlines() if line]
    expected = "ICED_CAPTURE_MARKER event=frame_ready mode=demo page=performance"
    if lines.count(expected) != 1:
        raise EvidenceError(f"expected exactly one first-present marker, got {lines!r}")


def validate_target_marker(path: Path, device: str) -> None:
    lines = [line for line in path.read_text(encoding="utf-8").splitlines() if line]
    expected = (
        "ICED_CAPTURE_MARKER event=target_ready mode=demo page=performance "
        f"device={device}"
    )
    if lines.count(expected) != 1:
        raise EvidenceError(f"expected exactly one target marker for {device!r}, got {lines!r}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", type=Path, required=True)
    parser.add_argument("--metadata", type=Path, required=True)
    parser.add_argument("--markers", type=Path, required=True)
    parser.add_argument("--source-manifest", type=Path, required=True)
    parser.add_argument("--niri-outputs", type=Path, required=True)
    parser.add_argument("--windows", type=Path, required=True)
    parser.add_argument("--action-log", type=Path, required=True)
    parser.add_argument("--receipt", type=Path, required=True)
    parser.add_argument("--repo-root", type=Path, required=True)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--current-worktree", action="store_true")
    args = parser.parse_args()
    try:
        values = metadata(args.metadata)
        image = png_receipt(args.image)
        visual = visual_content_receipt(args.image)
        if image.width < 800 or image.height < 500:
            raise EvidenceError(f"Iced image is too small: {image.width}x{image.height}")
        if visual.visible_pixels < image.width * image.height // 4:
            raise EvidenceError("Iced image is mostly transparent")
        validate_markers(args.markers)
        validate_target_marker(args.markers, values["capture_device"])
        validate_niri_outputs(args.niri_outputs)
        validate_source_manifest(args.source_manifest, args.repo_root.resolve())
        if file_sha256(args.source_manifest) != values["source_manifest_sha256"]:
            raise EvidenceError("source manifest hash differs from metadata")
        if file_sha256(args.binary) != values["binary_sha256"]:
            raise EvidenceError("captured binary hash differs from metadata")
        action_log = args.action_log.read_text(encoding="utf-8")
        if f"window_id={values['window_id']}" not in action_log:
            raise EvidenceError("screenshot action is not bound to the recorded window id")
        if "screenshot-window" not in action_log:
            raise EvidenceError("screenshot action receipt is missing screenshot-window")
        app_pid = None
        for line in action_log.splitlines():
            if line.startswith("app_pid="):
                app_pid = line.partition("=")[2]
                break
        validate_window(args.windows, values, app_pid)

        if args.current_worktree:
            root = args.repo_root.resolve()
            head = current("git", "rev-parse", "--short=12", "HEAD", cwd=root)
            rust = current("rustc", "-V", cwd=root)
            state = "dirty" if current("git", "status", "--porcelain", cwd=root) else "clean"
            if (head, rust, state) != (
                values["git_head"],
                values["rust"],
                values["worktree"],
            ):
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
            "artifact": {
                "image": args.image.name,
                "width": image.width,
                "height": image.height,
                "bytes": image.size,
                "sha256": image.sha256,
                "visible_pixels": visual.visible_pixels,
                "unique_colors": visual.unique_colors,
                "luminance_span": visual.luminance_span,
            },
        }
        args.receipt.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
        print(f"Iced evidence validation: PASS ({image.width}x{image.height})")
        return 0
    except (
        EvidenceError,
        OSError,
        ValueError,
        subprocess.CalledProcessError,
        json.JSONDecodeError,
    ) as error:
        print(f"Iced evidence validation: FAIL: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
