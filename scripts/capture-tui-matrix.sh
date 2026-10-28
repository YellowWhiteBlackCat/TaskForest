#!/usr/bin/env bash
# Drive the committed TUI scenario matrix, one supervised capture per row.
#
# The matrix row name is a receipt label; the scene token is derived from
# capture-tui.sh's CAPTURE_SCENE case so page/device/appearance rows stay
# scene-less. Build the product binary once, then let each row start its own
# supervisor and write its own validated run under target/tui-evidence/runs/.
set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
MATRIX="$REPO/scripts/capture_tui_scenarios.tsv"
CAPTURE="$REPO/scripts/capture-tui.sh"
BINARY="$REPO/target/debug/taskforest-t"
BUILD=1
DRY_RUN=0
LIST=0
SELECTED=()

usage() {
  cat >&2 <<'EOF'
usage: bash scripts/capture-tui-matrix.sh [options]

  --scenarios a,b       capture only these matrix rows (row name or base scene)
  --no-build            reuse target/debug/taskforest-t
  --dry-run             print the resolved env for each selected row
  --list                list matrix rows with their derived scene token
  -h, --help            show this help

Without --scenarios the complete matrix runs; each row is a separate
supervised capture and may take minutes.
EOF
}

while [ $# -gt 0 ]; do
  case "$1" in
    --scenarios)
      [ $# -ge 2 ] || { printf '%s\n' '--scenarios needs a comma-separated list' >&2; exit 2; }
      IFS=',' read -r -a SELECTED <<< "$2"
      shift 2
      ;;
    --no-build)
      BUILD=0
      shift
      ;;
    --dry-run)
      DRY_RUN=1
      shift
      ;;
    --list)
      LIST=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      printf 'unknown argument: %s\n' "$1" >&2
      usage
      exit 2
      ;;
  esac
done

[ -f "$MATRIX" ] || { printf 'matrix not found: %s\n' "$MATRIX" >&2; exit 2; }
[ -f "$CAPTURE" ] || { printf 'capture runner not found: %s\n' "$CAPTURE" >&2; exit 2; }

HEADER="$(head -n 1 "$MATRIX")"
EXPECTED_HEADER=$'name\tpage\tdevice\tcolumns\tlines\tskin'
if [ "$HEADER" != "$EXPECTED_HEADER" ]; then
  printf 'unexpected TUI matrix header: %s\n' "$HEADER" >&2
  exit 2
fi

# The CAPTURE_SCENE case in capture-tui.sh is the runner-owned scene
# vocabulary. Parse it instead of keeping a second list in this file.
SUPPORTED_SCENES="$(
  awk '
    /^case "\$CAPTURE_SCENE" in/ { inside=1; next }
    inside && /^[[:space:]]*\*\)/ { exit }
    inside && /^[[:space:]]*[a-z0-9|-]+\)/ {
      line=$0
      sub(/\).*/, "", line)
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", line)
      n=split(line, tokens, "|")
      for (i=1; i<=n; i++) if (tokens[i] != "") print tokens[i]
    }
  ' "$CAPTURE"
)"
[ -n "$SUPPORTED_SCENES" ] || {
  printf 'cannot parse supported scenes from %s\n' "$CAPTURE" >&2
  exit 2
}

is_scene() {
  grep -Fxq "$1" <<< "$SUPPORTED_SCENES"
}

selected() {
  [ "${#SELECTED[@]}" -eq 0 ] && return 0
  local want
  for want in "${SELECTED[@]}"; do
    [ "$want" = "$1" ] && return 0
    [ "$want" = "$2" ] && return 0
  done
  return 1
}

if [ "$LIST" -eq 0 ] && [ "$DRY_RUN" -eq 0 ] && [ "$BUILD" -eq 1 ]; then
  printf 'building %s\n' "$BINARY"
  (cd "$REPO" && cargo build --locked -p taskmanager-tui --bin taskforest-t -j 4)
