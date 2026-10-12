#!/usr/bin/env python3
"""Validate the single-frame Ratatui/Niri evidence bundle."""

from __future__ import annotations

import argparse
import csv
import json
import re
import struct
import subprocess
import sys
import tempfile
import zlib
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path

from validate_capture_evidence import EvidenceError, file_sha256, png_chunk, png_receipt

try:
    from capture_supervisor import SupervisorError, validate_supervised_metadata
    from capture_tui_terminal_profiles import MatrixError, read_profiles
except ModuleNotFoundError:
    from scripts.capture_supervisor import (  # type: ignore[no-redef]
        SupervisorError,
        validate_supervised_metadata,
    )
    from scripts.capture_tui_terminal_profiles import (  # type: ignore[no-redef]
        MatrixError,
        read_profiles,
    )


@dataclass(frozen=True)
class VisualContentReceipt:
    visible_pixels: int
    unique_colors: int
    luminance_span: int


TERMINAL_PROFILE_PATTERN = re.compile(
    r"TUI_CAPTURE_MARKER event=terminal_profile "
    r"language=(en|zh) glyphs=(ascii|unicode) "
    r"color=(truecolor|ansi256|ansi16|monochrome)"
)
TERMINAL_PROFILE_INPUTS = {
    "terminal_term": "term",
    "terminal_colorterm": "colorterm",
    "terminal_no_color": "no_color",
    "terminal_lc_all": "lc_all",
    "terminal_lc_ctype": "lc_ctype",
    "terminal_lc_messages": "lc_messages",
    "terminal_lang": "lang",
    "terminal_glyph_override": "glyph_override",
}


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


def visual_content_receipt(
    path: Path, sample_bounds: tuple[int, int, int, int] | None = None
) -> VisualContentReceipt:
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
    sample_left, sample_top, sample_right, sample_bottom = sample_bounds or (0, 0, width, height)
    if not (0 <= sample_left < sample_right <= width and 0 <= sample_top < sample_bottom <= height):
        raise EvidenceError("invalid visual-content sample bounds")
    sample_pixels = (sample_right - sample_left) * (sample_bottom - sample_top)
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
    for row_index in range(height):
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

        if not sample_top <= row_index < sample_bottom:
            continue
        for index in range(sample_left * channels, sample_right * channels, channels):
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
    if visible < sample_pixels // 4:
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
        binary = evidence_root / "bin" / "taskforest-t"
        binary.parent.mkdir()
        binary.write_bytes(b"receipt binary")
        repo_root = Path(temporary)
        binary_values = {
            "binary": str(binary.relative_to(repo_root)),
            "binary_sha256": file_sha256(binary),
            "binary_provision": "prebuilt",
            "binary_source": "target/debug/taskforest-t",
            "terminal_scenario": "<manual>",
        }
        assert validate_binary_provenance(
            binary_values,
            repo_root,
            repo_root / "target" / "tui-evidence",
        ) == binary
        binary_values["binary_sha256"] = "0" * 64
        try:
            validate_binary_provenance(
                binary_values,
                repo_root,
                repo_root / "target" / "tui-evidence",
            )
        except EvidenceError:
            pass
        else:
            raise EvidenceError("binary provenance accepted a mismatched hash")
        outside = Path(temporary) / "outside.png"
        try:
            resolve_evidence_path(outside, evidence_root, "image")
        except EvidenceError:
            pass
        else:
            raise EvidenceError("evidence path outside the run root was accepted")

    for row in read_profiles().values():
        values = {"terminal_scenario": row["name"]}
        values.update(
            {
                metadata_key: row[matrix_key]
                for metadata_key, matrix_key in TERMINAL_PROFILE_INPUTS.items()
            }
        )
        language = row["expected_language"]
        glyphs = row["expected_glyphs"]
        color = row["expected_color"]
        values.update(
            {
                "terminal_profile_language": language,
                "terminal_profile_glyphs": glyphs,
                "terminal_profile_color": color,
            }
        )
        marker = (
            "TUI_CAPTURE_MARKER event=terminal_profile "
            f"language={language} glyphs={glyphs} color={color}"
        )
        validate_terminal_profile(values, marker)

    try:
        terminal_profile_from_markers(
            "TUI_CAPTURE_MARKER event=terminal_profile language=en "
            "glyphs=unicode color=ansi256\n"
            "TUI_CAPTURE_MARKER event=terminal_profile language=en "
            "glyphs=unicode color=ansi256"
        )
    except EvidenceError:
        pass
    else:
        raise EvidenceError("duplicate resolved terminal profile markers were accepted")


