"""Behavior tests for the fail-closed Windows Iced capture validator."""

from __future__ import annotations

import hashlib
import shutil
import struct
import sys
import tempfile
import unittest
import zlib
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from validate_iced_windows_capture import (
    EvidenceError,
    parse_metadata,
    resolved_renderer_text_stack,
    validate_bundle,
    validate_capture_payload,
)


REPO_ROOT = Path(__file__).resolve().parents[2]


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def png_chunk(kind: bytes, payload: bytes) -> bytes:
    crc = zlib.crc32(kind)
    crc = zlib.crc32(payload, crc) & 0xFFFFFFFF
    return struct.pack(">I", len(payload)) + kind + payload + struct.pack(">I", crc)


def one_pixel_png() -> bytes:
    header = struct.pack(">IIBBBBB", 1, 1, 8, 2, 0, 0, 0)
    pixels = zlib.compress(b"\x00\x27\x68\xa0")
    return (
        b"\x89PNG\r\n\x1a\n"
        + png_chunk(b"IHDR", header)
        + png_chunk(b"IDAT", pixels)
        + png_chunk(b"IEND", b"")
    )


class IcedWindowsCaptureValidatorTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.bundle = Path(self.temporary.name)
        self.image = one_pixel_png()
        (self.bundle / "capture.png").write_bytes(self.image)
        marker = (
            "ICED_CAPTURE_MARKER event=process_ready pid=47211\n"
            "ICED_CAPTURE_MARKER event=frame_ready mode=demo page=performance\n"
            "ICED_CAPTURE_MARKER event=target_ready mode=demo page=performance device=cpu\n"
        )
        (self.bundle / "app-marker.txt").write_text(marker, encoding="utf-8")
        target = {
            "capture_api": "windows.graphics.capture (windows-capture 1.5.0)",
            "target_pid": "47211",
            "window_process_name": "taskforest-i.exe",
            "window_title": "TaskForestI",
            "window_rect_size": "1x1",
            "window_size": "1x1",
            "renderer_active": "unknown",
            "native_text_api": "unknown",
            "text_shaping": "cosmic-text Advanced Auto default; widget overrides may vary",
            "text_rasterizer": "cryoglyph for WGPU; cosmic-text+swash for tiny-skia fallback",
            "text_rasterizer_active": "unknown",
            "font_weight_axis_policy": "directwrite-policy",
            "font_weight_450_name": "Medium",
            "font_weight_450_value": "500",
        }
        target_text = "".join(f"{key}={value}\n" for key, value in target.items())
        (self.bundle / "capture-target.txt").write_text(target_text, encoding="utf-8")
        self.metadata = {
            "frontend": "iced",
            "target": "windows",
            "scenario": "performance-cpu",
            "pair_id": "core-07-performance-cpu-windows-dark-1180x780",
            "theme": "windows-dark",
            "high_contrast": "0",
            "requested_window_size": "1180x780",
            "window_size": "1x1",
            "window_rect_size": "1x1",
            **target,
            "target_pid": "47211",
            "binary": "target/debug/taskforest-i.exe",
            "binary_sha256": "a" * 64,
            "git_head": "a" * 12,
            "worktree": "dirty",
            "rust": "rustc 1.99.0 (test)",
            "cargo_lock_sha256": "b" * 64,
            "renderer_stack": "iced_renderer@0.14.0;iced_wgpu@0.14.0",
            "text_stack": "cryoglyph@0.1.0;cosmic-text@0.15.0",
            "source_scope": "iced",
            "source_manifest": "source-manifest.tsv",
            "source_manifest_sha256": "c" * 64,
            "marker_file": "app-marker.txt",
            "marker_sha256": hashlib.sha256(marker.encode("utf-8")).hexdigest(),
            "target_metadata": "capture-target.txt",
            "target_metadata_sha256": hashlib.sha256(target_text.encode("utf-8")).hexdigest(),
            "peer_receipt": "../gpui/receipt-metadata.txt",
            "peer_capture_metadata": "../gpui/capture-metadata.txt",
            "peer_image": "../gpui/capture.png",
            "peer_source_manifest": "../gpui/source-manifest.tsv",
            "peer_frontend": "gpui",
            "peer_target": "windows",
            "peer_scenario": "performance-cpu",
            "peer_theme": "windows-dark",
            "peer_high_contrast": "0",
            "peer_pair_id": "core-07-performance-cpu-windows-dark-1180x780",
            "peer_requested_window_size": "1180x780",
            "peer_window_size": "1x1",
            "peer_png_sha256": "d" * 64,
            "peer_binary_sha256": "e" * 64,
            "peer_source_manifest_sha256": "f" * 64,
            "timestamp_utc": "2026-10-12T00:00:00Z",
            "png_sha256": hashlib.sha256(self.image).hexdigest(),
        }
        self.write_metadata()

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def write_metadata(self) -> None:
        text = "".join(f"{key}={value}\n" for key, value in self.metadata.items())
        (self.bundle / "capture-metadata.txt").write_text(text, encoding="utf-8")

    def make_complete_bundle(self) -> tuple[Path, Path]:
        repo = self.bundle / "repo"
        repo.mkdir()
        shutil.copyfile(REPO_ROOT / "Cargo.lock", repo / "Cargo.lock")

        pair = repo / "target" / "windows-evidence" / "pair-123"
        iced = pair / "iced"
        gpui = pair / "gpui"
        iced.mkdir(parents=True)
        gpui.mkdir()

        binaries = repo / "target" / "debug"
        binaries.mkdir()
        iced_binary = binaries / "taskforest-i.exe"
        gpui_binary = binaries / "taskforest-g.exe"
        iced_binary.write_bytes(b"synthetic iced binary")
        gpui_binary.write_bytes(b"synthetic gpui binary")

        iced_source = repo / "crates" / "taskmanager-iced" / "src" / "main.rs"
        gpui_source = repo / "crates" / "taskmanager-gpui" / "src" / "main.rs"
        iced_source.parent.mkdir(parents=True)
        gpui_source.parent.mkdir(parents=True)
        iced_source.write_text("fn iced_source() {}\n", encoding="utf-8")
        gpui_source.write_text("fn gpui_source() {}\n", encoding="utf-8")

        def write_manifest(source: Path, destination: Path) -> str:
            relative = source.relative_to(repo).as_posix()
            content = f"{sha256_bytes(source.read_bytes())}  {relative}\n"
            destination.write_text(content, encoding="utf-8")
            return sha256_bytes(content.encode("utf-8"))

        iced_manifest_hash = write_manifest(iced_source, iced / "source-manifest.tsv")
        gpui_manifest_hash = write_manifest(gpui_source, gpui / "source-manifest.tsv")

        (iced / "capture.png").write_bytes(self.image)
        (gpui / "capture.png").write_bytes(self.image)
        marker = (self.bundle / "app-marker.txt").read_bytes()
        target = (self.bundle / "capture-target.txt").read_bytes()
        (iced / "app-marker.txt").write_bytes(marker)
        (iced / "capture-target.txt").write_bytes(target)

        renderer_stack, text_stack = resolved_renderer_text_stack(repo / "Cargo.lock")
        gpui_receipt = {
            "frontend": "gpui",
            "target": "windows",
            "scenario": "performance-cpu",
            "pair_id": "core-07-performance-cpu-windows-dark-1180x780",
            "theme": "windows-dark",
            "high_contrast": "0",
            "requested_window_size": "1180x780",
            "window_size": "1x1",
            "git_head": "a" * 12,
            "worktree": "dirty",
            "rust": "rustc 1.99.0 (test)",
            "binary": "target/debug/taskforest-g.exe",
            "binary_sha256": sha256_bytes(gpui_binary.read_bytes()),
            "source_manifest_sha256": gpui_manifest_hash,
        }
        (gpui / "receipt-metadata.txt").write_text(
            "".join(f"{key}={value}\n" for key, value in gpui_receipt.items()),
            encoding="utf-8",
        )
        (gpui / "capture-metadata.txt").write_text(
            f"window_size=1x1\npng_sha256={sha256_bytes(self.image)}\n",
            encoding="utf-8",
        )

        metadata = self.metadata.copy()
        metadata.update(
            {
                "binary_sha256": sha256_bytes(iced_binary.read_bytes()),
                "cargo_lock_sha256": sha256_bytes((repo / "Cargo.lock").read_bytes()),
                "renderer_stack": renderer_stack,
                "text_stack": text_stack,
                "source_manifest_sha256": iced_manifest_hash,
                "marker_sha256": sha256_bytes(marker),
                "target_metadata_sha256": sha256_bytes(target),
                "peer_png_sha256": sha256_bytes(self.image),
                "peer_binary_sha256": gpui_receipt["binary_sha256"],
                "peer_source_manifest_sha256": gpui_manifest_hash,
            }
        )
        (iced / "capture.png").write_bytes(self.image)
        (iced / "capture-metadata.txt").write_text(
            "".join(f"{key}={value}\n" for key, value in metadata.items()),
            encoding="utf-8",
        )
        return iced, repo

    def test_accepts_a_self_consistent_iced_windows_image(self) -> None:
        values = validate_capture_payload(self.bundle)
        self.assertEqual(values["frontend"], "iced")
        self.assertEqual(values["target"], "windows")

    def test_rejects_a_different_frontend(self) -> None:
        self.metadata["frontend"] = "gpui"
        self.write_metadata()
        with self.assertRaisesRegex(EvidenceError, "unexpected frontend"):
            validate_capture_payload(self.bundle)

    def test_rejects_a_non_windows_target(self) -> None:
        self.metadata["target"] = "linux"
        self.write_metadata()
        with self.assertRaisesRegex(EvidenceError, "unexpected capture target"):
            validate_capture_payload(self.bundle)

    def test_rejects_a_different_process_name(self) -> None:
        self.metadata["window_process_name"] = "taskforest-g.exe"
        self.write_metadata()
        with self.assertRaisesRegex(EvidenceError, "unexpected target process name"):
            validate_capture_payload(self.bundle)

    def test_rejects_missing_provenance(self) -> None:
        del self.metadata["git_head"]
        self.write_metadata()
        with self.assertRaisesRegex(EvidenceError, "metadata missing fields"):
            validate_capture_payload(self.bundle)

    def test_rejects_a_missing_png_hash(self) -> None:
        del self.metadata["png_sha256"]
        self.write_metadata()
        with self.assertRaisesRegex(EvidenceError, "metadata missing fields"):
            validate_capture_payload(self.bundle)

    def test_rejects_an_inconsistent_png_hash(self) -> None:
        self.metadata["png_sha256"] = "0" * 64
        self.write_metadata()
        with self.assertRaisesRegex(EvidenceError, "capture.png hash differs"):
            validate_capture_payload(self.bundle)

    def test_rejects_a_pid_that_does_not_match_the_app_marker(self) -> None:
        self.metadata["target_pid"] = "47212"
        self.write_metadata()
        with self.assertRaisesRegex(EvidenceError, "PID emitted by the Iced app"):
            validate_capture_payload(self.bundle)

    def test_rejects_a_requested_size_outside_the_paired_route(self) -> None:
        self.metadata["requested_window_size"] = "1280x800"
        self.write_metadata()
        with self.assertRaisesRegex(EvidenceError, "unsupported paired requested window size"):
            validate_capture_payload(self.bundle)

    def test_validates_the_complete_pair_and_source_provenance(self) -> None:
        iced, repo = self.make_complete_bundle()
        values = validate_bundle(iced, repo, check_current=False)
        self.assertEqual(values["frontend"], "iced")
        self.assertEqual(values["peer_frontend"], "gpui")

    def test_rejects_a_gpui_peer_with_a_different_pair_id(self) -> None:
        iced, repo = self.make_complete_bundle()
        peer_receipt = iced.parent / "gpui" / "receipt-metadata.txt"
        peer = parse_metadata(peer_receipt)
        peer["pair_id"] = "different-pair"
        peer_receipt.write_text(
            "".join(f"{key}={value}\n" for key, value in peer.items()),
            encoding="utf-8",
        )
        with self.assertRaisesRegex(EvidenceError, "GPUI peer differs from Iced on pair_id"):
            validate_bundle(iced, repo, check_current=False)

    def test_rejects_peer_artifacts_that_escape_the_pair_directory(self) -> None:
        iced, repo = self.make_complete_bundle()
        metadata = parse_metadata(iced / "capture-metadata.txt")
        metadata["peer_image"] = "../../outside.png"
        (iced / "capture-metadata.txt").write_text(
            "".join(f"{key}={value}\n" for key, value in metadata.items()),
            encoding="utf-8",
        )
        with self.assertRaisesRegex(EvidenceError, "peer_image escapes the pair directory"):
            validate_bundle(iced, repo, check_current=False)

    def test_rejects_a_binary_changed_after_receipt_creation(self) -> None:
        iced, repo = self.make_complete_bundle()
        (repo / "target" / "debug" / "taskforest-i.exe").write_bytes(b"changed binary")
        with self.assertRaisesRegex(EvidenceError, "Iced binary hash differs"):
            validate_bundle(iced, repo, check_current=False)

    def test_rejects_source_changed_after_receipt_creation(self) -> None:
        iced, repo = self.make_complete_bundle()
        (repo / "crates" / "taskmanager-iced" / "src" / "main.rs").write_text(
            "fn changed_source() {}\n", encoding="utf-8"
        )
        with self.assertRaisesRegex(EvidenceError, "source provenance mismatch"):
            validate_bundle(iced, repo, check_current=False)

    def test_rejects_stale_current_git_provenance(self) -> None:
        iced, repo = self.make_complete_bundle()
        with patch("validate_iced_windows_capture.current", return_value="changed"):
            with self.assertRaisesRegex(EvidenceError, "provenance differs from the current"):
                validate_bundle(iced, repo, check_current=True)


if __name__ == "__main__":
    unittest.main()
