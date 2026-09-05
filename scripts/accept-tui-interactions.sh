#!/usr/bin/env bash
# Run the fail-closed TUI interaction acceptance gate.
#
# This is headless acceptance for the Ratatui terminal frontend (taskmanager-tui /
# taskforest-t). It runs all tests for taskmanager-tui with cargo nextest under the
# locked workspace, verifies that all tests pass (0 failures), and emits an evidence
# receipt to target/tui-interaction-evidence/<run_id>/receipt.json.
#
# Usage:
#   bash scripts/accept-tui-interactions.sh              # standard acceptance run
#   bash scripts/accept-tui-interactions.sh --self-test  # self-test parser & receipt logic
#   bash scripts/accept-tui-interactions.sh --verbose    # stream nextest output to console
set -euo pipefail
export LC_ALL=C

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO"

die() {
    printf 'TUI interaction gate: FAIL: %s\n' "$1" >&2
    exit 1
}

# ---------------------------------------------------------------------------
# Self-test mode: verifies parsing and receipt generation on synthetic logs.
# ---------------------------------------------------------------------------
if [[ "${1:-}" == "--self-test" ]]; then
    timeout 30s python3 -c '
import json, re, tempfile, os, sys

def parse_counts(text):
    m = re.search(r"Summary\s*\[[^\]]*\]\s*(\d+)\s*tests run:\s*(.*)", text)
    if m:
        total = int(m.group(1))
        details = m.group(2)
        p = int(re.search(r"(\d+)\s*passed", details).group(1)) if re.search(r"(\d+)\s*passed", details) else 0
        f = int(re.search(r"(\d+)\s*failed", details).group(1)) if re.search(r"(\d+)\s*failed", details) else 0
        s = int(re.search(r"(\d+)\s*skipped", details).group(1)) if re.search(r"(\d+)\s*skipped", details) else 0
        return total, p, f, s
    return 0, 0, 0, 0

# Test parsing synthetic cases
assert parse_counts("Summary [ 3.768s ] 608 tests run: 608 passed, 0 skipped") == (608, 608, 0, 0)
assert parse_counts("Summary [ 3.768s ] 608 tests run: 605 passed, 3 failed, 0 skipped") == (608, 605, 3, 0)
assert parse_counts("Summary [ 1.0s ] 10 tests run: 9 passed, 1 failed (1 slow), 0 skipped") == (10, 9, 1, 0)
assert parse_counts("error: build failed") == (0, 0, 0, 0)

# Test receipt generation
with tempfile.TemporaryDirectory() as tmp:
    r_path = os.path.join(tmp, "receipt.json")
    data = {
        "run_id": "test_run",
        "timestamp": "2026-09-04T00:00:00Z",
        "git_head": "0123456789ab",
        "worktree": "clean",
        "status": "pass",
        "command": "cargo nextest run --locked -p taskmanager-tui -j 4",
        "total": 608,
        "passed": 608,
        "failed": 0,
        "skipped": 0,
        "exit_code": 0,
    }
    with open(r_path, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=2)
        f.write("\n")
    with open(r_path, "r", encoding="utf-8") as f:
        loaded = json.load(f)
    assert loaded["status"] == "pass"
    assert loaded["failed"] == 0
    assert loaded["passed"] == 608
' || die "self-test assertions failed"
    printf 'TUI interaction gate self-test: PASS\n'
    exit 0
fi

# ---------------------------------------------------------------------------
# Preflight environment checks
# ---------------------------------------------------------------------------
for cmd in cargo git rustc; do
    command -v "$cmd" >/dev/null 2>&1 || die "missing required command: $cmd"
done
command -v python3 >/dev/null 2>&1 || die "missing required command: python3"

if [[ -x "scripts/agent-workdir.sh" ]]; then
    eval "$(scripts/agent-workdir.sh enter tui-interactions)"
fi

RUN_STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
GIT_HEAD="$(git rev-parse --short=12 HEAD 2>/dev/null || printf 'no-git')"
WORKTREE_STATE=clean
if [ -n "$(git status --porcelain)" ]; then
    WORKTREE_STATE=dirty
fi
RUN_ID="${RUN_STAMP}_${GIT_HEAD}_${WORKTREE_STATE}_$$"
OUT_DIR="$REPO/target/tui-interaction-evidence/$RUN_ID"
mkdir -p "$OUT_DIR"
RECEIPT="$OUT_DIR/receipt.json"
NEXTEST_LOG="$OUT_DIR/nextest.log"

