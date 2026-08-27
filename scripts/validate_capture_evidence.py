#!/usr/bin/env python3
"""Fail-closed validator for the nested-Niri screenshot evidence bundle."""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import struct
import subprocess
import sys
import tempfile
import zlib
from dataclasses import dataclass
from datetime import datetime, timezone
from decimal import Decimal, InvalidOperation
from pathlib import Path

MANIFEST_FIELDS = (
    "captured_at",
    "git_head",
    "worktree",
    "rust",
    "scenario",
    "image",
    "skin",
    "page",
    "device",
    "settings",
    "width",
    "height",
    "bytes",
    "sha256",
    "markers",
    "log",
    "log_sha256",
    "marker_receipt",
)
MATRIX_FIELDS = (
    "name",
    "skin",
    "page",
    "device",
    "settings",
    "scenario",
    "window_size",
    "capture_size",
)
WINDOW_RECEIPT_FIELDS = (
    "scenario",
    "app_pid",
    "window_id",
    "windows_json",
    "windows_sha256",
    "action_log",
    "action_sha256",
)
PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"
MAX_PNG_BYTES = 64 * 1024 * 1024
EXPECTED_NIRI_OUTPUT = "winit"
EXPECTED_NIRI_SCALE = Decimal("1")
EXPECTED_APP_ID = "io.github.YellowWhiteBlackCat.TaskForestG"


class EvidenceError(RuntimeError):
    pass


@dataclass(frozen=True)
class PngReceipt:
    width: int
    height: int
    size: int
    sha256: str


def parse_capture_size(value: str, context: str) -> tuple[int, int]:
    try:
        width, height = (int(part) for part in value.lower().split("x", 1))
    except (TypeError, ValueError) as error:
        raise EvidenceError(f"{context}: invalid capture_size {value!r}") from error
    if width < 1 or height < 1:
        raise EvidenceError(f"{context}: capture_size must be positive")
    return width, height


def file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def png_receipt(path: Path) -> PngReceipt:
    data = path.read_bytes()
    if len(data) > MAX_PNG_BYTES:
        raise EvidenceError(f"PNG exceeds {MAX_PNG_BYTES} bytes: {path}")
    if not data.startswith(PNG_SIGNATURE):
        raise EvidenceError(f"invalid PNG signature: {path}")

    offset = len(PNG_SIGNATURE)
    width = height = 0
    saw_ihdr = saw_iend = False
    chunk_index = 0
    while offset < len(data):
        if offset + 12 > len(data):
            raise EvidenceError(f"truncated PNG chunk header: {path}")
        length = struct.unpack(">I", data[offset : offset + 4])[0]
        chunk_type = data[offset + 4 : offset + 8]
        data_start = offset + 8
        data_end = data_start + length
        crc_end = data_end + 4
        if crc_end > len(data):
            raise EvidenceError(f"truncated PNG chunk body: {path}")
        chunk_data = data[data_start:data_end]
        stored_crc = struct.unpack(">I", data[data_end:crc_end])[0]
        computed_crc = zlib.crc32(chunk_type)
        computed_crc = zlib.crc32(chunk_data, computed_crc) & 0xFFFFFFFF
        if stored_crc != computed_crc:
            raise EvidenceError(f"PNG CRC mismatch in {chunk_type!r}: {path}")

        if chunk_index == 0 and chunk_type != b"IHDR":
            raise EvidenceError(f"PNG does not start with IHDR: {path}")
        if chunk_type == b"IHDR":
            if saw_ihdr or length != 13:
                raise EvidenceError(f"invalid or duplicate PNG IHDR: {path}")
            width, height = struct.unpack(">II", chunk_data[:8])
            if width == 0 or height == 0:
                raise EvidenceError(f"PNG has empty dimensions: {path}")
            saw_ihdr = True
        elif chunk_type == b"IEND":
            if length != 0:
                raise EvidenceError(f"invalid PNG IEND: {path}")
            saw_iend = True
            offset = crc_end
            break

        offset = crc_end
        chunk_index += 1

    if not saw_ihdr or not saw_iend or offset != len(data):
        raise EvidenceError(f"incomplete or trailing PNG data: {path}")
    return PngReceipt(width, height, len(data), hashlib.sha256(data).hexdigest())


