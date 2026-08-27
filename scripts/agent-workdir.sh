#!/usr/bin/env bash
# Agent work-directory + build-cache lease manager.
#
# Keeps Cargo transient output on the repository NVMe (never /tmp) and points
# Cargo at the SHARED <root>/target so the dependency graph is
# compiled once and reused, not rebuilt per task. See AGENTS.md,
# "Temporary-File & Build-Isolation Discipline".
#
# Usage:
#   eval "$(scripts/agent-workdir.sh enter my-feature)"   # redirect TMPDIR/CARGO_TARGET_DIR + arm cleanup
#   scripts/agent-workdir.sh reclaim                       # reap stale leases (recorded pid no longer alive)
#   scripts/agent-workdir.sh gate                          # exit 1 if /tmp holds a Cargo target/ or repo copy
#
# `.tmp/` is gitignored. The lease dir is named <task>-<caller-pid>-<epoch> so a
# crashed run leaves a reclaimable lease rather than anonymous litter.

set -euo pipefail

root="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
runs="$root/.tmp/agent-runs"
shared_target="$root/target"
mkdir -p "$runs"

cmd="${1:-enter}"
task="${2:-task}"

# Remove lease dirs whose recorded pid is no longer alive. Safe across concurrent
# runs: a live sibling's pid passes `kill -0` and is left untouched.
reclaim_stale() {
  shopt -s nullglob 2>/dev/null || true
  for lease_file in "$runs"/*/LEASE; do
    [ -f "$lease_file" ] || continue
    local pid=""
    pid="$(grep -E '^pid=' "$lease_file" | head -1 | cut -d= -f2- || true)"
    if [ -n "$pid" ] && ! kill -0 "$pid" 2>/dev/null; then
      local dir
      dir="$(dirname "$lease_file")"
      rm -rf "$dir" && echo "agent-workdir: reclaimed stale lease $dir" >&2
    fi
  done
}

case "$cmd" in
  enter)
    reclaim_stale
    # $$ would be this script's pid (a child of the caller); $PPID is the caller's
    # shell, which is what the trap + reclaim should key on.
    pid="$PPID"
    ts="$(date +%s)"
    lease="$runs/${task}-${pid}-${ts}"
    mkdir -p "$lease/tmp"
    printf 'task=%s\npid=%s\nstarted=%s\nhost=%s\n' \
      "$task" "$pid" "$ts" "$(hostname 2>/dev/null || echo unknown)" > "$lease/LEASE"
    # Emit shell for the caller to eval. Variables resolve here to literal paths;
    # \$TMPDIR/\$CARGO_TARGET_DIR in the echo are left for eval-time expansion.
    cat <<EOF
export TMPDIR="$lease/tmp"
export CARGO_TARGET_DIR="$shared_target"
export TASKMGR_AGENT_LEASE="$lease"
trap 'rm -rf "$lease"' EXIT
echo "agent-workdir: TMPDIR=\$TMPDIR  CARGO_TARGET_DIR=\$CARGO_TARGET_DIR  lease=$lease" >&2
EOF
    ;;
  reclaim)
    reclaim_stale
    ;;
  gate)
    # Fail if /tmp contains a Cargo target/ tree or a full repository copy.
    # These are the two signatures that exhausted the tmpfs in the soultower incident.
    status=0

    # (1) any Cargo target/ under /tmp — signature: a `.rustc` metadata subdir.
    while IFS= read -r -d '' rustc_dir; do
      status=1
      echo "agent-workdir GATE: Cargo target/ under /tmp -> $(dirname "$rustc_dir")" >&2
    done < <(find /tmp -maxdepth 6 -type d -name '.rustc' -print0 2>/dev/null || true)

    # (2) a full repository copy under /tmp — Cargo.toml + rust-toolchain.toml +
    # (crates/ or src/). rust-toolchain.toml makes this taskmanager-specific and
    # avoids flagging unrelated Cargo projects living in /tmp.
    while IFS= read -r -d '' cargo_toml; do
      dir="$(dirname "$cargo_toml")"
      if [ -f "$dir/rust-toolchain.toml" ] && { [ -d "$dir/crates" ] || [ -d "$dir/src" ]; }; then
        status=1
        echo "agent-workdir GATE: full repository copy under /tmp -> $dir" >&2
      fi
    done < <(find /tmp -maxdepth 5 -type f -name 'Cargo.toml' -print0 2>/dev/null || true)

    if [ "$status" -eq 0 ]; then
      echo "agent-workdir GATE: clean — no Cargo target/ or repo copy under /tmp" >&2
    fi
    exit "$status"
    ;;
  *)
    echo "usage: agent-workdir.sh {enter <task> | reclaim | gate}" >&2
    exit 2
    ;;
esac
