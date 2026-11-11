#!/usr/bin/env bash
# Run the Bevy production optical-GPU proof and write a fail-closed receipt.
#
# The proof test remains ignored by the ordinary nextest suite because it needs
# a live renderer. This gate removes the "ignored therefore unmeasured" gap:
# discovery checks that the named production-shader proof exists, then the gate
# runs it and records one of:
#
#   PROVEN           hardware adapter + all required operations passed
#   SOFTWARE_ADAPTER proof passed, but the product must stay Painted
#   NO_ADAPTER       wgpu cannot select an adapter; Painted fallback is recorded
#   SKIPPED          TM_OPTICAL_GPU_GATE=0 explicitly disabled the proof
#   REGRESSION       discovery, proof, parsing, or shader behavior failed
#
# TM_OPTICAL_GPU_GATE=auto (default) records degraded outcomes without
# pretending they are optical proof. TM_OPTICAL_GPU_GATE=1 fails closed unless a
# hardware adapter proves the production WESL.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo"

mode="${TM_OPTICAL_GPU_GATE:-auto}"
case "$mode" in
    auto | 0 | 1) ;;
    *)
        echo "TM_OPTICAL_GPU_GATE must be auto, 0, or 1" >&2
        exit 2
        ;;
esac

proof_name="production_shader_renders_optical_effects_and_tracks_the_backdrop"
package="taskmanager-bevy-ui"
evidence_dir="target/optical-gpu-evidence"
receipt="$evidence_dir/receipt.json"
run_dir="$evidence_dir/runs/$(date -u +%Y%m%dT%H%M%SZ)-$$"
discovery_log="$run_dir/discovery.log"
run_log="$run_dir/nextest.log"
empty_log="$run_dir/empty.log"
deadline="${TM_OPTICAL_GPU_TIMEOUT:-1800}"

if [[ "${1:-}" == "--self-test" ]]; then
    mkdir -p "$run_dir"
    : >"$empty_log"
    timeout --kill-after=10s 120s python3 scripts/quality/bevy_optical_gpu_receipt.py self-test
    receipt_line='optical-gpu receipt: adapter=AdapterInfo { name: "self-test", device_type: DiscreteGpu, backend: Vulkan } operations=BackdropSampling,Blur,Refraction,Dispersion,Specular,Grain max_nested_layers=1 max_blur_px=64 nesting_layers_proven=false fresnel_rim_proven=false'
    printf '%s\n' "$receipt_line" >"$run_dir/self-test-run.log"
    timeout --kill-after=10s 120s python3 scripts/quality/bevy_optical_gpu_receipt.py write \
        --mode auto --discovery-log "$empty_log" \
        --run-log "$run_dir/self-test-run.log" --exit-code 0 --discovered \
        --receipt "$run_dir/self-test-receipt.json"
    echo "bevy-optical-gpu gate self-test: PASS"
    exit 0
fi

mkdir -p "$run_dir"
: >"$empty_log"

if [[ "$mode" == "0" ]]; then
    timeout --kill-after=10s 120s python3 scripts/quality/bevy_optical_gpu_receipt.py write \
        --mode "$mode" --discovery-log "$empty_log" --run-log "$empty_log" \
        --exit-code 0 --discovered --receipt "$receipt"
    exit $?
fi

lock_args=()
if [[ -z "${TM_CARGO_LOCK:-}" ]]; then
    lock_args=(--locked)
fi

set +e
timeout --kill-after=30s "$deadline" cargo nextest list "${lock_args[@]}" \
    -p "$package" --lib --run-ignored=only --message-format json \
    -E "test($proof_name)" >"$discovery_log" 2>&1
discovery_rc=$?
set -e

discovered=false
discovery_args=()
if [[ "$discovery_rc" == "0" ]] && grep -Fq "$proof_name" "$discovery_log"; then
    discovered=true
    discovery_args+=(--discovered)
fi

if [[ "$discovered" == "true" ]]; then
    set +e
    timeout --kill-after=30s "$deadline" cargo nextest run "${lock_args[@]}" \
        -p "$package" --lib -j 4 --run-ignored=only --no-capture \
        -E "test($proof_name)" >"$run_log" 2>&1
    run_rc=$?
    set -e
else
    run_rc=127
    {
        echo "nextest discovery failed or did not find $proof_name"
        tail -n 40 "$discovery_log"
    } >"$run_log"
fi

timeout --kill-after=10s 120s python3 scripts/quality/bevy_optical_gpu_receipt.py write \
    --mode "$mode" --discovery-log "$discovery_log" --run-log "$run_log" \
    --exit-code "$run_rc" "${discovery_args[@]}" --receipt "$receipt"