def metadata(path: Path) -> dict[str, str]:
    values: dict[str, str] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        key, separator, value = line.partition("=")
        if not separator or not key or key in values:
            raise EvidenceError(f"invalid metadata line: {line!r}")
        values[key] = value
    required = {
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
        "niri",
        "terminal",
        "stack",
        "binary",
        "binary_sha256",
        "binary_provision",
        "binary_source",
        "source_scope",
        "source_manifest_sha256",
        "dbus_isolation",
        "command",
        "terminal_scenario",
        *TERMINAL_PROFILE_INPUTS,
        "terminal_profile_language",
        "terminal_profile_glyphs",
        "terminal_profile_color",
    }
    if missing := required - values.keys():
        raise EvidenceError(f"metadata missing fields: {sorted(missing)}")
    if values["dbus_isolation"] != "private-session":
        raise EvidenceError("TUI capture must use the private D-Bus session")
    return values


def terminal_profile_from_markers(markers: str) -> tuple[str, str, str]:
    lines = [
        line
        for line in markers.splitlines()
        if line.startswith("TUI_CAPTURE_MARKER event=terminal_profile ")
    ]
    if len(lines) != 1:
        raise EvidenceError(
            f"expected exactly one resolved terminal profile marker, found {len(lines)}"
        )
    match = TERMINAL_PROFILE_PATTERN.fullmatch(lines[0])
    if match is None:
        raise EvidenceError(f"invalid resolved terminal profile marker: {lines[0]!r}")
    return match.groups()


def validate_terminal_profile(values: dict[str, str], markers: str) -> dict[str, str]:
    language, glyphs, color = terminal_profile_from_markers(markers)
    marker_profile = {
        "terminal_profile_language": language,
        "terminal_profile_glyphs": glyphs,
        "terminal_profile_color": color,
    }
    if any(values[key] != value for key, value in marker_profile.items()):
        raise EvidenceError("resolved terminal profile metadata differs from the painted frame marker")

    scenario = values["terminal_scenario"]
    if scenario != "<manual>":
        try:
            row = read_profiles().get(scenario)
        except (MatrixError, OSError) as error:
            raise EvidenceError(f"terminal profile matrix is invalid: {error}") from error
        if row is None:
            raise EvidenceError(f"unknown terminal profile scenario: {scenario!r}")
        for metadata_key, matrix_key in TERMINAL_PROFILE_INPUTS.items():
            if values[metadata_key] != row[matrix_key]:
                raise EvidenceError(
                    f"terminal input {metadata_key} differs from matrix scenario {scenario!r}"
                )
        expected = (
            row["expected_language"],
            row["expected_glyphs"],
            row["expected_color"],
        )
        if (language, glyphs, color) != expected:
            raise EvidenceError(
                f"resolved terminal profile {(language, glyphs, color)!r} "
                f"differs from {scenario!r} expectation {expected!r}"
            )
    return {
        "scenario": scenario,
        "language": language,
        "glyphs": glyphs,
        "color": color,
    }


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


def validate_binary_provenance(
    values: dict[str, str], root: Path, evidence_root: Path
) -> Path:
    binary = resolve_evidence_path(root / values["binary"], evidence_root, "binary")
    if not binary.is_file() or file_sha256(binary) != values["binary_sha256"]:
        raise EvidenceError("run-owned TUI binary is missing or differs from its receipt hash")
    if values["binary_provision"] not in {"capture-build", "prebuilt"}:
        raise EvidenceError("unknown TUI binary provisioning path")
    if values["binary_source"] != "target/debug/taskforest-t":
        raise EvidenceError("TUI evidence must use the taskforest-t product binary")
    if (
        values["terminal_scenario"] != "<manual>"
        and values["binary_provision"] != "prebuilt"
    ):
        raise EvidenceError("terminal matrix rows must reuse the prebuilt taskforest-t binary")
    return binary


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
        values = metadata(metadata_path)
        binary_path = validate_binary_provenance(values, root, evidence_root)
        cols = int(values.get("terminal_columns", "120"))
        lines_cnt = int(values.get("terminal_lines", "36"))
        min_width = 500 if cols <= 80 else 900
        min_height = 400 if lines_cnt <= 24 else 500
        if receipt.width < min_width or receipt.height < min_height:
            raise EvidenceError(f"TUI image is too small: {receipt.width}x{receipt.height}")
        try:
            validate_supervised_metadata(values, root, metadata_path, "tui")
        except SupervisorError as error:
            raise EvidenceError(str(error)) from error
        expected_page = values.get("page", "performance")
        markers = markers_path.read_text(encoding="utf-8")
        terminal_profile = validate_terminal_profile(values, markers)
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
            "run_uuid": values["run_uuid"],
            "git_head": values["git_head"],
            "worktree": values["worktree"],
            "rust": values["rust"],
            "stack": values["stack"],
            "binary": binary_path.name,
            "binary_sha256": values["binary_sha256"],
            "binary_provision": values["binary_provision"],
            "source_scope": values["source_scope"],
            "source_manifest_sha256": values["source_manifest_sha256"],
            "terminal_environment": {
                key.removeprefix("terminal_"): values[key]
                for key in TERMINAL_PROFILE_INPUTS
            },
            "terminal_profile": terminal_profile,
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
    except (
        EvidenceError,
        MatrixError,
        OSError,
        ValueError,
        subprocess.CalledProcessError,
    ) as error:
        print(f"TUI evidence validation: FAIL: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
