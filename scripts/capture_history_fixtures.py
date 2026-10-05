#!/usr/bin/env python3
"""Seed private replay JSONL inputs consumed by the real app-host query worker."""

import argparse
import json
import time
from pathlib import Path


def scope_token(value: str) -> str:
    if value == "-":
        return "%2D"
    return "".join(chr(byte) if chr(byte) in "abcdefghijklmnopqrstuvwxyz0123456789.-" else f"%{byte:02X}" for byte in value.encode("utf-8"))


def seed(directory: Path, kind: str) -> None:
    directory.mkdir(parents=True, exist_ok=True)
    now_ms = int(time.time() * 1000)
    span_ms = 23 * 3600 * 1000
    points = 3600

    def values(base: float, amplitude: float, period: int, integral: bool = False) -> list[dict]:
        records = []
        for index in range(points):
            value = base + amplitude * ((index % period) / period)
            at = now_ms - span_ms + index * span_ms // (points - 1)
            records.append({"r": index + 1, "c": at, "m": at, "v": round(value) if integral else round(value, 3)})
        return records

    if kind == "system":
        series = {
            "cpu-usage-pct__-__-": values(12.0, 55.0, 137),
            "memory-used-pct__-__-": values(38.0, 22.0, 251),
            "swap-used-pct__-__-": values(4.0, 9.0, 193),
            "network-rate-bps__-__-": values(2_000_000.0, 28_000_000.0, 97),
            "gpu-usage-pct__-__-": values(8.0, 40.0, 113),
        }
    else:
        series = {}
        applications = [
            ("launcher:org.mozilla.firefox", 32.0, 1_180_000_000.0, 12.0),
            ("launcher:com.google.Chrome", 24.0, 2_620_000_000.0, 26.0),
            ("launcher:com.visualstudio.code", 18.0, 1_040_000_000.0, 9.0),
            ("launcher:io.example.TaskForest", 11.0, 168_000_000.0, 1.0),
            ("process:example-worker", 6.0, 58_000_000.0, 1.0),
        ]
        for index, (identity, cpu, memory, count) in enumerate(applications):
            series[f"application-cpu-usage-pct__-__-__{scope_token(identity)}"] = values(cpu, 18.0, 89 + index)
            series[f"application-memory-bytes__-__-__{scope_token(identity)}"] = values(memory, memory * 0.18, 137 + index)
            series[f"application-process-count__-__-__{scope_token(identity)}"] = values(count, 2.0, 181 + index, True)
    for stem, records in series.items():
        with (directory / f"{stem}.jsonl").open("w", encoding="utf-8") as output:
            for record in records:
                output.write(json.dumps(record, separators=(",", ":")) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", required=True, type=Path)
    parser.add_argument("--kind", required=True, choices=("system", "application"))
    options = parser.parse_args()
    seed(options.directory, options.kind)
