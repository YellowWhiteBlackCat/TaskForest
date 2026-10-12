#!/usr/bin/env python3
"""Validate the captured HealthReview Events footer through screenshot OCR."""

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
    """The captured frame does not prove the complete Events footer."""


ENGLISH_FOOTER_ROWS = (
    "up down groups p alerts e event center",
    "w storage health s sensors q alert rules",
    "f2 filter none f5 export f8 clear",
    "h esc close",
)
EVENT_ACTIONS_PATTERN = re.compile(r"\bf2 filter\b.*\bf5 export\b.*\bf8 clear\b")
SUPPORTED_SIZES = {(120, 36), (54, 16)}
MATRIX_ROWS = {
    "health-review-events": ("performance", "default", "120", "36"),
    "health-review-events-compact": ("performance", "default", "54", "16"),
}


def normalize_line(line: str) -> str:
    return " ".join(re.findall(r"[a-z0-9]+", line.casefold()))


def validate_footer_text(text: str, columns: int, lines: int, language: str) -> list[str]:
    if (columns, lines) not in SUPPORTED_SIZES:
        raise EvidenceError(
            f"unsupported HealthReview Events capture size: {columns}x{lines}"
        )
    if language != "en":
        raise EvidenceError(
            "HealthReview Events OCR requires the English terminal profile"
        )
    normalized = [normalize_line(line) for line in text.splitlines()]
    missing = []
    for index, expected in enumerate(ENGLISH_FOOTER_ROWS):
        if index == 2:
            visible = any(EVENT_ACTIONS_PATTERN.search(line) for line in normalized)
        else:
            visible = any(expected in line for line in normalized)
        if not visible:
            missing.append(expected)
    if missing:
        raise EvidenceError(
            "HealthReview Events footer rows are missing or truncated: "
            + "; ".join(missing)
        )
    return normalized


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
        raise EvidenceError(f"HealthReview Events OCR failed: {error}") from error
    return result.stdout


def validate_capture_matrix(path: Path) -> None:
    with path.open(encoding="utf-8", newline="") as handle:
        rows = {
            row["name"]: row
            for row in csv.DictReader(handle, delimiter="\t")
        }
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
    complete = "\n".join(ENGLISH_FOOTER_ROWS)
    validate_footer_text(complete, 120, 36, "en")
    validate_footer_text(complete, 54, 16, "en")
    for row in ENGLISH_FOOTER_ROWS:
        truncated = complete.replace(row, row.rsplit(" ", 1)[0])
        try:
            validate_footer_text(truncated, 120, 36, "en")
        except EvidenceError:
            pass
        else:
            raise EvidenceError(f"self-test accepted a truncated footer row: {row!r}")
    for invalid in [(120, 24), (80, 24), (54, 24)]:
        try:
            validate_footer_text(complete, *invalid, "en")
        except EvidenceError:
            pass
        else:
            raise EvidenceError(f"self-test accepted unsupported size: {invalid!r}")


def validate_image(
    image: Path,
    columns: int,
    lines: int,
    language: str,
    ocr_output: Path | None,
    receipt_path: Path | None,
) -> dict[str, object]:
    if not image.is_file():
        raise EvidenceError(f"HealthReview Events image is missing: {image}")
    recognized = ocr_text(image)
    normalized = validate_footer_text(recognized, columns, lines, language)
    if ocr_output is not None:
        ocr_output.write_text(recognized, encoding="utf-8")
    receipt: dict[str, object] = {
        "schema_version": 1,
        "status": "pass",
        "scene": "health-review-events",
        "language": language,
        "terminal_columns": columns,
        "terminal_lines": lines,
        "image_sha256": hashlib.sha256(image.read_bytes()).hexdigest(),
        "ocr_sha256": hashlib.sha256(recognized.encode("utf-8")).hexdigest(),
        "footer_rows": normalized,
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
        print("TUI HealthReview Events OCR validator self-test: PASS")
        return 0
    if args.image is None or args.columns is None or args.lines is None:
        parser.error("--image, --columns and --lines are required unless --self-test is used")
    try:
        receipt = validate_image(
            args.image,
            args.columns,
            args.lines,
            args.language,
            args.ocr_output,
            args.receipt,
        )
    except (EvidenceError, OSError, json.JSONDecodeError) as error:
        print(f"TUI HealthReview Events OCR validation: FAIL: {error}", file=sys.stderr)
        return 2
    print(
        "TUI HealthReview Events OCR validation: PASS "
        f"({args.columns}x{args.lines}, language={args.language})"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
