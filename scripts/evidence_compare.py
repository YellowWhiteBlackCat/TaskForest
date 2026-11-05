#!/usr/bin/env python3
"""Put a reference frame and a candidate frame side by side and measure them.

The capture evidence separates "what the product must look like" (the
promoted reference) from "what the candidate renders today". This tool turns
that pair into one readable artifact: a labeled composite, an amplified
difference map over the overlapping region, and the numbers behind them.

The two frontends do not share window chrome, so a whole-frame pixel delta
mostly measures the frame. `--region` restricts the comparison to a relative
content rectangle (fractions of each image) so the measurement is about the
page, not the window.

Usage:
    python3 scripts/evidence_compare.py --reference a.png --candidate b.png \
        --out target/compare/perf-memory.png
    python3 scripts/evidence_compare.py --reference a.png --candidate b.png \
        --out target/compare/perf-memory.png --region 0.02,0.20,0.72,0.75
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw

CAPTION_HEIGHT = 34
GAP = 12
BACKGROUND = (24, 26, 30)
CAPTION_COLOR = (233, 235, 239)


class CompareError(RuntimeError):
    """A fail-closed comparison error."""


def load(path: Path) -> Image.Image:
    """Load a frame as RGB."""
    if not path.is_file():
        raise CompareError(f"frame is missing: {path}")
    with Image.open(path) as image:
        return image.convert("RGB")


def region_box(size: tuple[int, int], region: str | None) -> tuple[int, int, int, int]:
    """Return the pixel box of a relative content rectangle."""
    width, height = size
    if not region:
        return (0, 0, width, height)
    parts = region.split(",")
    if len(parts) != 4:
        raise CompareError("--region expects left,top,width,height as fractions")
    try:
        left, top, span_width, span_height = (float(part) for part in parts)
    except ValueError as error:
        raise CompareError("--region expects four numbers") from error
    box = (
        max(0, int(left * width)),
        max(0, int(top * height)),
        max(1, int(span_width * width)),
        max(1, int(span_height * height)),
    )
    return box


def difference(left: np.ndarray, right: np.ndarray) -> np.ndarray:
    """Return the amplified per-pixel difference of two equally sized blocks."""
    delta = np.abs(left.astype(np.int16) - right.astype(np.int16)).max(axis=2)
    return np.clip(delta.astype(np.float32) * 4.0, 0, 255).astype(np.uint8)


def metrics(left: np.ndarray, right: np.ndarray) -> dict[str, float]:
    """Return the comparison numbers of two equally sized blocks."""
    delta = np.abs(left.astype(np.int16) - right.astype(np.int16)).astype(np.float32)
    per_pixel = delta.max(axis=2)
    return {
        "mean_channel_delta": float(delta.mean()),
        "max_channel_delta": float(delta.max()),
        "pixels_over_8": float((per_pixel > 8).mean() * 100.0),
        "pixels_over_32": float((per_pixel > 32).mean() * 100.0),
    }


def caption(draw: ImageDraw.ImageDraw, x: int, y: int, width: int, text: str) -> None:
    """Draw one caption line under a frame."""
    draw.text((x + 4, y + 10), text, fill=CAPTION_COLOR)


def build(
    reference: Image.Image,
    candidate: Image.Image,
    reference_label: str,
    candidate_label: str,
) -> tuple[Image.Image, Image.Image]:
    """Return the composite and the difference map."""
    height = max(reference.height, candidate.height)
    reference_padded = Image.new("RGB", (reference.width, height), BACKGROUND)
    reference_padded.paste(reference, (0, 0))
    candidate_padded = Image.new("RGB", (candidate.width, height), BACKGROUND)
    candidate_padded.paste(candidate, (0, 0))
    composite = Image.new(
        "RGB",
        (reference_padded.width + GAP + candidate_padded.width, height + CAPTION_HEIGHT),
        BACKGROUND,
    )
    draw = ImageDraw.Draw(composite)
    composite.paste(reference_padded, (0, CAPTION_HEIGHT))
    composite.paste(candidate_padded, (reference_padded.width + GAP, CAPTION_HEIGHT))
    caption(draw, 0, 0, reference_padded.width, reference_label)
    caption(
        draw,
        reference_padded.width + GAP,
        0,
        candidate_padded.width,
        candidate_label,
    )
    overlap_width = min(reference.width, candidate.width)
    overlap_height = min(reference.height, candidate.height)
    left = np.asarray(reference_padded.crop((0, 0, overlap_width, overlap_height)))
    right = np.asarray(candidate_padded.crop((0, 0, overlap_width, overlap_height)))
    delta = Image.fromarray(difference(left, right)).convert("RGB")
    return composite, delta


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", type=Path, required=True)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--region", help="left,top,width,height as fractions")
    parser.add_argument("--reference-label")
    parser.add_argument("--candidate-label")
    args = parser.parse_args(argv)

    try:
        reference = load(args.reference)
        candidate = load(args.candidate)
        box = region_box(reference.size, args.region)
        candidate_box = region_box(candidate.size, args.region)
        reference_block = reference.crop(box)
        candidate_block = candidate.crop(candidate_box)
        span = (
            min(reference_block.width, candidate_block.width),
            min(reference_block.height, candidate_block.height),
        )
        left = np.asarray(reference_block.crop((0, 0, span[0], span[1])))
        right = np.asarray(candidate_block.crop((0, 0, span[0], span[1])))
        numbers = metrics(left, right)
        composite, delta = build(
            reference,
            candidate,
            args.reference_label or f"reference {args.reference.name}",
            args.candidate_label or f"candidate {args.candidate.name}",
        )
        args.out.parent.mkdir(parents=True, exist_ok=True)
        composite.save(args.out)
        delta.save(args.out.with_suffix(".diff.png"))
    except (CompareError, OSError, ValueError) as error:
        print(f"evidence compare: FAIL: {error}", file=sys.stderr)
        return 2

    print(f"evidence compare: {args.out}")
    print(f"  reference_size={reference.width}x{reference.height}")
    print(f"  candidate_size={candidate.width}x{candidate.height}")
    print(f"  compared_region={span[0]}x{span[1]}")
    print(f"  mean_channel_delta={numbers['mean_channel_delta']:.3f}")
    print(f"  max_channel_delta={numbers['max_channel_delta']:.0f}")
    print(f"  pixels_over_8={numbers['pixels_over_8']:.2f}%")
    print(f"  pixels_over_32={numbers['pixels_over_32']:.2f}%")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
