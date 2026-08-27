#!/usr/bin/env python3
"""Validate a redaction-safe Linux target-environment capability receipt.

This validator checks receipt provenance and honesty. Optional --require
arguments let a target-host job fail closed unless the expected capability is
eligible; capability eligibility is still not provider-operation evidence.
"""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path
from typing import Any

SCHEMA_VERSION = 6
EBPF_ABI_VERSION = 3
TARGETS = {
    "ebpf_process_rates",
    "nvidia_gpu",
    "amd_gpu",
    "ata_smart",
    "sas_smart",
    "usb_smart",
    "openrc",
    "at_spi",
    "hotplug",
    "intel_gpu_engine_pmu",
}
STATUSES = {
    "eligible",
    "hardware_not_detected",
    "tool_missing",
    "backend_not_compiled",
    "backend_inactive",
    "privilege_required",
    "backend_unconfirmed",
    "unsupported_platform",
}
FORBIDDEN_KEYS = {
    "serial",
    "hostname",
    "device_path",
    "model",
    "command_output",
}
INIT_EVIDENCE = {
    "systemd_pid1",
    "openrc_runtime",
    "unknown_pid1",
}
TOP_LEVEL_STATUS_FIELDS = {
    "ata_smart_probe",
    "nvidia_nvml_probe",
    "openrc_probe",
    "systemd_probe",
}
TARGET_BOOLEAN_FIELDS = {
    "kernel_btf_available",
    "cgroup_v2_available",
    "effective_bpf_privilege",
    "ebpf_compat_environment_available",
    "ebpf_compat_probe_permission_required",
    "at_spi_session_detected",
    "effective_perfmon_privilege",
}
TARGET_COUNT_FIELDS = {
    "amd_device_markers",
    "sas_candidate_devices",
    "usb_candidate_devices",
    "intel_gpu_engine_pmu_devices",
}
TOP_LEVEL_BOOLEAN_FIELDS = {
    "smartctl_available",
    "nvme_cli_available",
    "systemctl_available",
    "openrc_tools_available",
    "nvidia_backend_compiled",
}
TOP_LEVEL_COUNT_FIELDS = {
    "ata_candidate_devices",
    "nvme_namespace_devices",
    "nvidia_device_markers",
}
MIRRORED_STATUSES = {
    "ata_smart": "ata_smart_probe",
    "nvidia_gpu": "nvidia_nvml_probe",
    "openrc": "openrc_probe",
}
SHA256 = re.compile(r"[0-9a-f]{64}", flags=re.ASCII)


def parse_requirement(value: str) -> tuple[str, str]:
    target, separator, status = value.partition("=")
    if not separator or target not in TARGETS or status not in STATUSES:
        choices = ", ".join(sorted(TARGETS))
        raise argparse.ArgumentTypeError(
            f"expected TARGET=STATUS; target must be one of: {choices}"
        )
    return target, status


def walk_keys(value: Any) -> set[str]:
    if isinstance(value, dict):
        keys = set(value)
        for child in value.values():
            keys.update(walk_keys(child))
        return keys
    if isinstance(value, list):
        keys: set[str] = set()
        for child in value:
            keys.update(walk_keys(child))
        return keys
    return set()


def is_integer(value: Any) -> bool:
    return isinstance(value, int) and not isinstance(value, bool)


def validate_typed_fields(
    value: dict[str, Any],
    *,
    booleans: set[str],
    counts: set[str],
    prefix: str,
) -> list[str]:
    errors: list[str] = []
    for name in sorted(booleans):
        if not isinstance(value.get(name), bool):
            errors.append(f"{prefix}{name} must be a boolean")
    for name in sorted(counts):
        item = value.get(name)
        if not is_integer(item) or item < 0:
            errors.append(f"{prefix}{name} must be a non-negative integer")
    return errors


def validate_ebpf_object(value: Any) -> list[str]:
    # The pure-safe-Rust build embeds no eBPF object (ADR-021); null is honest.
    if value is None:
        return []
    if not isinstance(value, dict):
        return ["ebpf_object must be null or an object"]
    errors: list[str] = []
    if value.get("abi_version") != EBPF_ABI_VERSION:
        errors.append(f"ebpf_object.abi_version must be {EBPF_ABI_VERSION}")
    for name in ("source_sha256", "object_sha256"):
        digest = value.get(name)
        if not isinstance(digest, str) or SHA256.fullmatch(digest) is None:
            errors.append(f"ebpf_object.{name} must be a lowercase SHA-256 digest")
    size = value.get("object_size")
    if not is_integer(size) or size <= 0:
        errors.append("ebpf_object.object_size must be a positive integer")
    return errors


