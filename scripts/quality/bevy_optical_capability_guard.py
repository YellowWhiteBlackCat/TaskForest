#!/usr/bin/env python3
"""Fail-closed claim guard for Bevy optical glass.

The production renderer may name a capability ceiling only when the code and the
standard-gate receipt describe the same fact. This guard checks:

* capability is derived from a live ``RenderAdapterInfo`` rather than a palette
  constant;
* the GPU proof, gate wiring, and machine-readable receipt line exist;
* every receipt record honestly distinguishes hardware proof from Painted
  degradation and refuses to claim unproven nested layers.

The quick tier runs the static/profile check without demanding a fresh receipt
(the GPU proof itself is a standard-tier action). The standard tier runs with
``--require-fresh`` after ``bevy-optical-gpu`` has produced the receipt, so an
unmeasured or stale ceiling becomes blocking there.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import tempfile
from pathlib import Path

CAPABILITY = Path("crates/taskmanager-bevy-ui/src/glass/optical/capability.rs")
SCENE = Path("crates/taskmanager-bevy-ui/src/glass/optical/scene.rs")
PALETTE = Path("crates/taskmanager-bevy-ui/src/palette.rs")
PROOF = Path("crates/taskmanager-bevy-ui/tests/gui/optical_gpu.rs")
GATE = Path("scripts/quality/bevy-optical-gpu-gate.sh")
RECEIPT_SCRIPT = Path("scripts/quality/bevy_optical_gpu_receipt.py")
GUARD = Path(__file__)
GATES = Path("scripts/quality/local-gates.sh")
RECEIPT = Path("target/optical-gpu-evidence/receipt.json")
OPERATIONS = [
    "BackdropSampling",
    "Blur",
    "Refraction",
    "Dispersion",
    "Specular",
    "Grain",
]

# A nested-layer claim is only admissible when the GPU proof itself carries
# the named assertions that compare the chain target with the painted lower
# composite. Flipping the receipt string alone must never pass.
NESTED_PROOF_TOKENS = (
    "the chain must carry the first optical layer's real pixels",
    "chain_only_capture",
)

NESTED_CEILING_PATTERN = re.compile(
    r"OPTICAL_GLASS_MAX_NESTED_LAYERS\s*:\s*u8\s*=\s*(?P<ceiling>\d+)"
)


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def current_digest(repo: Path) -> str:
    import importlib.util

    spec = importlib.util.spec_from_file_location("receipt", repo / RECEIPT_SCRIPT)
    if spec is None or spec.loader is None:
        raise RuntimeError("cannot load receipt helper")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.source_digest(repo)


def require(text: str, token: str, path: Path, findings: list[str]) -> None:
    if token not in text:
        findings.append(f"{path}: missing required optical wiring token: {token}")


def nested_ceiling(capability: str) -> int | None:
    """The nesting ceiling the renderer source declares."""
    match = NESTED_CEILING_PATTERN.search(capability)
    if match is None:
        return None
    return int(match.group("ceiling"))


def check_implementation(repo: Path, findings: list[str]) -> None:
    palette = read(repo / PALETTE)
    if "reported_by_renderer" in palette:
        findings.append(f"{PALETTE}: optical capability must not be a palette constant")

    capability = read(repo / CAPABILITY)
    require(capability, "adapter.is_none_or(adapter_is_software)", CAPABILITY, findings)
    require(capability, "OpticalGlassCapability::UNSUPPORTED", CAPABILITY, findings)
    require(capability, "OPTICAL_GLASS_PROVEN_OPERATIONS", CAPABILITY, findings)

    scene = read(repo / SCENE)
    require(scene, "render_adapter: Option<Res<RenderAdapterInfo>>", SCENE, findings)
    require(scene, "optical_glass_capability(render_adapter.as_deref())", SCENE, findings)

    proof = read(repo / PROOF)
    require(proof, "optical-gpu receipt:", PROOF, findings)
    require(
        proof,
        "operations=BackdropSampling,Blur,Refraction,Dispersion,Specular,Grain",
        PROOF,
        findings,
    )
    if "nesting_layers_proven=true" in proof:
        # A nested claim must be backed by the proof's own chain assertions
        # and by a declared ceiling above one layer.
        for token in NESTED_PROOF_TOKENS:
            require(proof, token, PROOF, findings)
        ceiling = nested_ceiling(capability)
        if ceiling is None:
            findings.append(f"{CAPABILITY}: the nesting ceiling is not declared")
        elif ceiling < 2:
            findings.append(
                f"{CAPABILITY}: a nested-layer claim needs a ceiling of at least 2"
            )


def check_wiring(repo: Path, findings: list[str]) -> None:
    gates = read(repo / GATES)
    require(gates, "run_stage bevy-optical-capability-self quick", GATES, findings)
    require(gates, "run_stage bevy-optical-capability quick", GATES, findings)
    require(gates, "run_stage bevy-optical-gpu-self standard", GATES, findings)
    require(gates, "run_stage bevy-optical-gpu standard", GATES, findings)
    require(gates, "--require-fresh", GATES, findings)
    require(gates, "bash scripts/quality/bevy-optical-gpu-gate.sh", GATES, findings)

    gate = read(repo / GATE)
    require(gate, "cargo nextest list", GATE, findings)
    require(gate, "cargo nextest run", GATE, findings)
    require(
        gate,
        "production_shader_renders_optical_effects_and_tracks_the_backdrop",
        GATE,
        findings,
    )


def evaluate_receipt(
    receipt: dict[str, object],
    *,
    current: str | None,
    mode: str,
    declared_ceiling: int | None = None,
) -> list[str]:
    findings: list[str] = []
    if receipt.get("schema_version") != 1:
        findings.append("receipt: unsupported schema_version")
    if receipt.get("kind") != "bevy-optical-gpu-proof":
        findings.append("receipt: wrong kind")
    if current is not None and receipt.get("source_digest") != current:
        findings.append("receipt: source digest is stale; re-run the optical GPU gate")

    verdict = str(receipt.get("verdict", "MISSING"))
    capability = str(receipt.get("runtime_capability", ""))
    operations = receipt.get("operations_proven", [])
    if mode == "1" and verdict != "PROVEN":
        findings.append(f"receipt: TM_OPTICAL_GPU_GATE=1 requires PROVEN, got {verdict}")
    if verdict == "PROVEN":
        if operations != OPERATIONS:
            findings.append(
                "receipt: PROVEN operations differ from the production operation set"
            )
        if not capability.startswith("OPTICAL"):
            findings.append("receipt: PROVEN must report an OPTICAL runtime capability")
    elif verdict in {"NO_ADAPTER", "SOFTWARE_ADAPTER", "SKIPPED", "REGRESSION"}:
        if not capability.startswith("PAINTED"):
            findings.append("receipt: a degraded optical proof must explicitly record Painted")
        if operations and verdict != "SOFTWARE_ADAPTER":
            findings.append(
                "receipt: a degraded non-software proof cannot claim proven operations"
            )
    else:
        findings.append(f"receipt: unknown verdict {verdict}")
    nesting_proven = bool(receipt.get("nesting_layers_proven"))
    receipt_nesting = receipt.get("max_nested_layers")
    if nesting_proven:
        if not isinstance(receipt_nesting, int) or receipt_nesting < 2:
            findings.append(
                "receipt: a proven nested-layer claim must record at least two layers"
            )
        elif declared_ceiling is not None and receipt_nesting != declared_ceiling:
            findings.append(
                "receipt: the recorded nesting ceiling differs from the declared one"
            )
    elif isinstance(receipt_nesting, int) and receipt_nesting > 1:
        findings.append(
            "receipt: a nesting ceiling above one layer requires a proven nested layer"
        )
    return findings


def check_receipt(
    repo: Path, findings: list[str], *, require_fresh: bool
) -> dict[str, object] | None:
    path = repo / RECEIPT
    if not path.is_file():
        if require_fresh:
            findings.append(
                f"{RECEIPT}: missing; the standard bevy-optical-gpu stage must produce it"
            )
        return None
    try:
        receipt = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as error:
        findings.append(f"{RECEIPT}: unreadable: {error}")
        return None
    current = current_digest(repo) if require_fresh else None
    findings.extend(
        evaluate_receipt(
            receipt,
            current=current,
            mode=os.environ.get("TM_OPTICAL_GPU_GATE", "auto"),
            declared_ceiling=nested_ceiling(read(repo / CAPABILITY)),
        )
    )
    return receipt


def self_test() -> int:
    with tempfile.TemporaryDirectory(prefix="bevy-optical-claim-self-") as directory:
        root = Path(directory)
        receipts = root / "receipts"
        receipts.mkdir()

        cases = [
            ("PROVEN", OPERATIONS, "OPTICAL (proven)", "auto", False),
            ("NO_ADAPTER", [], "PAINTED (unproven)", "auto", False),
            ("SOFTWARE_ADAPTER", OPERATIONS, "PAINTED (software adapter)", "auto", False),
            ("SKIPPED", [], "PAINTED (unproven)", "auto", False),
            ("NO_ADAPTER", [], "PAINTED (unproven)", "1", True),
            ("SOFTWARE_ADAPTER", OPERATIONS, "PAINTED (software adapter)", "1", True),
            ("PROVEN", ["Blur"], "OPTICAL (proven)", "auto", True),
            ("NO_ADAPTER", ["Blur"], "PAINTED (unproven)", "auto", True),
        ]
        for verdict, operations, capability, mode, should_fail in cases:
            path = receipts / f"{verdict}-{mode}.json"
            payload = {
                "schema_version": 1,
                "kind": "bevy-optical-gpu-proof",
                "source_digest": "current",
                "verdict": verdict,
                "operations_proven": operations,
                "runtime_capability": capability,
                "nesting_layers_proven": False,
            }
            path.write_text(json.dumps(payload), encoding="utf-8")
            loaded = json.loads(path.read_text(encoding="utf-8"))
            findings = evaluate_receipt(loaded, current="current", mode=mode)
            if bool(findings) != should_fail:
                print(f"self-test FAIL: {verdict}/{mode} findings={findings}")
                return 1

        # A nested-layer claim is admissible only with a matching ceiling.
        nested_ok = {
            "schema_version": 1,
            "kind": "bevy-optical-gpu-proof",
            "source_digest": "current",
            "verdict": "PROVEN",
            "operations_proven": OPERATIONS,
            "runtime_capability": "OPTICAL (proven)",
            "nesting_layers_proven": True,
            "max_nested_layers": 2,
        }
        findings = evaluate_receipt(
            nested_ok, current="current", mode="auto", declared_ceiling=2
        )
        if findings:
            print(f"self-test FAIL: a proven nested claim was rejected: {findings}")
            return 1
        findings = evaluate_receipt(
            nested_ok, current="current", mode="auto", declared_ceiling=1
        )
        if not findings:
            print("self-test FAIL: a nested claim with a mismatched ceiling was accepted")
            return 1
        nested_unproven = dict(nested_ok)
        nested_unproven["nesting_layers_proven"] = False
        nested_unproven["max_nested_layers"] = 2
        findings = evaluate_receipt(
            nested_unproven, current="current", mode="auto", declared_ceiling=2
        )
        if not findings:
            print("self-test FAIL: an unproven ceiling above one layer was accepted")
            return 1

        stale = json.loads((receipts / "PROVEN-auto.json").read_text(encoding="utf-8"))
        findings = evaluate_receipt(stale, current="new", mode="auto")
        if "receipt: source digest is stale; re-run the optical GPU gate" not in findings:
            print(f"self-test FAIL: stale receipt was accepted: {findings}")
            return 1

        print("bevy-optical capability guard self-test: PASS")
        return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument(
        "--require-fresh",
        action="store_true",
        help="demand a present receipt whose source digest matches this tree",
    )
    args = parser.parse_args()
    if args.self_test:
        return self_test()

    repo = args.repo_root.resolve()
    findings: list[str] = []
    for relative in (CAPABILITY, SCENE, PALETTE, PROOF, GATE, RECEIPT_SCRIPT, GUARD, GATES):
        if not (repo / relative).is_file():
            findings.append(f"missing required file: {relative}")
    if findings:
        print("bevy-optical capability guard: FAIL")
        for finding in findings:
            print(f"- {finding}")
        return 1

    try:
        check_implementation(repo, findings)
        check_wiring(repo, findings)
        receipt = check_receipt(repo, findings, require_fresh=args.require_fresh)
    except (OSError, RuntimeError, json.JSONDecodeError) as error:
        print(f"bevy-optical capability guard: FAIL: {error}")
        return 2

    if findings:
        print("bevy-optical capability guard: FAIL")
        for finding in findings:
            print(f"- {finding}")
        return 1

    mode = os.environ.get("TM_OPTICAL_GPU_GATE", "auto")
    if receipt is None:
        verdict = "ABSENT"
        capability = "PAINTED (standard proof not yet run on this host)"
    else:
        verdict = str(receipt.get("verdict"))
        capability = str(receipt.get("runtime_capability"))
    profile = "fresh" if args.require_fresh else "static"
    print(
        "bevy-optical capability guard: PASS "
        f"(profile={profile}, receipt={verdict}, runtime={capability}, mode={mode})"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
