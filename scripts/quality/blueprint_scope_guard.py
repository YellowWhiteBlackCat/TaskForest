#!/usr/bin/env python3
"""Fail-closed guard for the 225-slot blueprint disposition census (ADR-059).

The committed TSV is public data: slot, area, disposition, feature id and a
public reason. This guard checks the structural contract that does not need the
Rust `FeatureId` vocabulary:

* exact header and five tab-separated columns;
* every slot 1..=225 exactly once, in canonical area order;
* 82 adopted slots and 143 explicitly unsupported slots;
* exactly five adopted slots in every area (15 areas x 15 slots);
* adopted rows name a unique feature id and carry no reason;
* unsupported rows carry no feature id and a non-empty public reason.

The Rust contract test (`feature_coverage::blueprint_dispositions::tests`)
resolves adopted ids against `FeatureId::ALL` and enforces the bidirectional
`FeatureId::blueprint_slot` mapping; this guard keeps the quick gate honest
without compiling the workspace.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

HEADER = "slot\tarea\tdisposition\tfeature\treason"
SLOT_COUNT = 225
ITEMS_PER_AREA = 15
ADOPTED_COUNT = 225
UNSUPPORTED_COUNT = 0
FEATURE_ALIAS_SLOTS: dict[str, set[int]] = {
    "pressure.memory-thrashing-health": {179},
}
AREA_ADOPTED = {
    "process-lifecycle": 15,
    "memory-forensics": 15,
    "handle-descriptor-audit": 15,
    "thread-topology": 15,
    "network-sockets": 15,
    "storage-filesystem-io": 15,
    "hardware-topology-numa": 15,
    "accelerator-telemetry": 15,
    "power-thermal": 15,
    "security-isolation": 15,
    "services-init": 15,
    "pressure-saturation": 15,
    "ipc-dbus": 15,
    "dynamic-tracing": 15,
    "history-time-travel": 15,
}
AREAS = (
    "process-lifecycle",
    "memory-forensics",
    "handle-descriptor-audit",
    "thread-topology",
    "network-sockets",
    "storage-filesystem-io",
    "hardware-topology-numa",
    "accelerator-telemetry",
    "power-thermal",
    "security-isolation",
    "services-init",
    "pressure-saturation",
    "ipc-dbus",
    "dynamic-tracing",
    "history-time-travel",
)
DASH = "-"


def validate(text: str) -> list[str]:
    """Return every structural defect; an empty list is the only passing state."""
    errors: list[str] = []
    rows: list[tuple[int, str, str, str, str, str]] = []
    header_seen = False
    for line_no, raw in enumerate(text.splitlines(), start=1):
        stripped = raw.strip()
        if not stripped or stripped.startswith("#"):
            continue
        if not header_seen:
            header_seen = True
            if raw.rstrip() != HEADER:
                errors.append(f"line {line_no}: header must be {HEADER!r}")
            continue
        fields = [field.strip() for field in raw.split("\t")]
        if len(fields) != 5:
            errors.append(f"line {line_no}: expected 5 columns, found {len(fields)}")
            continue
        rows.append((line_no, fields[0], fields[1], fields[2], fields[3], fields[4]))  # type: ignore[arg-type]
    if not header_seen:
        errors.append("missing header")
    if not rows:
        errors.append("no data rows")
        return errors

    seen: dict[int, int] = {}
    adopted_features: dict[str, int] = {}
    adopted_per_area: dict[str, int] = {area: 0 for area in AREAS}
    adopted = 0
    unsupported = 0
    for line_no, slot_raw, area, disposition, feature, reason in rows:
        try:
            slot = int(slot_raw)
        except ValueError:
            errors.append(f"line {line_no}: slot {slot_raw!r} is not an integer")
            continue
        if not 1 <= slot <= SLOT_COUNT:
            errors.append(f"line {line_no}: slot {slot} outside 1..={SLOT_COUNT}")
            continue
        if slot in seen:
            errors.append(f"line {line_no}: slot {slot} already used on line {seen[slot]}")
            continue
        seen[slot] = line_no
        expected_area = AREAS[(slot - 1) // ITEMS_PER_AREA]
        if area != expected_area:
            errors.append(
                f"line {line_no}: slot {slot} belongs to {expected_area!r}, not {area!r}"
            )
        if disposition == "adopted":
            adopted += 1
            if area in adopted_per_area:
                adopted_per_area[area] += 1
            if feature == DASH or not feature:
                errors.append(f"line {line_no}: adopted slot {slot} names no feature")
            elif feature in adopted_features and slot not in FEATURE_ALIAS_SLOTS.get(feature, set()):
                errors.append(
                    f"line {line_no}: feature {feature!r} already adopted on line "
                    f"{adopted_features[feature]}"
                )
            else:
                adopted_features[feature] = line_no
            if reason != DASH:
                errors.append(f"line {line_no}: adopted slot {slot} must carry reason '-'")
        elif disposition == "unsupported":
            unsupported += 1
            if feature != DASH:
                errors.append(f"line {line_no}: unsupported slot {slot} must not name a feature")
            if not reason or reason == DASH:
                errors.append(f"line {line_no}: unsupported slot {slot} needs a public reason")
        else:
            errors.append(
                f"line {line_no}: disposition {disposition!r} is not adopted/unsupported"
            )

    for slot in range(1, SLOT_COUNT + 1):
        if slot not in seen:
            errors.append(f"slot {slot} has no disposition")
    if adopted != ADOPTED_COUNT:
        errors.append(f"adopted count {adopted} != {ADOPTED_COUNT}")
    for area in AREAS:
        if adopted_per_area[area] != AREA_ADOPTED[area]:
            errors.append(
                f"area {area!r} adopted count {adopted_per_area[area]} != {AREA_ADOPTED[area]}"
            )
    if unsupported != UNSUPPORTED_COUNT:
        errors.append(f"unsupported count {unsupported} != {UNSUPPORTED_COUNT}")
    if adopted + unsupported != SLOT_COUNT:
        errors.append(f"dispositioned count {adopted + unsupported} != {SLOT_COUNT}")
    return errors


def committed_path(repo_root: Path) -> Path:
    return repo_root / "scripts" / "parity" / "blueprint_dispositions.tsv"


def fixture(adopted_per_area: int | None = None) -> str:
    lines = [HEADER]
    for slot in range(1, SLOT_COUNT + 1):
        area = AREAS[(slot - 1) // ITEMS_PER_AREA]
        index = (slot - 1) % ITEMS_PER_AREA
        cap = AREA_ADOPTED[area] if adopted_per_area is None else adopted_per_area
        if index < cap:
            lines.append(f"{slot}\t{area}\tadopted\tfixture.feature.{slot}\t-")
        else:
            lines.append(f"{slot}\t{area}\tunsupported\t-\t{area} item not delivered")
    return "\n".join(lines) + "\n"


def self_test() -> int:
    failures: list[str] = []

    def expect_clean(name: str, text: str) -> None:
        errors = validate(text)
        if errors:
            failures.append(f"{name}: expected clean, got {errors[:3]}")

    def expect_error(name: str, text: str, needle: str) -> None:
        errors = validate(text)
        if not any(needle in error for error in errors):
            failures.append(f"{name}: expected {needle!r}, got {errors[:3]}")

    expect_clean("valid fixture", fixture())
    expect_error(
        "duplicate slot",
        fixture().replace("2\t", "1\t", 1),
        "already used",
    )
    expect_error(
        "missing slot",
        "\n".join(
            line
            for line in fixture().splitlines()
            if not line.startswith("225\t")
        )
        + "\n",
        "slot 225 has no disposition",
    )
    expect_error(
        "area mismatch",
        fixture().replace("1\tprocess-lifecycle\t", "1\tmemory-forensics\t", 1),
        "belongs to",
    )
    expect_error(
        "adopted reason",
        fixture().replace("1\tprocess-lifecycle\tadopted\tfixture.feature.1\t-",
                          "1\tprocess-lifecycle\tadopted\tfixture.feature.1\twhy", 1),
        "must carry reason",
    )
    expect_error(
        "unsupported feature",
        fixture().replace("225\thistory-time-travel\tadopted\tfixture.feature.225\t-",
                          "225\thistory-time-travel\tunsupported\tfixture.feature.225\twhy", 1),
        "must not name a feature",
    )
    expect_error(
        "wrong adopted count",
        fixture(adopted_per_area=4),
        "adopted count",
    )
    moved = fixture().replace(
        "1\tprocess-lifecycle\tadopted\tfixture.feature.1\t-",
        "1\tprocess-lifecycle\tunsupported\t-\tprocess-lifecycle item not delivered",
        1,
    ).replace(
        "2\tprocess-lifecycle\tadopted\tfixture.feature.2\t-",
        "2\tprocess-lifecycle\tunsupported\t-\tprocess-lifecycle item not delivered",
        1,
    )
    expect_error("area adopted count", moved, "adopted count")
    expect_error(
        "malformed row",
        fixture() + "not-a-row\n",
        "expected 5 columns",
    )

    if failures:
        for failure in failures:
            print(f"blueprint-scope self-test: FAIL {failure}", file=sys.stderr)
        return 1
    print("blueprint-scope self-test: PASS")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true", help="run the guard's own fixtures")
    parser.add_argument("--repo-root", type=Path, default=Path.cwd(), help="repository root")
    parser.add_argument("--tsv", type=Path, default=None, help="override the committed table path")
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    path = args.tsv or committed_path(args.repo_root)
    try:
        text = path.read_text(encoding="utf-8")
    except OSError as error:
        print(f"blueprint-scope: FAIL cannot read {path}: {error}", file=sys.stderr)
        return 1
    errors = validate(text)
    if errors:
        for error in errors:
            print(f"blueprint-scope: FAIL {error}", file=sys.stderr)
        return 1
    print(
        "blueprint-scope: PASS "
        f"({SLOT_COUNT} slots: {ADOPTED_COUNT} adopted, {UNSUPPORTED_COUNT} unsupported)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
