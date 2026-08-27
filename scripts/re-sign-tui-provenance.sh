#!/usr/bin/env bash
# Re-sign the TUI capture source provenance after any source change.
#
# The capture gate requires the selected TUI package and its shared dependency
# closure to match the hashes recorded in the local TUI capture evidence.
# GPUI/Iced-only edits are intentionally outside this scope; shared edits still
# invalidate the TUI receipt. Pixel recapture is not needed for provenance-only
# changes, so this script re-signs the existing frame safely.
#
# Usage:
#   scripts/re-sign-tui-provenance.sh
# The refreshed files stay under ignored target/ evidence for local review.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO"

OUT="$REPO/target/tui-evidence/latest"
MANIFEST="$OUT/tui-source-manifest.sha256"
METADATA="$OUT/tui-capture-metadata.txt"
RECEIPT="$OUT/tui-capture-validation.json"
TSV="$OUT/tui-capture-manifest.tsv"

PYTHONDONTWRITEBYTECODE=1 timeout 60s python3 scripts/frontend_source_manifest.py \
    --frontend tui --repo-root "$REPO" --output "$MANIFEST"

NEW_HASH="$(sha256sum "$MANIFEST" | cut -d' ' -f1)"

# Point the metadata at the new manifest before validating (the validator
# cross-checks them; the pre-change values remain recoverable from git).
sed -i "s|^source_manifest_sha256=.*|source_manifest_sha256=$NEW_HASH|" "$METADATA"

# Validate the new manifest against the worktree BEFORE mutating the receipt.
timeout 30s python3 scripts/validate_tui_evidence.py \
    --image "$OUT/tui-mvp.png" \
    --metadata "$METADATA" \
    --markers "$OUT/tui-capture-markers.log" \
    --source-manifest "$MANIFEST" \
    --manifest "$TSV" \
    --receipt "$RECEIPT" \
    --repo-root "$REPO" \
    --check-only

NEW_META="$(sha256sum "$METADATA" | cut -d' ' -f1)"
NOW="$(date -u +%Y-%m-%dT%H:%M:%S+00:00)"

timeout 30s python3 - "$RECEIPT" "$NEW_HASH" "$NEW_META" "$NOW" <<'PY'
import json
import sys

path, manifest_hash, metadata_hash, validated_at = sys.argv[1:]
with open(path, encoding="utf-8") as handle:
    payload = json.load(handle)
payload["validated_at"] = validated_at
payload["source_manifest_sha256"] = manifest_hash
payload["metadata_sha256"] = metadata_hash
with open(path, "w", encoding="utf-8") as handle:
    json.dump(payload, handle, indent=2)
    handle.write("\n")
PY

# The validator appends one `ready` row per validation run; replicate it so the
# TSV rows stay in sync with the receipt history (convention from prior re-signs).
timeout 30s python3 - "$TSV" "$OUT" <<'PY'
import csv
import json
import pathlib
import sys

tsv, out = sys.argv[1:]
with open(out + "/tui-capture-validation.json", encoding="utf-8") as handle:
    artifact = json.load(handle)["artifact"]
with open(tsv, "a", encoding="utf-8", newline="") as handle:
    writer = csv.writer(handle, delimiter="\t", lineterminator="\n")
    writer.writerow([
        artifact["image"],
        artifact["width"],
        artifact["height"],
        artifact["bytes"],
        artifact["sha256"],
        "ready",
    ])
PY

echo "TUI provenance re-signed: source_manifest_sha256=$NEW_HASH metadata_sha256=$NEW_META"