def read_tsv(path: Path, expected_fields: tuple[str, ...]) -> list[dict[str, str]]:
    with path.open("r", encoding="utf-8", newline="") as handle:
        reader = csv.DictReader(handle, delimiter="\t")
        if tuple(reader.fieldnames or ()) != expected_fields:
            raise EvidenceError(
                f"{path} fields differ: expected {expected_fields}, got {reader.fieldnames}"
            )
        rows = list(reader)
    if not rows:
        raise EvidenceError(f"empty TSV: {path}")
    if any(None in row or any(value is None for value in row.values()) for row in rows):
        raise EvidenceError(f"malformed TSV row: {path}")
    return rows


def safe_child(root: Path, relative: str) -> Path:
    candidate = (root / relative).resolve()
    try:
        candidate.relative_to(root.resolve())
    except ValueError as error:
        raise EvidenceError(f"artifact escapes evidence root: {relative}") from error
    return candidate


def read_metadata(path: Path) -> dict[str, str]:
    values: dict[str, str] = {}
    for raw_line in path.read_text(encoding="utf-8").splitlines():
        key, separator, value = raw_line.partition("=")
        if not separator or not key or key in values:
            raise EvidenceError(f"invalid metadata line: {raw_line!r}")
        values[key] = value
    required = {"run_id", "captured_at", "git_head", "worktree", "rust", "niri", "command"}
    if missing := required - values.keys():
        raise EvidenceError(f"metadata missing fields: {sorted(missing)}")
    return values


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


def niri_output_receipt(path: Path, minimum_size: tuple[int, int]) -> dict[str, object]:
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise EvidenceError(f"invalid Niri output receipt: {path}") from error
    if isinstance(payload, dict):
        if set(payload) != {EXPECTED_NIRI_OUTPUT} or not isinstance(payload[EXPECTED_NIRI_OUTPUT], dict):
            raise EvidenceError("nested Niri output receipt must contain exactly one winit output")
        output = payload[EXPECTED_NIRI_OUTPUT]
    elif isinstance(payload, list) and len(payload) == 1 and isinstance(payload[0], dict):
        output = payload[0]
    else:
        raise EvidenceError("nested Niri output receipt must contain exactly one output")
    if output.get("name") != EXPECTED_NIRI_OUTPUT:
        raise EvidenceError(f"nested Niri output is not {EXPECTED_NIRI_OUTPUT!r}")
    logical = output.get("logical")
    if not isinstance(logical, dict):
        raise EvidenceError("nested Niri output has no logical geometry")
    try:
        scale = Decimal(str(logical["scale"]))
        width = int(logical["width"])
        height = int(logical["height"])
    except (KeyError, TypeError, ValueError, InvalidOperation) as error:
        raise EvidenceError("nested Niri output has invalid scale or geometry") from error
    if scale != EXPECTED_NIRI_SCALE:
        raise EvidenceError(f"nested Niri output scale {scale} is not parity-safe scale=1")
    if (width, height) < minimum_size:
        raise EvidenceError(
            f"nested Niri logical output {width}x{height} is smaller than required "
            f"{minimum_size[0]}x{minimum_size[1]}"
        )
    return {"name": output["name"], "scale": str(scale), "width": width, "height": height}


def validate_window_payload(payload: object, app_pid: int, window_id: int, scenario: str) -> None:
    if not isinstance(payload, list):
        raise EvidenceError(f"{scenario}: Niri window receipt is not an array")
    for window in payload:
        if not isinstance(window, dict):
            continue
        try:
            candidate_id = int(window["id"])
            candidate_pid = int(window["pid"])
        except (KeyError, TypeError, ValueError):
            continue
        if (
            candidate_id == window_id
            and candidate_pid == app_pid
            and window.get("app_id") == EXPECTED_APP_ID
        ):
            return
    raise EvidenceError(
        f"{scenario}: window receipt does not bind app_id={EXPECTED_APP_ID}, "
        f"pid={app_pid}, window_id={window_id}"
    )


