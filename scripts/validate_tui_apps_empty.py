#!/usr/bin/env python3
"""Validate the Applications empty-state capture and its query boundary."""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import os
import re
import subprocess
import sys
from pathlib import Path


class EvidenceError(RuntimeError):
    """The captured Applications frame does not prove the empty state."""


SUPPORTED_SIZES = {(120, 36), (54, 16)}
MATRIX_ROWS = {
    "apps-empty": ("applications", "default", "120", "36"),
    "apps-empty-compact": ("applications", "default", "54", "16"),
}
REPORTED_STATE = "no processes reported yet"
QUERY_STATE = "no processes match the query"


def normalize_text(text: str) -> str:
    return " ".join(re.findall(r"[a-z0-9]+", text.casefold()))


def validate_text(text: str, columns: int, lines: int, language: str) -> str:
    if (columns, lines) not in SUPPORTED_SIZES:
        raise EvidenceError(f"unsupported Applications empty capture size: {columns}x{lines}")
    if language != "en":
        raise EvidenceError("Applications empty-state OCR requires the English terminal profile")
    normalized = normalize_text(text)
    if REPORTED_STATE not in normalized:
        raise EvidenceError("the complete 'No processes reported yet' state is missing or truncated")
    if QUERY_STATE in normalized:
        raise EvidenceError("the query-mismatch state appeared without a query")
    return normalized


def validate_capture_matrix(path: Path) -> None:
    with path.open(encoding="utf-8", newline="") as handle:
        rows = {row["name"]: row for row in csv.DictReader(handle, delimiter="\t")}
    for name, expected in MATRIX_ROWS.items():
        row = rows.get(name)
        if row is None:
            raise EvidenceError(f"capture matrix is missing scenario {name!r}")
        actual = (row["page"], row["device"], row["columns"], row["lines"])
        if actual != expected:
            raise EvidenceError(
                f"capture matrix scenario {name!r} has {actual!r}, expected {expected!r}"
            )


def self_test() -> None:
    repository = Path(__file__).resolve().parent.parent
    validate_capture_matrix(repository / "scripts" / "capture_tui_scenarios.tsv")
    complete = "No processes reported yet."
    for columns, lines in sorted(SUPPORTED_SIZES):
        validate_text(complete, columns, lines, "en")
    for invalid in ["No processes reported", QUERY_STATE, ""]:
        try:
            validate_text(invalid, 120, 36, "en")
        except EvidenceError:
            pass
        else:
            raise EvidenceError(f"self-test accepted an invalid state: {invalid!r}")
    for invalid in [(120, 24), (80, 24), (54, 24)]:
        try:
            validate_text(complete, *invalid, "en")
        except EvidenceError:
            pass
        else:
            raise EvidenceError(f"self-test accepted unsupported size: {invalid!r}")


def ocr_text(image: Path) -> str:
    environment = os.environ.copy()
    environment.update(OMP_THREAD_LIMIT="1", OMP_NUM_THREADS="1")
    try:
        result = subprocess.run(
            ["tesseract", str(image), "stdout", "--psm", "6", "-l", "eng"],
            env=environment,
            check=True,
            text=True,
            capture_output=True,
            timeout=15,
        )
    except (OSError, subprocess.SubprocessError) as error:
        raise EvidenceError(f"Applications empty-state OCR failed: {error}") from error
    return result.stdout


def validate_image(
    image: Path,
    columns: int,
    lines: int,
    language: str,
    ocr_output: Path | None,
    receipt_path: Path | None,
) -> dict[str, object]:
    if not image.is_file():
        raise EvidenceError(f"Applications empty-state image is missing: {image}")
    recognized = ocr_text(image)
    normalized = validate_text(recognized, columns, lines, language)
    if ocr_output is not None:
        ocr_output.write_text(recognized, encoding="utf-8")
    receipt: dict[str, object] = {
        "schema_version": 1,
        "status": "pass",
        "scene": "apps-empty",
        "language": language,
        "terminal_columns": columns,
        "terminal_lines": lines,
        "expected_state": "reported_empty",
        "prohibited_state": "query_mismatch",
        "image_sha256": hashlib.sha256(image.read_bytes()).hexdigest(),
        "ocr_sha256": hashlib.sha256(recognized.encode("utf-8")).hexdigest(),
        "normalized_ocr": normalized,
    }
    if receipt_path is not None:
        receipt_path.write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    return receipt


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--image", type=Path)
    parser.add_argument("--columns", type=int)
    parser.add_argument("--lines", type=int)
    parser.add_argument("--language", choices=("en", "zh"), default="en")
    parser.add_argument("--ocr-output", type=Path)
    parser.add_argument("--receipt", type=Path)
    args = parser.parse_args(argv)
    if args.self_test:
        self_test()
        print("TUI Applications empty-state validator self-test: PASS")
        return 0
    if args.image is None or args.columns is None or args.lines is None:
        parser.error("--image, --columns and --lines are required unless --self-test is used")
    try:
        validate_image(
            args.image,
            args.columns,
            args.lines,
            args.language,
            args.ocr_output,
            args.receipt,
        )
    except (EvidenceError, OSError, json.JSONDecodeError) as error:
        print(f"TUI Applications empty-state validation: FAIL: {error}", file=sys.stderr)
        return 2
    print(
        "TUI Applications empty-state validation: PASS "
        f"({args.columns}x{args.lines}, language={args.language})"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