fi
if [ "$LIST" -eq 0 ] && [ "$DRY_RUN" -eq 0 ]; then
  [ -x "$BINARY" ] || { printf 'missing executable: %s\n' "$BINARY" >&2; exit 2; }
fi

RUNS=0
FAILED=0
MATCHED=0
while IFS=$'\t' read -r name page device columns lines skin; do
  [ "$name" = "name" ] && continue
  [ -z "$name" ] && continue
  case "$name" in \#*) continue ;; esac

  base="${name%-compact}"
  scene=""
  if is_scene "$base"; then
    scene="$base"
  fi
  if ! selected "$name" "$base"; then
    continue
  fi
  MATCHED=$((MATCHED + 1))

  if [ "$LIST" -eq 1 ]; then
    printf '%s\tpage=%s\tdevice=%s\tcolumns=%s\tlines=%s\tskin=%s\tscene=%s\n' \
      "$name" "$page" "$device" "$columns" "$lines" "$skin" "${scene:-<none>}"
    continue
  fi

  if [ "$DRY_RUN" -eq 1 ]; then
    printf 'DRY RUN %s: page=%s device=%s columns=%s lines=%s skin=%s scene=%s\n' \
      "$name" "$page" "$device" "$columns" "$lines" "$skin" "${scene:-<none>}"
    continue
  fi

  printf 'TUI matrix capture: %s (scene=%s, page=%s, device=%s, %sx%s, %s)\n' \
    "$name" "${scene:-<none>}" "$page" "$device" "$columns" "$lines" "$skin"

  if (
    unset TM_TUI_CAPTURE_SCENE TM_TUI_CAPTURE_DEVICE TM_TUI_CAPTURE_SOURCE_FAILURE
    unset TM_TUI_CAPTURE_TERMINAL_SCENARIO
    export TM_TUI_CAPTURE_PAGE="$page"
    export TM_TUI_CAPTURE_COLUMNS="$columns"
    export TM_TUI_CAPTURE_LINES="$lines"
    export TM_TUI_CAPTURE_SKIN="$skin"
    export TM_TUI_CAPTURE_PREBUILT_BINARY="$BINARY"
    export TM_TUI_CAPTURE_MATRIX_ROW="$name"
    if [ "$device" != "default" ] && [ -n "$device" ]; then
      export TM_TUI_CAPTURE_DEVICE="$device"
    fi
    if [ -n "$scene" ]; then
      export TM_TUI_CAPTURE_SCENE="$scene"
    fi
    bash "$CAPTURE"
  ); then
    RUNS=$((RUNS + 1))
    metadata="$REPO/target/tui-evidence/latest/tui-capture-metadata.txt"
    if [ ! -f "$metadata" ]; then
      printf 'matrix capture %s: missing latest receipt\n' "$name" >&2
      FAILED=$((FAILED + 1))
      continue
    fi
    if ! grep -Fxq "matrix_row=$name" "$metadata"; then
      printf 'matrix capture %s: latest receipt does not name the row\n' "$name" >&2
      FAILED=$((FAILED + 1))
    fi
    if [ -n "$scene" ] && ! grep -Fxq "scene=$scene" "$metadata"; then
      printf 'matrix capture %s: latest receipt does not name scene %s\n' "$name" "$scene" >&2
      FAILED=$((FAILED + 1))
    fi
  else
    printf 'matrix capture %s: FAILED\n' "$name" >&2
    FAILED=$((FAILED + 1))
  fi
done < "$MATRIX"

if [ "${#SELECTED[@]}" -gt 0 ] && [ "$MATCHED" -eq 0 ]; then
  printf 'no matrix rows matched --scenarios: %s\n' "${SELECTED[*]}" >&2
  exit 2
fi

if [ "$LIST" -eq 1 ] || [ "$DRY_RUN" -eq 1 ]; then
  exit 0
fi

printf 'TUI matrix capture: %s run(s), %s failure(s)\n' "$RUNS" "$FAILED"
[ "$FAILED" -eq 0 ]