def validate_window_receipts(
    path: Path, repo_root: Path, matrix: dict[str, dict[str, str]]
) -> dict[str, dict[str, object]]:
    rows = read_tsv(path, WINDOW_RECEIPT_FIELDS)
    receipts: dict[str, dict[str, object]] = {}
    for row in rows:
        scenario = row["scenario"]
        if scenario not in matrix or scenario in receipts:
            raise EvidenceError(f"invalid or duplicate window receipt scenario: {scenario!r}")
        if not row["app_pid"].isdigit() or not row["window_id"].isdigit():
            raise EvidenceError(f"{scenario}: window receipt ids must be positive integers")
        app_pid = int(row["app_pid"])
        window_id = int(row["window_id"])
        if app_pid < 1 or window_id < 1:
            raise EvidenceError(f"{scenario}: window receipt ids must be positive")

        windows_path = safe_child(repo_root, row["windows_json"])
        if not windows_path.is_file() or file_sha256(windows_path) != row["windows_sha256"]:
            raise EvidenceError(f"{scenario}: Niri window JSON is absent or its hash drifted")
        try:
            windows = json.loads(windows_path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as error:
            raise EvidenceError(f"{scenario}: invalid Niri window JSON") from error
        validate_window_payload(windows, app_pid, window_id, scenario)

        action_path = safe_child(repo_root, row["action_log"])
        if not action_path.is_file() or file_sha256(action_path) != row["action_sha256"]:
            raise EvidenceError(f"{scenario}: screenshot action log is absent or its hash drifted")
        action = action_path.read_text(encoding="utf-8")
        if (
            f"app_pid={app_pid}" not in action
            or f"window_id={window_id}" not in action
            or f"screenshot-window --id {window_id} " not in action
        ):
            raise EvidenceError(f"{scenario}: screenshot action was not bound to its window receipt")
        receipts[scenario] = {
            "app_pid": app_pid,
            "window_id": window_id,
            "windows_sha256": row["windows_sha256"],
            "action_sha256": row["action_sha256"],
        }
    if receipts.keys() != matrix.keys():
        raise EvidenceError(
            "matrix/window-receipt scenarios differ: "
            f"missing={sorted(matrix.keys() - receipts.keys())}, "
            f"extra={sorted(receipts.keys() - matrix.keys())}"
        )
    return receipts


def command_output(*args: str, cwd: Path) -> str:
    result = subprocess.run(
        args, cwd=cwd, check=True, text=True, capture_output=True, timeout=15
    )
    return result.stdout.strip()


def validate_bundle(args: argparse.Namespace) -> dict[str, object]:
    matrix_rows = read_tsv(args.matrix, MATRIX_FIELDS)
    manifest_rows = read_tsv(args.manifest, MANIFEST_FIELDS)
    metadata = read_metadata(args.metadata)
    repo_root = args.repo_root.resolve()
    screenshots = args.screenshots.resolve()

    if args.source_manifest is not None:
        if metadata.get("source_scope") != "gpui":
            raise EvidenceError(f"unexpected source scope: {metadata.get('source_scope')!r}")
        claimed_source_hash = metadata.get("source_manifest_sha256")
        if not claimed_source_hash:
            raise EvidenceError("metadata is missing source_manifest_sha256")
        validate_source_manifest(args.source_manifest, repo_root)
        if file_sha256(args.source_manifest) != claimed_source_hash:
            raise EvidenceError("source manifest hash differs from metadata")

    if (args.niri_outputs is None) != (args.window_receipts is None):
        raise EvidenceError("Niri output and window receipts must be supplied together")
    if args.require_binary:
        for field in ("binary", "binary_sha256"):
            if field not in metadata or not metadata[field]:
                raise EvidenceError(f"metadata missing required binary field: {field}")
        binary = safe_child(repo_root, metadata["binary"])
        claimed_hash = metadata["binary_sha256"]
        if len(claimed_hash) != 64 or any(char not in "0123456789abcdef" for char in claimed_hash):
            raise EvidenceError("metadata binary_sha256 is not a lowercase SHA-256 digest")
        if not binary.is_file() or file_sha256(binary) != claimed_hash:
            raise EvidenceError("current binary is absent or its SHA-256 differs from metadata")

    matrix: dict[str, dict[str, str]] = {}
    for row in matrix_rows:
        name = row["name"]
        if not name or name in matrix or any(char not in "abcdefghijklmnopqrstuvwxyz0123456789-" for char in name):
            raise EvidenceError(f"invalid or duplicate matrix name: {name!r}")
        matrix[name] = row

    manifest: dict[str, dict[str, str]] = {}
    for row in manifest_rows:
        image = row["image"]
        if not image.endswith(".png") or "/" in image or "\\" in image:
            raise EvidenceError(f"unsafe manifest image: {image!r}")
        name = image.removesuffix(".png")
        if name in manifest:
            raise EvidenceError(f"duplicate manifest image: {image}")
        manifest[name] = row

    if matrix.keys() != manifest.keys():
        raise EvidenceError(
            f"matrix/manifest scenarios differ: missing={sorted(matrix.keys() - manifest.keys())}, "
            f"extra={sorted(manifest.keys() - matrix.keys())}"
        )

    niri_receipt = None
    window_receipts = None
    if args.niri_outputs is not None and args.window_receipts is not None:
        minimum_size = tuple(
            max(parse_capture_size(row["capture_size"], row["name"])[index] for row in matrix_rows)
            for index in range(2)
        )
        niri_receipt = niri_output_receipt(args.niri_outputs, minimum_size)
        window_receipts = validate_window_receipts(args.window_receipts, repo_root, matrix)

    actual_pngs = {path.name for path in screenshots.glob("*.png")}
    expected_pngs = {f"{name}.png" for name in matrix}
    if actual_pngs != expected_pngs:
        raise EvidenceError(
            f"screenshot set differs: missing={sorted(expected_pngs - actual_pngs)}, "
            f"extra={sorted(actual_pngs - expected_pngs)}"
        )

    marker_lines: dict[str, list[str]] = {}
    for line in args.markers.read_text(encoding="utf-8").splitlines():
        name, separator, payload = line.partition("\t")
        if not separator or name not in matrix:
            raise EvidenceError(f"invalid marker receipt line: {line!r}")
        marker_lines.setdefault(name, []).append(payload)

    receipts: list[dict[str, object]] = []
    for name, expected in matrix.items():
        row = manifest[name]
        expected_scenario = expected["scenario"]
        for field in ("skin", "page", "device", "settings"):
            if row[field] != expected[field]:
                raise EvidenceError(f"{name}: {field} differs from canonical matrix")
        if row["scenario"] != expected_scenario:
            raise EvidenceError(f"{name}: scenario differs from canonical matrix")
        if row["markers"] != "ready":
            raise EvidenceError(f"{name}: runtime markers were not ready")
        if row["git_head"] != metadata["git_head"] or row["rust"] != metadata["rust"]:
            raise EvidenceError(f"{name}: provenance differs from metadata")
        if row["worktree"] != metadata["worktree"] or row["captured_at"] != metadata["captured_at"]:
            raise EvidenceError(f"{name}: run identity differs from metadata")
        marker_paths = {
            f"target/screenshot-evidence/{metadata['run_id']}/capture-markers.log"
        }
        if row["marker_receipt"] not in marker_paths:
            raise EvidenceError(f"{name}: marker receipt path is not canonical for this run")

        image_path = safe_child(screenshots, row["image"])
        receipt = png_receipt(image_path)
        if receipt.width < 720 or receipt.height < 480:
            raise EvidenceError(f"{name}: image is below the 720x480 contract")
        try:
            requested_width, requested_height = (
                int(part) for part in expected["capture_size"].lower().split("x", 1)
            )
        except (TypeError, ValueError) as error:
            raise EvidenceError(f"{name}: invalid canonical capture_size") from error
        if (receipt.width, receipt.height) != (requested_width, requested_height):
            raise EvidenceError(
                f"{name}: requested {requested_width}x{requested_height}, "
                f"captured {receipt.width}x{receipt.height}"
            )
        claimed = (int(row["width"]), int(row["height"]), int(row["bytes"]), row["sha256"])
        actual = (receipt.width, receipt.height, receipt.size, receipt.sha256)
        if claimed != actual:
            raise EvidenceError(f"{name}: PNG receipt mismatch: claimed={claimed}, actual={actual}")

        scenario = expected_scenario
        joined_markers = "\n".join(marker_lines.get(name, []))
        theme_token = (
            f"CAPTURE_MARKER event=theme_ready scenario={scenario} "
            f"theme={expected['skin']} high_contrast=false"
        )
        if theme_token not in joined_markers:
            raise EvidenceError(f"{name}: missing marker {theme_token}")
        for event in ("telemetry_ready", "ui_data_ready"):
            token = f"CAPTURE_MARKER event={event} scenario={scenario}"
            if token not in joined_markers:
                raise EvidenceError(f"{name}: missing marker {token}")
        if scenario != "standard":
            token = f"CAPTURE_MARKER event=scenario_ready scenario={scenario}"
            if token not in joined_markers:
                raise EvidenceError(f"{name}: missing marker {token}")

        log_path = safe_child(repo_root, row["log"])
        if args.require_logs and (not log_path.is_file() or file_sha256(log_path) != row["log_sha256"]):
            raise EvidenceError(f"{name}: full runtime log is absent or its hash drifted")
        receipts.append(
            {
                "image": row["image"],
                "scenario": scenario,
                "width": receipt.width,
                "height": receipt.height,
                "bytes": receipt.size,
                "sha256": receipt.sha256,
                "log_sha256": row["log_sha256"],
            }
        )

    if args.current_worktree:
        current_head = command_output("git", "rev-parse", "--short=12", "HEAD", cwd=repo_root)
        current_rust = command_output("rustc", "-V", cwd=repo_root)
        current_state = "dirty" if command_output("git", "status", "--porcelain", cwd=repo_root) else "clean"
        if (metadata["git_head"], metadata["rust"], metadata["worktree"]) != (
            current_head,
            current_rust,
            current_state,
        ):
            raise EvidenceError("capture provenance no longer matches the current worktree")

    return {
        "schema_version": 1,
        "status": "pass",
        "validated_at": datetime.now(timezone.utc).isoformat(timespec="seconds"),
        "run_id": metadata["run_id"],
        "git_head": metadata["git_head"],
        "worktree": metadata["worktree"],
        "rust": metadata["rust"],
        **(
            {
                "source_scope": metadata["source_scope"],
                "source_manifest_sha256": metadata["source_manifest_sha256"],
            }
            if args.source_manifest is not None
            else {}
        ),
        "matrix_sha256": file_sha256(args.matrix),
        "manifest_sha256": file_sha256(args.manifest),
        "metadata_sha256": file_sha256(args.metadata),
        "markers_sha256": file_sha256(args.markers),
        "niri_output": niri_receipt,
        "window_receipts": window_receipts,
        "artifacts": receipts,
    }


def png_chunk(kind: bytes, payload: bytes) -> bytes:
    crc = zlib.crc32(kind)
    crc = zlib.crc32(payload, crc) & 0xFFFFFFFF
    return struct.pack(">I", len(payload)) + kind + payload + struct.pack(">I", crc)


def self_test() -> None:
    raw_scanline = b"\x00\x10\x20\x30"
    png = (
        PNG_SIGNATURE
        + png_chunk(b"IHDR", struct.pack(">IIBBBBB", 1, 1, 8, 2, 0, 0, 0))
        + png_chunk(b"IDAT", zlib.compress(raw_scanline))
        + png_chunk(b"IEND", b"")
    )
    with tempfile.TemporaryDirectory() as temporary:
        path = Path(temporary) / "valid.png"
        path.write_bytes(png)
        receipt = png_receipt(path)
        if (receipt.width, receipt.height, receipt.size) != (1, 1, len(png)):
            raise EvidenceError("valid PNG self-test receipt differs")
        corrupted = bytearray(png)
        corrupted[-5] ^= 1
        path.write_bytes(corrupted)
        try:
            png_receipt(path)
        except EvidenceError:
            pass
        else:
            raise EvidenceError("corrupt PNG self-test was accepted")

        output_path = Path(temporary) / "niri-outputs.json"
        output_path.write_text(
            json.dumps(
                {
                    "winit": {
                        "name": "winit",
                        "logical": {"width": 1180, "height": 780, "scale": 1.0},
                    }
                }
            ),
            encoding="utf-8",
        )
        output = niri_output_receipt(output_path, (1180, 780))
        if output != {"name": "winit", "scale": "1.0", "width": 1180, "height": 780}:
            raise EvidenceError("Niri object output self-test receipt differs")
        output_path.write_text(
            json.dumps(
                [
                    {
                        "name": "winit",
                        "logical": {"width": 1180, "height": 780, "scale": 1},
                    }
                ]
            ),
            encoding="utf-8",
        )
        if niri_output_receipt(output_path, (720, 480))["scale"] != "1":
            raise EvidenceError("Niri array output self-test receipt differs")
        output_path.write_text(
            json.dumps(
                {
                    "winit": {
                        "name": "winit",
                        "logical": {"width": 1180, "height": 780, "scale": 2},
                    }
                }
            ),
            encoding="utf-8",
        )
        try:
            niri_output_receipt(output_path, (720, 480))
        except EvidenceError:
            pass
        else:
            raise EvidenceError("scale-2 Niri output self-test was accepted")

        validate_window_payload(
            [{"id": 7, "pid": 9, "app_id": EXPECTED_APP_ID}],
            9,
            7,
            "window-self-test",
        )
        try:
            validate_window_payload([], 9, 7, "empty-window-self-test")
        except EvidenceError:
            pass
        else:
            raise EvidenceError("empty Niri window receipt was accepted")
        try:
            validate_window_payload(
                [{"id": 7, "pid": 9, "app_id": "org.example.Other"}],
                9,
                7,
                "window-self-test",
            )
        except EvidenceError:
            pass
        else:
            raise EvidenceError("foreign app-id window self-test was accepted")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--matrix", type=Path)
    parser.add_argument("--manifest", type=Path)
    parser.add_argument("--screenshots", type=Path)
    parser.add_argument("--metadata", type=Path)
    parser.add_argument("--markers", type=Path)
    parser.add_argument("--niri-outputs", type=Path)
    parser.add_argument("--window-receipts", type=Path)
    parser.add_argument("--repo-root", type=Path)
    parser.add_argument("--receipt", type=Path)
    parser.add_argument("--source-manifest", type=Path)
    parser.add_argument("--require-binary", action="store_true")
    parser.add_argument("--require-logs", action="store_true")
    parser.add_argument("--current-worktree", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        if args.self_test:
            self_test()
            print("capture evidence validator self-test: PASS")
            return 0
        required = ("matrix", "manifest", "screenshots", "metadata", "markers", "repo_root")
        if missing := [name for name in required if getattr(args, name) is None]:
            raise EvidenceError(f"missing required arguments: {', '.join(missing)}")
        payload = validate_bundle(args)
        if args.receipt:
            args.receipt.parent.mkdir(parents=True, exist_ok=True)
            temporary = args.receipt.with_suffix(args.receipt.suffix + ".tmp")
            temporary.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
            temporary.replace(args.receipt)
        print(f"capture evidence validation: PASS ({len(payload['artifacts'])} scenarios)")
        return 0
    except (EvidenceError, OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"capture evidence validation: FAIL: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