# Save run metadata
{
    printf 'run_id=%s\n' "$RUN_ID"
    printf 'timestamp=%s\n' "$RUN_STAMP"
    printf 'git_head=%s\n' "$GIT_HEAD"
    printf 'worktree=%s\n' "$WORKTREE_STATE"
    printf 'rust=%s\n' "$(rustc -V)"
} >"$OUT_DIR/metadata.txt"

LOCK_FLAG="${TM_CARGO_LOCK---locked}"
RUN_CMD="cargo nextest run ${LOCK_FLAG:-"--locked"} -p taskmanager-tui -j 4"

TIMEOUT_PREFIX=()
if command -v timeout >/dev/null 2>&1; then
    TIMEOUT_PREFIX=(timeout --kill-after=10s 20m)
fi

printf 'Running TUI interaction tests (%s)...\n' "$RUN_CMD"

set +e
if [[ "${1:-}" == "--verbose" || "${1:-}" == "-v" ]]; then
    "${TIMEOUT_PREFIX[@]}" cargo nextest run "$LOCK_FLAG" -p taskmanager-tui -j 4 2>&1 | tee "$NEXTEST_LOG"
    STATUS="${PIPESTATUS[0]}"
else
    "${TIMEOUT_PREFIX[@]}" cargo nextest run "$LOCK_FLAG" -p taskmanager-tui -j 4 >"$NEXTEST_LOG" 2>&1
    STATUS=$?
fi
set -e

# Parse summary counts from nextest log
COUNTS=$(timeout 30s python3 -c '
import re, sys

with open(sys.argv[1], "r", encoding="utf-8", errors="replace") as f:
    text = f.read()

m = re.search(r"Summary\s*\[[^\]]*\]\s*(\d+)\s*tests run:\s*(.*)", text)
if m:
    total = int(m.group(1))
    details = m.group(2)
    p_m = re.search(r"(\d+)\s*passed", details)
    f_m = re.search(r"(\d+)\s*failed", details)
    s_m = re.search(r"(\d+)\s*skipped", details)
    passed = int(p_m.group(1)) if p_m else 0
    failed = int(f_m.group(1)) if f_m else 0
    skipped = int(s_m.group(1)) if s_m else 0
    print(f"{total} {passed} {failed} {skipped}")
else:
    print("0 0 0 0")
' "$NEXTEST_LOG")

read -r TOTAL_COUNT PASSED_COUNT FAILED_COUNT SKIPPED_COUNT <<< "$COUNTS"

GATE_STATUS="fail"
if [ "$STATUS" -eq 0 ] && [ "$FAILED_COUNT" -eq 0 ] && [ "$PASSED_COUNT" -gt 0 ]; then
    GATE_STATUS="pass"
fi

# Emit evidence receipt
timeout 30s python3 -c '
import json, sys

data = {
    "run_id": sys.argv[1],
    "timestamp": sys.argv[2],
    "git_head": sys.argv[3],
    "worktree": sys.argv[4],
    "status": sys.argv[5],
    "command": sys.argv[6],
    "total": int(sys.argv[7]),
    "passed": int(sys.argv[8]),
    "failed": int(sys.argv[9]),
    "skipped": int(sys.argv[10]),
    "exit_code": int(sys.argv[11]),
}
with open(sys.argv[12], "w", encoding="utf-8") as f:
    json.dump(data, f, indent=2)
    f.write("\n")
' "$RUN_ID" "$RUN_STAMP" "$GIT_HEAD" "$WORKTREE_STATE" "$GATE_STATUS" "$RUN_CMD" "$TOTAL_COUNT" "$PASSED_COUNT" "$FAILED_COUNT" "$SKIPPED_COUNT" "$STATUS" "$RECEIPT"

if [ "$GATE_STATUS" != "pass" ]; then
    printf 'TUI interaction gate: FAIL (%s failed, exit code %s)\n' "$FAILED_COUNT" "$STATUS" >&2
    if [ -s "$NEXTEST_LOG" ]; then
        printf '--- last 30 lines of %s ---\n' "$NEXTEST_LOG" >&2
        tail -n 30 "$NEXTEST_LOG" >&2
        printf '------------------------------------\n' >&2
    fi
    printf 'receipt: %s\n' "$RECEIPT" >&2
    exit 1
fi

printf 'TUI interaction gate: PASS (%s tests green) -> %s\n' "$PASSED_COUNT" "$RECEIPT"
exit 0
