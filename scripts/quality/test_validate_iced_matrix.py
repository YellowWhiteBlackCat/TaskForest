"""Behavior tests for Iced capture marker routing."""

from __future__ import annotations

import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from validate_capture_evidence import EvidenceError
from validate_iced_matrix import validate_markers


class IcedMatrixMarkerRoutingTests(unittest.TestCase):
    def test_session_confirmation_devices_expect_users_page_markers(self) -> None:
        for device in (
            "users-session-confirmation",
            "users-session-confirmation-compact",
        ):
            with self.subTest(device=device), tempfile.TemporaryDirectory() as directory:
                markers = Path(directory) / "markers.log"
                markers.write_text(
                    "\n".join(
                        (
                            "ICED_CAPTURE_MARKER event=frame_ready mode=demo page=users",
                            f"ICED_CAPTURE_MARKER event=target_ready mode=demo page=users device={device}",
                        )
                    )
                    + "\n",
                    encoding="utf-8",
                )

                validate_markers(markers, device)

    def test_cpu_viewport_marker_proves_client_size_and_typed_budget(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            markers = Path(directory) / "markers.log"
            markers.write_text(
                "\n".join(
                    (
                        "ICED_CAPTURE_MARKER event=frame_ready mode=demo page=performance",
                        "ICED_CAPTURE_MARKER event=target_ready mode=demo page=performance device=cpu",
                        "ICED_CAPTURE_MARKER event=viewport_ready mode=demo page=performance device=cpu width=1180.00 height=780.00 page_width=1180.00 page_height=780.00 rung=Charts inventory=Full",
                    )
                )
                + "\n",
                encoding="utf-8",
            )

            validate_markers(markers, "cpu", "1180x780")

            fractional = markers.read_text(encoding="utf-8").replace(
                "width=1180.00 height=780.00 page_width=1180.00 page_height=780.00",
                "width=1180.38 height=779.62 page_width=1180.38 page_height=779.62",
            )
            markers.write_text(fractional, encoding="utf-8")
            validate_markers(markers, "cpu", "1180x780")

            with self.assertRaisesRegex(EvidenceError, "differs from requested"):
                validate_markers(markers, "cpu", "1181x780")

            compact = markers.read_text(encoding="utf-8").replace(
                "width=1180.38 height=779.62 page_width=1180.38 page_height=779.62 rung=Charts inventory=Full",
                "width=720.00 height=480.00 page_width=720.00 page_height=480.00 rung=Floor inventory=AggregateOnly",
            )
            markers.write_text(compact, encoding="utf-8")
            validate_markers(markers, "cpu", "720x480")

            markers.write_text(
                compact.replace("rung=Floor inventory=AggregateOnly", "rung=Charts inventory=Full"),
                encoding="utf-8",
            )
            with self.assertRaisesRegex(EvidenceError, "expected rung/inventory"):
                validate_markers(markers, "cpu", "720x480")
            markers.write_text(
                compact.replace(" inventory=AggregateOnly", ""), encoding="utf-8"
            )
            with self.assertRaisesRegex(EvidenceError, "lacks a valid rung/inventory"):
                validate_markers(markers, "cpu", "720x480")

    def test_process_context_menu_requires_its_mounted_surface_marker(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            markers = Path(directory) / "markers.log"
            markers.write_text(
                "\n".join(
                    (
                        "ICED_CAPTURE_MARKER event=frame_ready mode=demo page=applications",
                        "ICED_CAPTURE_MARKER event=target_ready mode=demo page=applications device=process-context-menu",
                        "ICED_CAPTURE_MARKER event=surface_ready mode=demo page=applications surface=process-context-menu pid=4321 start_token=7",
                    )
                )
                + "\n",
                encoding="utf-8",
            )
            validate_markers(markers, "process-context-menu")

            markers.write_text(
                "\n".join(
                    (
                        "ICED_CAPTURE_MARKER event=frame_ready mode=demo page=applications",
                        "ICED_CAPTURE_MARKER event=target_ready mode=demo page=applications device=process-context-menu",
                    )
                )
                + "\n",
                encoding="utf-8",
            )
            with self.assertRaisesRegex(EvidenceError, "mounted Process context-menu"):
                validate_markers(markers, "process-context-menu")

            markers.write_text(
                "\n".join(
                    (
                        "ICED_CAPTURE_MARKER event=frame_ready mode=demo page=applications",
                        "ICED_CAPTURE_MARKER event=target_ready mode=demo page=applications device=process-context-menu",
                        "ICED_CAPTURE_MARKER event=surface_ready mode=demo page=applications surface=process-context-menu pid=4321",
                    )
                )
                + "\n",
                encoding="utf-8",
            )
            with self.assertRaisesRegex(EvidenceError, "lacks a live identity"):
                validate_markers(markers, "process-context-menu")

    def test_cpu_facts_requires_typed_projection_marker_and_budget(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            markers = Path(directory) / "markers.log"
            markers.write_text(
                "\n".join(
                    (
                        "ICED_CAPTURE_MARKER event=frame_ready mode=demo page=performance",
                        "ICED_CAPTURE_MARKER event=target_ready mode=demo page=performance device=cpu-facts",
                        "ICED_CAPTURE_MARKER event=viewport_ready mode=demo page=performance device=cpu-facts width=1180.00 height=780.00 page_width=1180.00 page_height=780.00 rung=Charts inventory=Full",
                        "ICED_CAPTURE_MARKER event=cpu_facts_ready mode=demo page=performance device=cpu-facts rail=stats-end core_breakdown=6/8/2 cache_kib=64/64/1024/16384 base_frequency_mhz=2400 core_counts=16/22 virtualization=VT-x core_temperature_c=55/59",
                    )
                )
                + "\n",
                encoding="utf-8",
            )
            validate_markers(markers, "cpu-facts", "1180x780")

            compact = markers.read_text(encoding="utf-8").replace(
                "width=1180.00 height=780.00 page_width=1180.00 page_height=780.00 rung=Charts inventory=Full",
                "width=720.00 height=480.00 page_width=720.00 page_height=480.00 rung=Floor inventory=AggregateOnly",
            )
            markers.write_text(compact, encoding="utf-8")
            validate_markers(markers, "cpu-facts", "720x480")

            markers.write_text(compact.replace(" rail=stats-end", ""), encoding="utf-8")
            with self.assertRaisesRegex(EvidenceError, "typed CPU facts readiness"):
                validate_markers(markers, "cpu-facts", "720x480")

            markers.write_text(compact.replace("core_breakdown=6/8/2", "core_breakdown=22/0/0"), encoding="utf-8")
            with self.assertRaisesRegex(EvidenceError, "typed CPU facts readiness"):
                validate_markers(markers, "cpu-facts", "720x480")


if __name__ == "__main__":
    unittest.main()
