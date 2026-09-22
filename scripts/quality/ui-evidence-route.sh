#!/usr/bin/env bash
# Route gate for UI evidence.
#
# When a diff touches a UI boundary, the standard gate must run the headless
# frontend interaction matrices (--with-gui), and a capture-acceptance run must
# carry fresh pixel receipts (--require-capture) for every frontend whose
# pixels the diff can actually move. Pure core changes never force re-capture.
# A receipt is only fresh when its frontend-scoped source manifest hash still
# matches the current worktree and its metadata records the private
# background-Niri route; mtime alone is not evidence after a dirty-tree
# change.
#
# Impact routing (W22-B): the two requirements are separate judgments.
#
#   * `ui_touched` keeps the existing headless requirement: any UI-boundary
#     path still needs `--with-gui`. Contract, registry and gate layers
#     (`crates/taskmanager-ui-contract/tests/**`, the declaration-only
#     modules) set it too; their evidence channel is the headless suite, not
#     pixels.
#   * `gpui_touched`/`tui_touched`/`iced_touched`/`bevy_touched` mean "this
#     diff can change what that frontend paints", so `--require-capture`
#     demands a fresh receipt for it. Paths that cannot move pixels (the
#     contract test tree, declaration registries, vocabulary and conformance
#     modules) set none. A path consumed by several renderers names each
#     consumer; GPUI is never the default guess for a shared contract.
#   * The closing `crates/taskmanager-ui-contract/*` arm is the conservative
#     default for an unlisted path (new module, `lib.rs`, `Cargo.toml`), so a
#     new file cannot silently lose its capture demand.
#
# The frontend sets below follow real render consumers (module imports under
# `crates/*/src`), not a broad directory prefix:
#
#   ui-contract `tests/**`, `README.md`, `capabilities.rs`, `conformance.rs`,
#     `functional.rs`, `keybindings.rs`, `message.rs`, `feature_coverage*`,
#     `accessibility*`            -> headless only (no paint path)
#   ui-contract `src/focus.rs`    -> gpui (`taskmanager-ui` focus policy)
#   ui-contract `src/columns.rs`  -> gpui + iced + bevy (shared table specs;
#                                    the TUI builds its own column model)
#   ui-contract `src/navigation.rs` -> gpui + iced + tui (shared page help;
#                                    Bevy maps its own page labels)
#   ui-contract `src/icon.rs`, `src/command.rs` -> all four (painted icon and
#     command vocabulary resolved by the shared shell presentation)
#   `locales/*`                   -> all four (catalog strings are embedded
#                                    by the shared application layer)
#   `taskmanager-icons/*`         -> gpui + iced + bevy (semantic SVG assets;
#                                    the TUI maps `IconId` to terminal glyphs
#                                    itself and does not link this crate)
#
# Frontend test trees and root acceptance tests (`tests/gui/*`,
# `crates/taskmanager-*/tests/**`) keep their existing per-frontend routing;
# narrowing that dev-only layer is a separate owner decision, not an implicit
# part of this refinement.
#
# S5 interaction route (W23-B): the headless requirement follows the unified
# interaction declaration.  `scripts/parity/cross_frontend_matrix.tsv` carries
# every frontend in one file, so the route reads its changed rows and maps each
# to its own frontend (per-frontend-matrix parity: never a coarser "all four"
# guess and never narrower than the changed rows).  The S5 driver, resolver,
# schema and declaration files are dev-only evidence machinery whose channel is
# the headless evidence chain, so they demand the headless route and never a
# pixel receipt - the same class as the ui-contract declaration/test layers.
# The legacy per-frontend matrices and accept scripts stay routed while they
# remain compatibility assets (D6/S5 retirement window).
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