def validate(
    receipt: dict[str, Any], requirements: list[tuple[str, str]]
) -> list[str]:
    errors: list[str] = []
    if receipt.get("schema_version") != SCHEMA_VERSION:
        errors.append(f"schema_version must be {SCHEMA_VERSION}")
    if receipt.get("source") != "live_host":
        errors.append("source must be live_host; fixture receipts are not target evidence")
    if receipt.get("capability_only") is not True:
        errors.append("capability_only must be true")
    if receipt.get("hardware_build_profile") != "standard_all":
        errors.append("target receipt must come from the standard all-hardware artifact")
    if (
        receipt.get("hardware_build_profile") == "standard_all"
        and receipt.get("nvidia_backend_compiled") is not True
    ):
        errors.append(
            "standard all-hardware receipt must include the current NVIDIA backend"
        )
    observed_at = receipt.get("observed_at_unix_ms")
    if not is_integer(observed_at) or observed_at <= 0:
        errors.append("observed_at_unix_ms must be a positive integer")
    if receipt.get("init_evidence") not in INIT_EVIDENCE:
        errors.append("init_evidence must identify systemd, OpenRC, or unknown PID 1")
    errors.extend(
        validate_typed_fields(
            receipt,
            booleans=TOP_LEVEL_BOOLEAN_FIELDS,
            counts=TOP_LEVEL_COUNT_FIELDS,
            prefix="",
        )
    )
    for name in sorted(TOP_LEVEL_STATUS_FIELDS):
        status = receipt.get(name)
        if status not in STATUSES:
            errors.append(f"{name} has invalid status: {status!r}")
    errors.extend(validate_ebpf_object(receipt.get("ebpf_object")))

    target_environment = receipt.get("target_environment")
    if not isinstance(target_environment, dict):
        errors.append("target_environment object is missing")
        target_environment = {}
    missing = TARGETS.difference(target_environment)
    if missing:
        errors.append(f"target_environment is missing: {', '.join(sorted(missing))}")
    for target in TARGETS.intersection(target_environment):
        status = target_environment[target]
        if status not in STATUSES:
            errors.append(f"{target} has invalid status: {status!r}")
    errors.extend(
        validate_typed_fields(
            target_environment,
            booleans=TARGET_BOOLEAN_FIELDS,
            counts=TARGET_COUNT_FIELDS,
            prefix="target_environment.",
        )
    )
    unprivileged_bpf = target_environment.get("unprivileged_bpf_disabled")
    if unprivileged_bpf is not None and (
        not is_integer(unprivileged_bpf) or unprivileged_bpf not in range(3)
    ):
        errors.append(
            "target_environment.unprivileged_bpf_disabled must be null, 0, 1, or 2"
        )
    perf_event_paranoid = target_environment.get("perf_event_paranoid")
    if perf_event_paranoid is not None and not is_integer(perf_event_paranoid):
        errors.append("target_environment.perf_event_paranoid must be null or an integer")
    pmu_status = target_environment.get("intel_gpu_engine_pmu")
    pmu_devices = target_environment.get("intel_gpu_engine_pmu_devices")
    perfmon_privilege = target_environment.get("effective_perfmon_privilege")
    if pmu_status == "hardware_not_detected" and pmu_devices != 0:
        errors.append(
            "target_environment.intel_gpu_engine_pmu hardware_not_detected requires zero devices"
        )
    if pmu_status == "eligible" and isinstance(pmu_devices, int) and pmu_devices == 0:
        errors.append(
            "target_environment.intel_gpu_engine_pmu eligible requires at least one device"
        )
    if pmu_status == "privilege_required" and not (
        isinstance(pmu_devices, int)
        and pmu_devices > 0
        and perfmon_privilege is False
        and isinstance(perf_event_paranoid, int)
        and perf_event_paranoid >= 2
    ):
        errors.append(
            "target_environment.intel_gpu_engine_pmu privilege_required requires a PMU, "
            "paranoid >= 2, and no effective perf privilege"
        )
    for target, top_level in MIRRORED_STATUSES.items():
        if target_environment.get(target) != receipt.get(top_level):
            errors.append(f"target_environment.{target} must match {top_level}")
    for target, expected in requirements:
        actual = target_environment.get(target)
        if actual != expected:
            errors.append(f"{target} expected {expected!r}, got {actual!r}")

    forbidden = FORBIDDEN_KEYS.intersection(walk_keys(receipt))
    if forbidden:
        errors.append(f"receipt contains forbidden keys: {', '.join(sorted(forbidden))}")
    return errors


