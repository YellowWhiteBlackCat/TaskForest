#!/usr/bin/env bash
# Route gate for UI evidence.
#
# When a diff touches a UI boundary, the standard gate must run the headless
# frontend interaction matrices (--with-gui), and a capture-acceptance run must
# carry fresh pixel receipts (--require-capture). Pure core changes never
# force re-capture. A receipt is only fresh when its frontend-scoped source
# manifest hash still matches the current worktree and its metadata records
# the private background-Niri route; mtime alone is not evidence after a
# dirty-tree change.
#
# Usage:
#   bash scripts/quality/ui-evidence-route.sh [--base <ref>] [--with-gui]
#     [--require-capture]
# Default base is the merge-base with origin/main (fallback HEAD~1).

set -u

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo"

base=""
with_gui=0
require_capture=0
while [[ $# -gt 0 ]]; do
    case "$1" in
    --base)
        shift
        base="${1:-}"
        ;;
    --with-gui) with_gui=1 ;;
    --require-capture) require_capture=1 ;;
    *)
        echo "unknown argument '$1'" >&2
        exit 2
        ;;
    esac
    shift
done

if [[ -z "$base" ]]; then
    if git rev-parse --verify origin/main >/dev/null 2>&1; then
        base="$(git merge-base HEAD origin/main)"
    else
        base="HEAD~1"
    fi
fi

changed="$(mktemp "$repo/.tmp/ui-evidence-route.XXXXXX")"
trap 'rm -f "$changed"' EXIT
git diff --name-only "$base" >"$changed" || exit 1
git ls-files --others --exclude-standard >>"$changed" 2>/dev/null || true
sort -u "$changed" -o "$changed"

ui_touched=0
gpui_touched=0
tui_touched=0
iced_touched=0
bevy_touched=0
while IFS= read -r path; do
    case "$path" in
    crates/taskmanager-theme/*)
        ui_touched=1
        gpui_touched=1
        tui_touched=1
        iced_touched=1
        bevy_touched=1
        ;;
    crates/taskmanager-gpui/* | crates/taskmanager-ui/* | \
        crates/taskmanager-icons/* | crates/taskmanager-ui-contract/* | tests/gui/* | locales/* | \
        scripts/capture-niri.sh | scripts/capture-windows.sh | \
        scripts/accept-gpui-interactions.sh | scripts/windows/accept-gpui-interactions.sh | \
        scripts/gpui_interaction_matrix.tsv | \
        scripts/capture_scenarios.tsv)
        ui_touched=1
        gpui_touched=1
        ;;
    crates/taskmanager-tui/* | scripts/capture-tui.sh | scripts/re-sign-tui-provenance.sh)
        ui_touched=1
        tui_touched=1
        ;;
    crates/taskmanager-iced/* | scripts/capture-iced.sh | scripts/capture-iced-matrix.sh | \
        scripts/capture_iced_scenarios.tsv | scripts/validate_iced_matrix.py | \
        scripts/validate_iced_evidence.py)
        ui_touched=1
        iced_touched=1
        ;;
    crates/taskmanager-bevy-ui/* | scripts/capture-bevy.sh | scripts/capture_bevy_scenarios.tsv | \
        scripts/validate_bevy_matrix.py | scripts/accept-bevy-interactions.sh | \
        scripts/bevy_interaction_matrix.tsv)
        ui_touched=1
        bevy_touched=1
        ;;
    esac
done <"$changed"

if [[ "$ui_touched" == "0" ]]; then
    echo "PASS ui-evidence-route: no UI boundary changes (base=$base)"
    exit 0
fi

if [[ "$with_gui" == "0" ]]; then
    echo "FAIL ui-evidence-route: UI boundary changed (base=$base); standard requires" >&2
    echo "     --with-gui for the selected frontend interaction matrix" >&2
    exit 1
fi

if [[ "$require_capture" == "1" ]]; then
    base_time="$(git show -s --format=%ct "$base" 2>/dev/null || echo 0)"

    for command in find grep jq sha256sum timeout; do
        if ! command -v "$command" >/dev/null 2>&1; then
            echo "FAIL ui-evidence-route: --require-capture needs $command" >&2
            exit 1
        fi
    done
    if ! timeout 5s python3 --version >/dev/null 2>&1; then
        echo "FAIL ui-evidence-route: --require-capture needs Python 3" >&2
        exit 1
    fi

    newer_than_base() {
        local file="$1"
        [[ -f "$file" ]] || return 1
        local mtime
        mtime="$(stat -c %Y "$file" 2>/dev/null || echo 0)"
        [[ "$mtime" -ge "$base_time" ]]
    }

    current_manifest_sha() {
        local frontend="$1" manifest digest
        manifest="$(mktemp "$repo/.tmp/ui-evidence-route-manifest.XXXXXX")" || return 1
        if ! timeout --kill-after=10s 60s python3 scripts/frontend_source_manifest.py \
            --frontend "$frontend" --repo-root "$repo" --output "$manifest" \
            >/dev/null; then
            rm -f "$manifest"
            return 1
        fi
        digest="$(sha256sum "$manifest" | awk '{print $1}')"
        rm -f "$manifest"
        printf '%s\n' "$digest"
    }

    find_current_receipt() {
        local root="$1" name="$2" frontend="$3"
        local expected_hash run_dir metadata recorded_hash file
        expected_hash="$(current_manifest_sha "$frontend")" || return 1
        [[ -d "$root" ]] || return 1
        while IFS= read -r file; do
            newer_than_base "$file" || continue
            run_dir="$(dirname "$file")"
            metadata="$(find "$run_dir" -maxdepth 1 -type f \
                \( -name '*metadata.txt' -o -name 'metadata.txt' \) -print -quit)"
            [[ -n "$metadata" && -f "$metadata" ]] || continue
            grep -q '^niri_background=1$' "$metadata" || continue
            recorded_hash="$(jq -r '.source_manifest_sha256 // empty' "$file" 2>/dev/null || true)"
            if [[ -z "$recorded_hash" ]]; then
                recorded_hash="$(awk -F= '$1 == "source_manifest_sha256" { print $2; exit }' \
                    "$metadata")"
            fi
            if [[ -n "$recorded_hash" && "$recorded_hash" == "$expected_hash" ]]; then
                return 0
            fi
        done < <(find "$root" -type f -name "$name" 2>/dev/null)
        return 1
    }

    missing=""
    if [[ "$gpui_touched" == "1" ]]; then
        if ! find_current_receipt "target/screenshot-evidence" "capture-validation.json" gpui &&
            ! find_current_receipt "target/windows-evidence" "receipt-metadata.txt" gpui; then
            missing="$missing gpui-capture"
        fi
    fi
    if [[ "$tui_touched" == "1" ]] &&
        ! find_current_receipt "target/tui-evidence" "tui-capture-validation.json" tui; then
        missing="$missing tui-capture"
    fi
    if [[ "$iced_touched" == "1" ]] &&
        ! find_current_receipt "target/iced-evidence" "iced-capture-validation.json" iced; then
        missing="$missing iced-capture"
    fi
    if [[ "$bevy_touched" == "1" ]] &&
        ! find_current_receipt "target/bevy-evidence" "bevy-capture-validation.json" bevy; then
        missing="$missing bevy-capture"
    fi

    if [[ -n "$missing" ]]; then
        echo "FAIL ui-evidence-route: UI boundary changed but fresh capture evidence is" >&2
        echo "     missing:$missing (run the selected frontend capture workflow)" >&2
        exit 1
    fi
fi

echo "PASS ui-evidence-route: UI boundary covered by headless matrix and capture receipts"
exit 0
