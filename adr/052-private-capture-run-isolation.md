# ADR-052: UUID-scoped private capture runs

Status: accepted

## Context

The Linux screenshot route must be usable by several workers at once without
registering a TaskForest tray item on the host bus, stealing the host desktop
focus, mixing windows, overwriting receipts, or leaving nested KWin/Niri
processes behind. A timestamp/PID directory, shell EXIT trap, and process-group
signals are not sufficient: an interrupted run can orphan a private KWin/Niri
pair, and shared `target/debug` or `latest` paths can race with a sibling run.

## Decision

1. Every capture invocation is assigned a random UUID by the private supervisor.
2. The UUID owns the evidence directory, immutable application binary, runtime,
   XDG config/data/cache/state directories, D-Bus session and Wayland/KWin
   socket identity. Fixed service names are permitted only inside that private
   D-Bus session.
3. A user-owned cgroup v2 leaf and detached watchdog own the entire child tree.
   Cleanup signals only that leaf; stale leases are reclaimable by UUID and PID
   start time. The route fails closed when cgroup ownership cannot be proved.
4. Cargo builds are serialized only during the shared-target build and then
   copied into the UUID-owned binary path. Complete validated evidence is
   published through a per-frontend lock and an atomic `latest` pointer.
5. The dual-run isolation test is a release gate: A and B run concurrently,
   each sees only its own window, terminating A leaves B alive, host D-Bus and
   Wayland state remain unchanged, and both runs end with zero residue.

## Consequences

- Private captures do not create host tray, portal, notification or input-method
  services and cannot use the host Wayland socket.
- A failed or interrupted run retains diagnostic receipts but not its runtime
  tree; hard-killed supervisors are reaped by the detached watchdog or the
  UUID-scoped reclaim command.
- Shared Cargo compilation remains a bounded resource, not a runtime ownership
  boundary. No capture may execute the shared `target/debug` binary directly.

## Verification

- `scripts/capture_supervisor.py` drill checks prove cgroup creation, normal
  cleanup and supervisor hard-kill reaping.
- `scripts/capture_publish.py --self-test` proves locked UUID pointer rotation.
- `scripts/test_capture_isolation.py` proves the concurrent A/B contract on a
  private virtual KWin route; it never enables visible capture mode.
- The frontend validators require the UUID, supervisor, cgroup, runtime and
  binary ownership fields before accepting a receipt.
