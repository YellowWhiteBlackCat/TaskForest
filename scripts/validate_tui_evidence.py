#!/usr/bin/env python3
"""Validate the single-frame Ratatui/Niri evidence bundle."""

from __future__ import annotations

import argparse
import csv
import json
import struct
import subprocess
import sys
import tempfile
import zlib
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path

from validate_capture_evidence import EvidenceError, file_sha256, png_chunk, png_receipt


@dataclass(frozen=True)
class VisualContentReceipt:
    visible_pixels: int
    unique_colors: int
    luminance_span: int


def _paeth(left: int, above: int, upper_left: int) -> int:
    estimate = left + above - upper_left
    left_distance = abs(estimate - left)
    above_distance = abs(estimate - above)
    upper_left_distance = abs(estimate - upper_left)
    if left_distance <= above_distance and left_distance <= upper_left_distance:
        return left
    if above_distance <= upper_left_distance:
        return above
    return upper_left


def visual_content_receipt(path: Path) -> VisualContentReceipt:
    """Decode Niri's 8-bit RGB(A) PNG and reject transparent or uniform frames."""
    data = path.read_bytes()
    offset = 8
    width = height = bit_depth = color_type = interlace = 0
    compressed = bytearray()
    while offset < len(data):
        length = struct.unpack(">I", data[offset : offset + 4])[0]
        kind = data[offset + 4 : offset + 8]
        payload = data[offset + 8 : offset + 8 + length]
        offset += 12 + length
        if kind == b"IHDR":
            width, height, bit_depth, color_type, _, _, interlace = struct.unpack(
                ">IIBBBBB", payload
            )
        elif kind == b"IDAT":
            compressed.extend(payload)
        elif kind == b"IEND":
            break

    if bit_depth != 8 or color_type not in (2, 6) or interlace != 0:
        raise EvidenceError("TUI visual-content check requires non-interlaced 8-bit RGB(A)")
    channels = 4 if color_type == 6 else 3
    row_bytes = width * channels
    raw = zlib.decompress(compressed)
    if len(raw) != height * (row_bytes + 1):
        raise EvidenceError("TUI PNG scanline payload length differs from IHDR")

    previous = bytearray(row_bytes)
    position = 0
    visible = 0
    colors: set[tuple[int, int, int]] = set()
    minimum_luminance = 255
    maximum_luminance = 0
    for _ in range(height):
        filter_kind = raw[position]
        position += 1
        encoded = raw[position : position + row_bytes]
        position += row_bytes
        decoded = bytearray(row_bytes)
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
                predictor = _paeth(left, above, upper_left)
            else:
                raise EvidenceError(f"unsupported PNG row filter: {filter_kind}")
            decoded[index] = (value + predictor) & 0xFF
        previous = decoded

        for index in range(0, row_bytes, channels):
            red, green, blue = decoded[index : index + 3]
            alpha = decoded[index + 3] if channels == 4 else 255
            if alpha <= 16:
                continue
            visible += 1
            if len(colors) < 256:
                colors.add((red, green, blue))
            luminance = (54 * red + 183 * green + 19 * blue) // 256
            minimum_luminance = min(minimum_luminance, luminance)
            maximum_luminance = max(maximum_luminance, luminance)

    receipt = VisualContentReceipt(
        visible_pixels=visible,
        unique_colors=len(colors),
        luminance_span=maximum_luminance - minimum_luminance,
    )
    if visible < width * height // 4:
        raise EvidenceError(f"TUI frame is mostly transparent: {visible} visible pixels")
    if receipt.unique_colors < 16 or receipt.luminance_span < 24:
        raise EvidenceError(
            "TUI frame lacks visible content variation: "
            f"colors={receipt.unique_colors}, luminance_span={receipt.luminance_span}"
        )
    return receipt


def self_test() -> None:
    width = height = 32
    solid_rows = b"".join(b"\x00" + b"\x00\x00\x00\xff" * width for _ in range(height))
    png = (
        b"\x89PNG\r\n\x1a\n"
        + png_chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
        + png_chunk(b"IDAT", zlib.compress(solid_rows))
        + png_chunk(b"IEND", b"")
    )
    with tempfile.TemporaryDirectory() as temporary:
        path = Path(temporary) / "blank.png"
        path.write_bytes(png)
        try:
            visual_content_receipt(path)
        except EvidenceError:
            pass
        else:
            raise EvidenceError("uniform black TUI frame was accepted")

        evidence_root = Path(temporary) / "target" / "tui-evidence" / "run"
        evidence_root.mkdir(parents=True)
        inside = evidence_root / "image.png"
        assert resolve_evidence_path(inside, evidence_root, "image") == inside
        outside = Path(temporary) / "outside.png"
        try:
            resolve_evidence_path(outside, evidence_root, "image")
        except EvidenceError:
            pass
        else:
            raise EvidenceError("evidence path outside the run root was accepted")


