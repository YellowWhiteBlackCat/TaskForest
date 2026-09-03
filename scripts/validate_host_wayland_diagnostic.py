#!/usr/bin/env python3
"""Fail-closed validator for a non-publishing host-Wayland GPUI diagnostic.

This validator deliberately produces *diagnostic-only* receipts.  A passing
receipt proves that one current-build active-window capture was internally
consistent; it never makes the image part of the durable parity matrix.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import re
import struct
import subprocess
import sys
import tempfile
import zlib
from collections import Counter
from dataclasses import dataclass
from decimal import Decimal, InvalidOperation
from pathlib import Path

try:
    from capture_supervisor import SupervisorError, validate_supervised_metadata
    from validate_capture_evidence import (
        EvidenceError,
        file_sha256,
        png_chunk,
        png_receipt,
        validate_source_manifest,
    )
except ModuleNotFoundError:  # Allow direct library-style imports from the repository root.
    from scripts.capture_supervisor import (  # type: ignore[no-redef]
        SupervisorError,
        validate_supervised_metadata,
    )
    from scripts.validate_capture_evidence import (  # type: ignore[no-redef]
        EvidenceError,
        file_sha256,
        png_chunk,
        png_receipt,
        validate_source_manifest,
    )


PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"
EXPECTED_SCALE = Decimal("1")
SKELETON_TEXT = (
    "collecting telemetry",
    "loading telemetry",
    "initializing telemetry",
)


@dataclass(frozen=True)
class VisualReceipt:
    visible_pixels: int
    sampled_pixels: int
    unique_colors: int
    luminance_span: int
    dominant_fraction: float


def read_metadata(path: Path) -> dict[str, str]:
    values: dict[str, str] = {}
    for raw_line in path.read_text(encoding="utf-8").splitlines():
        key, separator, value = raw_line.partition("=")
        if not separator or not key or key in values:
            raise EvidenceError(f"invalid metadata line: {raw_line!r}")
        values[key] = value
    required = {
        "schema_version",
        "run_id",
        "run_uuid",
        "frontend",
        "run_root",
        "runtime_root",
        "supervisor_pid",
        "cgroup_path",
        "dbus_address_sha256",
        "captured_at",
        "git_head",
        "worktree",
        "rust",
        "binary",
        "binary_sha256",
        "build_command",
        "build_status",
        "app_pid",
        "app_pid_start_time",
        "app_exe",
        "app_pid_exe_verified",
        "scenario",
        "theme",
        "expected_logical_size",
        "display_scale",
        "display_geometry",
        "display_mode",
        "capture_backend",
        "window_identity",
        "runtime_isolation",
        "dbus_isolation",
        "kwin_pid",
        "kwin_pid_start_time",
        "kwin_runtime",
        "kwin_socket",
        "tmpdir",
        "cargo_target_dir",
        "image",
        "image_width",
        "image_height",
        "image_bytes",
        "image_sha256",
        "log",
        "log_sha256",
        "markers",
        "markers_sha256",
        "parity_evidence",
        "durable_output",
    }
    if missing := required - values.keys():
        raise EvidenceError(f"metadata missing fields: {sorted(missing)}")
    return values


def safe_child(root: Path, relative: str) -> Path:
    candidate = (root / relative).resolve()
    try:
        candidate.relative_to(root.resolve())
    except ValueError as error:
        raise EvidenceError(f"artifact escapes diagnostic root: {relative}") from error
    if Path(relative).is_absolute():
        raise EvidenceError(f"diagnostic artifact must be relative: {relative}")
    return candidate


def parse_size(value: str) -> tuple[int, int]:
    match = re.fullmatch(r"([1-9][0-9]*)x([1-9][0-9]*)", value)
    if match is None:
        raise EvidenceError(f"invalid logical size: {value!r}")
    return int(match.group(1)), int(match.group(2))


def parse_scale(value: str) -> Decimal:
    try:
        scale = Decimal(value)
    except InvalidOperation as error:
        raise EvidenceError(f"invalid display scale: {value!r}") from error
    if not scale.is_finite() or scale <= 0:
        raise EvidenceError(f"invalid display scale: {value!r}")
    return scale


def scaled_size(logical: tuple[int, int], scale: Decimal) -> tuple[int, int]:
    physical = tuple(Decimal(part) * scale for part in logical)
    if any(value != value.to_integral_value() for value in physical):
        raise EvidenceError(f"scale does not produce integral pixels: {scale}")
    return int(physical[0]), int(physical[1])


def current(*command: str, cwd: Path) -> str:
    result = subprocess.run(
        command,
        cwd=cwd,
        check=True,
        text=True,
        capture_output=True,
        timeout=15,
    )
    return result.stdout.strip()


def _png_scanlines(path: Path) -> tuple[int, int, int, bytes]:
    data = path.read_bytes()
    if not data.startswith(PNG_SIGNATURE):
        raise EvidenceError(f"invalid PNG signature: {path}")
    offset = len(PNG_SIGNATURE)
    width = height = bit_depth = color_type = interlace = 0
    compressed = bytearray()
    while offset < len(data):
        if offset + 12 > len(data):
            raise EvidenceError(f"truncated PNG chunk: {path}")
        length = struct.unpack(">I", data[offset : offset + 4])[0]
        kind = data[offset + 4 : offset + 8]
        payload_start = offset + 8
        payload_end = payload_start + length
        if payload_end + 4 > len(data):
            raise EvidenceError(f"truncated PNG payload: {path}")
        payload = data[payload_start:payload_end]
        if kind == b"IHDR":
            if length != 13:
                raise EvidenceError(f"invalid PNG IHDR: {path}")
            width, height, bit_depth, color_type, _, _, interlace = struct.unpack(
                ">IIBBBBB", payload
            )
        elif kind == b"IDAT":
            compressed.extend(payload)
        elif kind == b"IEND":
            break
        offset = payload_end + 4
    if bit_depth != 8 or color_type not in (2, 6) or interlace != 0:
        raise EvidenceError(
            "diagnostic visual check requires non-interlaced 8-bit RGB/RGBA PNG"
        )
    channels = 4 if color_type == 6 else 3
    row_bytes = width * channels
    raw = zlib.decompress(compressed)
    if len(raw) != height * (row_bytes + 1):
        raise EvidenceError("PNG scanline payload length differs from IHDR")
    return width, height, channels, raw


def visual_receipt(path: Path) -> VisualReceipt:
    """Reject empty/uniform frames before any diagnostic can be considered."""
    width, height, channels, raw = _png_scanlines(path)
    total = width * height
    step = max(1, math.isqrt(total // 500_000) if total > 500_000 else 1)
    sampled_pixels = ((width + step - 1) // step) * ((height + step - 1) // step)
    previous = bytearray(width * channels)
    position = 0
    visible_pixels = 0
    colors: set[tuple[int, int, int]] = set()
    buckets: Counter[tuple[int, int, int]] = Counter()
    minimum_luminance = 255
    maximum_luminance = 0

    for row in range(height):
        filter_kind = raw[position]
        position += 1
        encoded = raw[position : position + width * channels]
        position += width * channels
        decoded = bytearray(width * channels)
        for index, value in enumerate(encoded):
            left = decoded[index - channels] if index >= channels else 0
            above = previous[index]
            upper_left = previous[index - channels] if index >= channels else 0
            if filter_kind == 0:
                predictor = 0
            elif filter_kind == 1:
                predictor = left
            elif filter_kind == 2:
                predictor = above
            elif filter_kind == 3:
                predictor = (left + above) // 2
            elif filter_kind == 4:
                estimate = left + above - upper_left
                left_distance = abs(estimate - left)
                above_distance = abs(estimate - above)
                upper_left_distance = abs(estimate - upper_left)
                if left_distance <= above_distance and left_distance <= upper_left_distance:
                    predictor = left
                elif above_distance <= upper_left_distance:
                    predictor = above
                else:
                    predictor = upper_left
            else:
                raise EvidenceError(f"unsupported PNG row filter: {filter_kind}")
            decoded[index] = (value + predictor) & 0xFF
        previous = decoded

        if row % step == 0:
            for column in range(0, width, step):
                index = column * channels
                red, green, blue = decoded[index : index + 3]
                alpha = decoded[index + 3] if channels == 4 else 255
                if alpha <= 16:
                    continue
                visible_pixels += 1
                rgb = (red, green, blue)
                if len(colors) < 2048:
                    colors.add(rgb)
                buckets[(red // 8, green // 8, blue // 8)] += 1
                luminance = (54 * red + 183 * green + 19 * blue) // 256
                minimum_luminance = min(minimum_luminance, luminance)
                maximum_luminance = max(maximum_luminance, luminance)

    if visible_pixels < sampled_pixels // 4:
        raise EvidenceError(f"diagnostic PNG is mostly transparent: {visible_pixels}")
    if not colors or len(colors) < 16 or maximum_luminance - minimum_luminance < 24:
        raise EvidenceError("diagnostic PNG lacks visible content variation")
    dominant_fraction = max(buckets.values()) / visible_pixels
    if dominant_fraction >= 0.94 and len(colors) < 128:
        raise EvidenceError(
            "diagnostic PNG is a near-uniform frame; skeleton/blank frame rejected"
        )
    return VisualReceipt(
        visible_pixels,
        sampled_pixels,
        len(colors),
        maximum_luminance - minimum_luminance,
        dominant_fraction,
    )


def normalise_ocr(text: str) -> str:
    return re.sub(r"[^a-z0-9]+", " ", text.lower()).strip()


def reject_skeleton_text(text: str) -> None:
    normalised = normalise_ocr(text)
    for phrase in SKELETON_TEXT:
        if phrase in normalised:
            raise EvidenceError(f"skeleton text detected by OCR: {phrase!r}")


def ocr_text(path: Path) -> str:
    result = subprocess.run(
        ["tesseract", str(path), "stdout", "--psm", "11"],
        check=True,
        text=True,
        capture_output=True,
        timeout=15,
    )
    return result.stdout


def require_relative_to(path: Path, root: Path, label: str) -> Path:
    resolved = path.resolve()
    try:
        resolved.relative_to(root.resolve())
    except ValueError as error:
        raise EvidenceError(f"{label} escapes repository: {path}") from error
    return resolved


def validate(args: argparse.Namespace) -> dict[str, object]:
    root = args.repo_root.resolve()
    run_dir = args.run_dir.resolve()
    metadata = read_metadata(args.metadata)
    try:
        validate_supervised_metadata(metadata, root, args.metadata, "host-wayland")
    except SupervisorError as error:
        raise EvidenceError(str(error)) from error
    logical_size = parse_size(args.logical_size)
    if metadata["schema_version"] != "1":
        raise EvidenceError("unsupported host diagnostic schema")
    if metadata["expected_logical_size"] != args.logical_size:
        raise EvidenceError("metadata logical size differs from requested size")
    if metadata["scenario"] != args.scenario or metadata["theme"] != args.theme:
        raise EvidenceError("metadata scenario/theme differs from requested state")
    if metadata["parity_evidence"] != "false":
        raise EvidenceError("host diagnostic may never claim parity_evidence=true")
    if metadata["durable_output"] != "none":
        raise EvidenceError("host diagnostic must not declare a durable output")
    if metadata["capture_backend"] != "spectacle-active-window":
        raise EvidenceError("unexpected host capture backend")
    private_kwin = metadata["runtime_isolation"] == "private-kwin-wayland"
    if private_kwin:
        if metadata["window_identity"] != "kwin-script-stacking-order":
            raise EvidenceError("private KWin capture lacks its exact window identity mode")
    elif metadata["window_identity"] != "active-window-selector-unverified":
        raise EvidenceError("host window identity is not explicitly unverified")
    if not private_kwin and metadata["runtime_isolation"] != "host-wayland-target-only":
        raise EvidenceError("unexpected runtime isolation declaration")
    if metadata["build_status"] != "success" or metadata["build_command"] != "cargo build --locked --quiet -p taskmanager-gpui --bin taskforest-g":
        raise EvidenceError("diagnostic lacks the required locked current-build receipt")
    if metadata["app_pid_exe_verified"] != "true":
        raise EvidenceError("app PID executable was not verified before capture")
    if metadata["runtime_isolation"] == "private-kwin-wayland":
        for field in ("kwin_pid", "kwin_pid_start_time"):
            if not metadata[field].isdigit() or int(metadata[field]) <= 0:
                raise EvidenceError(f"invalid private KWin receipt: {field}")
        if not metadata["kwin_runtime"].startswith("/tmp/taskforest-capture-host-wayland-"):
            raise EvidenceError("private KWin runtime is not run-owned")
        if not metadata["kwin_socket"].startswith(metadata["kwin_runtime"] + "/"):
            raise EvidenceError("private KWin socket escaped its runtime")
    for field in ("app_pid", "app_pid_start_time"):
        if not metadata[field].isdigit() or int(metadata[field]) <= 0:
            raise EvidenceError(f"invalid exact PID receipt: {field}")
    if len(metadata["binary_sha256"]) != 64 or not re.fullmatch(r"[0-9a-f]{64}", metadata["binary_sha256"]):
        raise EvidenceError("invalid current binary hash")

    binary = require_relative_to(root / metadata["binary"], root, "binary")
    expected_binary = (run_dir / "bin" / "taskforest-g").resolve()
    if binary != expected_binary:
        raise EvidenceError(f"diagnostic binary is not the run-owned target binary: {binary}")
    if metadata["app_exe"] != str(binary):
        raise EvidenceError("app PID executable path differs from current binary")
    if file_sha256(binary) != metadata["binary_sha256"]:
        raise EvidenceError("current binary hash differs from capture receipt")

    target = require_relative_to(Path(metadata["cargo_target_dir"]), root, "Cargo target")
    if target != (root / "target").resolve():
        raise EvidenceError("Cargo target is not the shared repository target")
    tmpdir = require_relative_to(Path(metadata["tmpdir"]), root, "TMPDIR")
    if not tmpdir.is_relative_to(root / ".tmp" / "agent-runs"):
        raise EvidenceError("TMPDIR is not under the repository agent-run scratch area")
    if str(tmpdir).startswith("/tmp"):
        raise EvidenceError("TMPDIR points at shared /tmp")

    if private_kwin:
        if metadata.get("dbus_isolation") != "private-session":
            raise EvidenceError("private KWin capture must use a private D-Bus session")
        for field in (
            "source_manifest",
            "source_manifest_sha256",
            "window_info",
            "window_info_sha256",
        ):
            if not metadata.get(field):
                raise EvidenceError(f"private capture metadata is missing {field}")
        source_manifest = require_relative_to(
            root / metadata["source_manifest"], root, "source manifest"
        )
        if file_sha256(source_manifest) != metadata["source_manifest_sha256"]:
            raise EvidenceError("GPUI source manifest hash differs from metadata")
        validate_source_manifest(source_manifest, root)
        window_info = safe_child(run_dir, metadata["window_info"])
        if file_sha256(window_info) != metadata["window_info_sha256"]:
            raise EvidenceError("private KWin window-info receipt hash drifted")
        window_info_text = window_info.read_text(encoding="utf-8")
        if (
            metadata["app_pid"] not in window_info_text
            or "TaskForestG" not in window_info_text
            or "active=true" not in window_info_text
        ):
            raise EvidenceError("private KWin window-info receipt is not bound to TaskForestG")

    image = safe_child(run_dir, metadata["image"])
    if image != args.image.resolve():
        raise EvidenceError("image argument differs from metadata image")
    image_receipt = png_receipt(image)
    claimed_image = (
        int(metadata["image_width"]),
        int(metadata["image_height"]),
        int(metadata["image_bytes"]),
        metadata["image_sha256"],
    )
    actual_image = (
        image_receipt.width,
        image_receipt.height,
        image_receipt.size,
        image_receipt.sha256,
    )
    if claimed_image != actual_image:
        raise EvidenceError(f"PNG receipt mismatch: claimed={claimed_image}, actual={actual_image}")

    display_scale = parse_scale(metadata["display_scale"])
    physical_size = scaled_size(logical_size, display_scale)
    if (image_receipt.width, image_receipt.height) != physical_size:
        raise EvidenceError(
            f"PNG size does not match logical size {args.logical_size} at scale {display_scale}: "
            f"expected {physical_size[0]}x{physical_size[1]}, got "
            f"{image_receipt.width}x{image_receipt.height}"
        )
    if display_scale != EXPECTED_SCALE:
        raise EvidenceError(
            f"display scale {display_scale} is not parity-safe; scaled host image rejected"
        )
    if (image_receipt.width, image_receipt.height) != logical_size:
        raise EvidenceError("parity-safe host capture is not the exact logical PNG size")

    log = safe_child(run_dir, metadata["log"])
    markers = safe_child(run_dir, metadata["markers"])
    if file_sha256(log) != metadata["log_sha256"]:
        raise EvidenceError("runtime log hash drifted")
    if file_sha256(markers) != metadata["markers_sha256"]:
        raise EvidenceError("marker receipt hash drifted")
    # Tracing output may carry SGR color sequences when the capture session
    # runs on a TTY. Strip them before any substring provenance check so
    # field markers (backend="spectacle-active-window", CAPTURE_MARKER ...)
    # match the emitted text, not its presentation.
    ansi_sgr = re.compile(r"\x1b\[[0-9;]*[A-Za-z]")
    log_text = ansi_sgr.sub("", log.read_text(encoding="utf-8"))
    marker_text = ansi_sgr.sub("", markers.read_text(encoding="utf-8"))
    marker_base = f"scenario={args.scenario}"
    required_markers = (
        f"CAPTURE_MARKER event=theme_ready {marker_base} theme={args.theme} high_contrast=false",
        f"CAPTURE_MARKER event=telemetry_ready {marker_base}",
        f"CAPTURE_MARKER event=ui_data_ready {marker_base}",
    )
    if args.scenario != "standard":
        required_markers += (f"CAPTURE_MARKER event=scenario_ready {marker_base}",)
    for marker in required_markers:
        if marker not in log_text or marker not in marker_text:
            raise EvidenceError(f"missing strict marker: {marker}")

    native_receipt = None
    if private_kwin:
        native_image = safe_child(run_dir, metadata["native_image"])
        native_png = png_receipt(native_image)
        claimed_native = (
            int(metadata["native_width"]),
            int(metadata["native_height"]),
            int(metadata["native_bytes"]),
            metadata["native_sha256"],
        )
        actual_native = (
            native_png.width,
            native_png.height,
            native_png.size,
            native_png.sha256,
        )
        if claimed_native != actual_native:
            raise EvidenceError(
                f"native PNG receipt mismatch: claimed={claimed_native}, actual={actual_native}"
            )
        if (native_png.width, native_png.height) != (image_receipt.width, image_receipt.height):
            raise EvidenceError("native and external active-window PNG dimensions differ")
        if native_png.size <= 5000:
            raise EvidenceError("native active-window PNG is unexpectedly small")
        if "current-window PNG capture completed" not in log_text:
            raise EvidenceError("native current-window completion is missing from the app log")
        if (
            'backend="in-process"' not in log_text
            and 'backend="spectacle-active-window"' not in log_text
        ):
            raise EvidenceError("native backend provenance is missing from the app log")
        visual_receipt(native_image)
        native_ocr = ocr_text(native_image)
        reject_skeleton_text(native_ocr)
        native_receipt = {
            "image": metadata["native_image"],
            "width": native_png.width,
            "height": native_png.height,
            "bytes": native_png.size,
            "sha256": native_png.sha256,
            "ocr_sha256": hashlib.sha256(native_ocr.encode("utf-8")).hexdigest(),
        }

    visual = visual_receipt(image)
    ocr = ocr_text(image)
    args.ocr_output.write_text(ocr, encoding="utf-8")
    reject_skeleton_text(ocr)

    if args.current_worktree:
        head = current("git", "rev-parse", "--short=12", "HEAD", cwd=root)
        rust = current("rustc", "-V", cwd=root)
        state = "dirty" if current("git", "status", "--porcelain", cwd=root) else "clean"
        if (metadata["git_head"], metadata["rust"], metadata["worktree"]) != (head, rust, state):
            raise EvidenceError("diagnostic provenance differs from current worktree")

    return {
        "schema_version": 1,
        "status": "pass",
        "diagnostic_only": True,
        "parity_evidence": False,
        "run_id": metadata["run_id"],
        "run_uuid": metadata["run_uuid"],
        "git_head": metadata["git_head"],
        "worktree": metadata["worktree"],
        "rust": metadata["rust"],
        "scenario": args.scenario,
        "logical_size": args.logical_size,
        "display_scale": str(display_scale),
        "image": metadata["image"],
        "image_width": image_receipt.width,
        "image_height": image_receipt.height,
        "image_bytes": image_receipt.size,
        "image_sha256": image_receipt.sha256,
        "log_sha256": metadata["log_sha256"],
        "markers_sha256": metadata["markers_sha256"],
        "source_manifest_sha256": metadata.get("source_manifest_sha256"),
        "window_identity": metadata["window_identity"],
        "native_window_capture": native_receipt,
        "visual": {
            "visible_pixels": visual.visible_pixels,
            "sampled_pixels": visual.sampled_pixels,
            "unique_colors": visual.unique_colors,
            "luminance_span": visual.luminance_span,
            "dominant_fraction": visual.dominant_fraction,
        },
        "ocr_sha256": hashlib.sha256(ocr.encode("utf-8")).hexdigest(),
    }


def write_receipt(path: Path, payload: dict[str, object]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    temporary.replace(path)


def self_test() -> None:
    assert parse_size("1180x780") == (1180, 780)
    assert scaled_size((1180, 780), Decimal("2")) == (2360, 1560)
    try:
        parse_size("1180x0")
    except EvidenceError:
        pass
    else:
        raise EvidenceError("invalid zero-size self-test was accepted")
    try:
        reject_skeleton_text("Collecting telemetry...")
    except EvidenceError:
        pass
    else:
        raise EvidenceError("skeleton OCR self-test was accepted")

    width = height = 32
    rows = []
    for y in range(height):
        row = bytearray([0])
        for x in range(width):
            row.extend(((x * 7) % 256, (y * 11) % 256, ((x + y) * 13) % 256))
        rows.append(bytes(row))
    png = (
        PNG_SIGNATURE
        + png_chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
        + png_chunk(b"IDAT", zlib.compress(b"".join(rows)))
        + png_chunk(b"IEND", b"")
    )
    with tempfile.TemporaryDirectory() as temporary:
        path = Path(temporary) / "pattern.png"
        path.write_bytes(png)
        receipt = visual_receipt(path)
        if receipt.unique_colors < 16 or receipt.luminance_span < 24:
            raise EvidenceError("visual variation self-test was too weak")
    print("host Wayland diagnostic validator self-test: PASS")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", type=Path)
    parser.add_argument("--metadata", type=Path)
    parser.add_argument("--markers", type=Path)
    parser.add_argument("--ocr-output", type=Path)
    parser.add_argument("--receipt", type=Path)
    parser.add_argument("--run-dir", type=Path)
    parser.add_argument("--repo-root", type=Path)
    parser.add_argument("--logical-size")
    parser.add_argument("--scenario")
    parser.add_argument("--theme")
    parser.add_argument("--current-worktree", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.self_test:
        try:
            self_test()
            return 0
        except (EvidenceError, OSError, ValueError) as error:
            print(f"host diagnostic validator self-test: FAIL: {error}", file=sys.stderr)
            return 1

    required = (
        "image",
        "metadata",
        "markers",
        "ocr_output",
        "receipt",
        "run_dir",
        "repo_root",
        "logical_size",
        "scenario",
        "theme",
    )
    if missing := [name for name in required if getattr(args, name) is None]:
        print(f"host diagnostic validation: FAIL: missing {', '.join(missing)}", file=sys.stderr)
        return 1
    try:
        payload = validate(args)
        write_receipt(args.receipt, payload)
        print(
            "host Wayland diagnostic validation: PASS "
            f"({payload['image_width']}x{payload['image_height']}; diagnostic-only, not parity)"
        )
        return 0
    except (EvidenceError, OSError, ValueError, subprocess.SubprocessError) as error:
        rejection = {
            "schema_version": 1,
            "status": "reject",
            "diagnostic_only": True,
            "parity_evidence": False,
            "reason": str(error),
        }
        try:
            if args.metadata.is_file():
                rejection["run_id"] = read_metadata(args.metadata).get("run_id", "unknown")
            write_receipt(args.receipt, rejection)
        except (OSError, EvidenceError):
            pass
        print(f"host Wayland diagnostic validation: FAIL: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