def valid_fixture() -> dict[str, Any]:
    target_environment: dict[str, Any] = {
        "kernel_btf_available": True,
        "cgroup_v2_available": True,
        "unprivileged_bpf_disabled": 2,
        "effective_bpf_privilege": False,
        "ebpf_compat_environment_available": False,
        "ebpf_compat_probe_permission_required": False,
        "amd_device_markers": 0,
        "sas_candidate_devices": 0,
        "usb_candidate_devices": 0,
        "at_spi_session_detected": False,
        "ebpf_process_rates": "backend_not_compiled",
        "nvidia_gpu": "hardware_not_detected",
        "amd_gpu": "hardware_not_detected",
        "ata_smart": "hardware_not_detected",
        "sas_smart": "hardware_not_detected",
        "usb_smart": "hardware_not_detected",
        "openrc": "backend_unconfirmed",
        "at_spi": "backend_not_compiled",
        "hotplug": "backend_unconfirmed",
        "intel_gpu_engine_pmu": "privilege_required",
        "intel_gpu_engine_pmu_devices": 1,
        "effective_perfmon_privilege": False,
        "perf_event_paranoid": 2,
    }
    return {
        "schema_version": SCHEMA_VERSION,
        "source": "live_host",
        "capability_only": True,
        "observed_at_unix_ms": 1,
        "hardware_build_profile": "standard_all",
        "init_evidence": "unknown_pid1",
        "ata_candidate_devices": 0,
        "nvme_namespace_devices": 1,
        "nvidia_device_markers": 0,
        "smartctl_available": True,
        "nvme_cli_available": False,
        "systemctl_available": True,
        "openrc_tools_available": False,
        "nvidia_backend_compiled": True,
        "ata_smart_probe": "hardware_not_detected",
        "nvidia_nvml_probe": "hardware_not_detected",
        "openrc_probe": "backend_unconfirmed",
        "systemd_probe": "backend_unconfirmed",
        "ebpf_object": None,
        "target_environment": target_environment,
    }


def self_test() -> None:
    fixture = valid_fixture()
    assert not validate(fixture, [])

    stale = {**fixture, "schema_version": SCHEMA_VERSION - 1}
    assert any("schema_version" in error for error in validate(stale, []))
    incomplete_standard = {**fixture, "nvidia_backend_compiled": False}
    assert any(
        "must include the current NVIDIA backend" in error
        for error in validate(incomplete_standard, [])
    )
    mismatched = {
        **fixture,
        "target_environment": {
            **fixture["target_environment"],
            "openrc": "eligible",
        },
    }
    assert any("must match openrc_probe" in error for error in validate(mismatched, []))
    untyped = {
        **fixture,
        "target_environment": {
            **fixture["target_environment"],
            "amd_device_markers": True,
        },
    }
    assert any("amd_device_markers" in error for error in validate(untyped, []))
    inconsistent_pmu = {
        **fixture,
        "target_environment": {
            **fixture["target_environment"],
            "intel_gpu_engine_pmu": "eligible",
            "intel_gpu_engine_pmu_devices": 0,
        },
    }
    assert any("eligible requires at least one device" in error for error in validate(inconsistent_pmu, []))
    privileged_pmu = {
        **fixture,
        "target_environment": {
            **fixture["target_environment"],
            "effective_perfmon_privilege": True,
        },
    }
    assert any("privilege_required requires" in error for error in validate(privileged_pmu, []))
    leaked = {**fixture, "provider": {"device_path": "/dev/example"}}
    assert any("forbidden keys" in error for error in validate(leaked, []))
    assert any(
        "expected 'eligible'" in error
        for error in validate(fixture, [("openrc", "eligible")])
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("receipt", type=Path, nargs="?")
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument(
        "--require",
        action="append",
        default=[],
        type=parse_requirement,
        metavar="TARGET=STATUS",
    )
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("Target environment receipt validator self-test: PASS")
        return 0
    if args.receipt is None:
        parser.error("receipt is required unless --self-test is used")

    try:
        receipt = json.loads(args.receipt.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        print(f"FAIL: could not read receipt: {error}")
        return 2
    if not isinstance(receipt, dict):
        print("FAIL: receipt root must be an object")
        return 2

    errors = validate(receipt, args.require)
    if errors:
        for error in errors:
            print(f"FAIL: {error}")
        return 1

    targets = receipt["target_environment"]
    summary = ", ".join(f"{target}={targets[target]}" for target in sorted(TARGETS))
    print(f"PASS: target capability receipt is valid: {summary}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