def metadata(path: Path) -> dict[str, str]:
    values: dict[str, str] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        key, separator, value = line.partition("=")
        if not separator or not key or key in values:
            raise EvidenceError(f"invalid metadata line: {line!r}")
        values[key] = value
    required = {
        "run_id",
        "captured_at",
        "git_head",
        "worktree",
        "rust",
        "niri",
        "terminal",
        "stack",
        "source_scope",
        "source_manifest_sha256",
        "command",
    }
    if missing := required - values.keys():
        raise EvidenceError(f"metadata missing fields: {sorted(missing)}")
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
            source.relative_to(root)
        except ValueError as error:
            raise EvidenceError(f"source path escapes repository: {relative}") from error
        if not source.is_file() or file_sha256(source) != digest:
            raise EvidenceError(f"source provenance mismatch: {relative}")


def resolve_evidence_path(path: Path, evidence_root: Path, label: str) -> Path:
    """Resolve an evidence path and keep it below target/tui-evidence."""
    resolved = path.resolve()
    try:
        resolved.relative_to(evidence_root)
    except ValueError as error:
        raise EvidenceError(
            f"{label} path escapes target/tui-evidence: {path}"
        ) from error
    return resolved


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", type=Path)
    parser.add_argument("--metadata", type=Path)
    parser.add_argument("--markers", type=Path)
    parser.add_argument("--source-manifest", type=Path)
    parser.add_argument("--manifest", type=Path)
    parser.add_argument("--receipt", type=Path)
    parser.add_argument("--repo-root", type=Path)
    parser.add_argument("--current-worktree", action="store_true")
    parser.add_argument("--check-only", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    try:
        if args.self_test:
            self_test()
            print("TUI evidence visual-content self-test: PASS")
            return 0
        required = ("image", "metadata", "markers", "source_manifest", "manifest", "receipt", "repo_root")
        if missing := [name for name in required if getattr(args, name) is None]:
            raise EvidenceError(f"missing required arguments: {', '.join(missing)}")
        root = args.repo_root.resolve()
        evidence_root = (root / "target" / "tui-evidence").resolve()
        image = resolve_evidence_path(args.image, evidence_root, "image")
        metadata_path = resolve_evidence_path(args.metadata, evidence_root, "metadata")
        markers_path = resolve_evidence_path(args.markers, evidence_root, "markers")
        source_manifest = resolve_evidence_path(
            args.source_manifest, evidence_root, "source manifest"
        )
        manifest = resolve_evidence_path(args.manifest, evidence_root, "manifest")
        receipt_path = resolve_evidence_path(args.receipt, evidence_root, "receipt")

        receipt = png_receipt(image)
        visual = visual_content_receipt(image)
        if receipt.width < 900 or receipt.height < 500:
            raise EvidenceError(f"TUI image is too small: {receipt.width}x{receipt.height}")
        values = metadata(metadata_path)
        expected_page = values.get("page", "performance")
        markers = markers_path.read_text(encoding="utf-8")
        for token in (
            "TUI_CAPTURE_MARKER event=demo_data_ready mode=demo",
            f"TUI_CAPTURE_MARKER event=frame_ready page={expected_page}",
        ):
            if token not in markers:
                raise EvidenceError(f"missing marker: {token}")
        if file_sha256(source_manifest) != values["source_manifest_sha256"]:
            raise EvidenceError("source manifest hash differs from metadata")
        if values["source_scope"] != "tui":
            raise EvidenceError(f"unexpected source scope: {values['source_scope']!r}")
        validate_source_manifest(source_manifest, root)
        if args.current_worktree:
            head = current("git", "rev-parse", "--short=12", "HEAD", cwd=root)
            rust = current("rustc", "-V", cwd=root)
            state = "dirty" if current("git", "status", "--porcelain", cwd=root) else "clean"
            if (head, rust, state) != (values["git_head"], values["rust"], values["worktree"]):
                raise EvidenceError("capture provenance differs from current worktree")

        if not args.check_only:
            with manifest.open("a", encoding="utf-8", newline="") as handle:
                writer = csv.writer(handle, delimiter="\t", lineterminator="\n")
                writer.writerow([
                    image.name,
                    receipt.width,
                    receipt.height,
                    receipt.size,
                    receipt.sha256,
                    "ready",
                ])
        payload = {
            "schema_version": 1,
            "status": "pass",
            "validated_at": datetime.now(timezone.utc).isoformat(timespec="seconds"),
            "run_id": values["run_id"],
            "git_head": values["git_head"],
            "worktree": values["worktree"],
            "rust": values["rust"],
            "stack": values["stack"],
            "source_scope": values["source_scope"],
            "source_manifest_sha256": values["source_manifest_sha256"],
            "metadata_sha256": file_sha256(metadata_path),
            "markers_sha256": file_sha256(markers_path),
            "artifact": {
                "image": image.name,
                "width": receipt.width,
                "height": receipt.height,
                "bytes": receipt.size,
                "sha256": receipt.sha256,
                "visible_pixels": visual.visible_pixels,
                "unique_colors": visual.unique_colors,
                "luminance_span": visual.luminance_span,
            },
        }
        if not args.check_only:
            receipt_path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
        print(f"TUI evidence validation: PASS ({receipt.width}x{receipt.height})")
        return 0
    except (EvidenceError, OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"TUI evidence validation: FAIL: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