# S5 unified interaction declaration: one file carries every frontend, so a
# change maps to the frontends whose rows actually moved.  Added/removed data
# rows name their own frontend; an unrecognised frontend token falls back to
# all four (fail-closed - the resolver would reject it, the route must not
# under-demand first); a comment-only change moves no row and demands no
# frontend receipt; a wholesale replacement (an untracked file, a failed row
# diff) falls back to the frontends the current file declares.  The mapping is
# never narrower than the changed rows.
unified_matrix="scripts/parity/cross_frontend_matrix.tsv"
unified_matrix_frontends() {
    local diff_text frontends
    diff_text="$(git diff -U0 --no-color "$base" -- "$unified_matrix" 2>/dev/null || true)"
    if [[ -n "$diff_text" ]]; then
        frontends="$(printf '%s\n' "$diff_text" | awk -F'\t' '
            /^[+-]/ && $0 !~ /^(\+\+\+|---)/ {
                row = $0
                sub(/^[+-]/, "", row)
                if (row == "" || row ~ /^#/) next
                split(row, columns, "\t")
                if (columns[3] == "" || columns[3] == "frontend") next
                if (columns[3] ~ /^(gpui|iced|tui|bevy)$/) print columns[3]
                else print "all"
            }' | sort -u)"
    else
        frontends=""
        if [[ -f "$unified_matrix" ]]; then
            frontends="$(awk -F'\t' '
                !/^#/ && $3 != "" && $3 != "frontend" {
                    print ($3 ~ /^(gpui|iced|tui|bevy)$/ ? $3 : "all")
                }' "$unified_matrix" | sort -u)"
        fi
    fi
    printf '%s\n' "$frontends"
}

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
    crates/taskmanager-icons/*)
        # Semantic icon registry (ADR-017): every asset-rendering shape
        # materializes these bytes.  The TUI maps `IconId` to terminal glyphs
        # itself and does not link this crate.
        ui_touched=1
        gpui_touched=1
        iced_touched=1
        bevy_touched=1
        ;;
    locales/*)
        # `locales/{en,zh}.json` are include_str!-embedded by the shared
        # application layer every product links; a string change repaints all
        # four frontends.
        ui_touched=1
        gpui_touched=1
        tui_touched=1
        iced_touched=1
        bevy_touched=1
        ;;
    crates/taskmanager-ui-contract/tests/* | \
        crates/taskmanager-ui-contract/README.md)
        # Contract gate tests and crate docs: dev-only, not linked into any
        # product binary; their evidence is the headless contract suite and
        # never a pixel frame.  The headless route requirement still applies.
        ui_touched=1
        ;;
    crates/taskmanager-ui-contract/src/capabilities.rs | \
        crates/taskmanager-ui-contract/src/conformance.rs | \
        crates/taskmanager-ui-contract/src/functional.rs | \
        crates/taskmanager-ui-contract/src/keybindings.rs | \
        crates/taskmanager-ui-contract/src/message.rs | \
        crates/taskmanager-ui-contract/src/feature_coverage.rs | \
        crates/taskmanager-ui-contract/src/feature_coverage/* | \
        crates/taskmanager-ui-contract/src/accessibility.rs | \
        crates/taskmanager-ui-contract/src/accessibility/*)
        # Declaration registries and vocabulary (capability/feature/intent/
        # keybinding coverage, message keys, contract tags) plus the semantic
        # accessibility model.  Consumed by declaration adapters and headless
        # gates; no frontend paint path reads them, and a pixel frame cannot
        # prove their claims.
        ui_touched=1
        ;;
    crates/taskmanager-ui-contract/src/focus.rs)
        # Modal focus/restore policy consumed by the GPUI reference component
        # layer (`taskmanager-ui`); Iced owns a local FocusTarget.
        ui_touched=1
        gpui_touched=1
        ;;
    crates/taskmanager-ui-contract/src/columns.rs)
        # Shared process-table column specs, consumed by the GPUI, Iced and
        # Bevy tables; the TUI builds its own terminal column model.
        ui_touched=1
        gpui_touched=1
        iced_touched=1
        bevy_touched=1
        ;;
    crates/taskmanager-ui-contract/src/icon.rs | \
        crates/taskmanager-ui-contract/src/command.rs)
        # Painted shared vocabulary: icon identities and per-command
        # icon/label descriptors resolved by the shared shell presentation
        # every frontend paints.
        ui_touched=1
        gpui_touched=1
        tui_touched=1
        iced_touched=1
        bevy_touched=1
        ;;
    crates/taskmanager-ui-contract/src/navigation.rs)
        # Shared page descriptors painted by the GPUI/Iced help overlays and
        # the TUI header; Bevy resolves its own page labels.
        ui_touched=1
        gpui_touched=1
        tui_touched=1
        iced_touched=1
        ;;
    crates/taskmanager-ui-contract/*)
        # Conservative default: an unlisted ui-contract path (new module,
        # `lib.rs` surface, Cargo manifest) may reach any consumer.  Demand
        # all four receipts rather than guessing a narrower set.
        ui_touched=1
        gpui_touched=1
        tui_touched=1
        iced_touched=1
        bevy_touched=1
        ;;
    crates/taskmanager-gpui/* | crates/taskmanager-ui/* | tests/gui/* | \
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
    "$unified_matrix")
        # The unified S5 interaction declaration: a row change names the
        # frontend whose interaction contract moved, so the route keeps the
        # per-frontend matrix routes (W22-B) by reading the changed rows.  The
        # file itself cannot paint; its channel is the headless interaction
        # acceptance, so it never adds a pixel receipt of its own.
        ui_touched=1
        while IFS= read -r matrix_frontend; do
            case "$matrix_frontend" in
            gpui) gpui_touched=1 ;;
            tui) tui_touched=1 ;;
            iced) iced_touched=1 ;;
            bevy) bevy_touched=1 ;;
            all)
                # Unrecognised frontend token: fail closed to every receipt.
                gpui_touched=1
                tui_touched=1
                iced_touched=1
                bevy_touched=1
                ;;
            esac
        done < <(unified_matrix_frontends)
        ;;
    scripts/parity/accept-frontend-interactions.sh | \
        scripts/parity/accept_frontend_interactions.py | \
        scripts/parity/resolve_frontend_evidence.py | \
        scripts/parity/test_resolve_frontend_evidence.py | \
        scripts/parity/cross_frontend_manifest.tsv | \
        scripts/parity/feature_evidence.tsv | \
        scripts/parity/feature_evidence_co_anchors.tsv | \
        scripts/parity/run_manifest.schema.json | \
        scripts/parity/README.md | \
        scripts/quality/cross_frontend_manifest.py)
        # S5 evidence chain: committed declaration data and gate machinery for
        # the cross-frontend evidence closure.  Their channel is the headless
        # evidence chain (the resolver stage and the unified driver), not a
        # pixel frame, so they demand the headless route and never a capture
        # receipt -- the same class as the ui-contract declaration/test layers.
        ui_touched=1
        ;;
    esac
done <"$changed"

capture_frontends=""
if [[ "$gpui_touched" == "1" ]]; then
    capture_frontends="gpui"
fi
if [[ "$tui_touched" == "1" ]]; then
    capture_frontends="${capture_frontends:+$capture_frontends }tui"
fi
if [[ "$iced_touched" == "1" ]]; then
    capture_frontends="${capture_frontends:+$capture_frontends }iced"
fi
if [[ "$bevy_touched" == "1" ]]; then
    capture_frontends="${capture_frontends:+$capture_frontends }bevy"
fi

if [[ "$ui_touched" == "0" ]]; then
    echo "PASS ui-evidence-route: no UI boundary changes (base=$base)"
    exit 0
fi

if [[ "$with_gui" == "0" ]]; then
    echo "FAIL ui-evidence-route: UI boundary changed (base=$base); standard requires" >&2
    echo "     --with-gui for the headless interaction acceptance" >&2
    echo "     (the interaction stages run scripts/parity/accept-frontend-interactions.sh)" >&2
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

if [[ "$require_capture" == "1" && -z "$capture_frontends" ]]; then
    echo "PASS ui-evidence-route: UI boundary is contract/registry-only (base=$base);"
    echo "     headless matrix covers it and no pixel receipt is owed"
    exit 0
fi

echo "PASS ui-evidence-route: UI boundary covered by headless matrix and capture receipts"
exit 0
