# TaskForest per-feature escalation policy (polkit)

This directory ships the polkit authorization policies for TaskForest's
per-feature privilege-escalation helpers (ADR-023; [`../docs/PERMISSION_MODEL.md`](../docs/PERMISSION_MODEL.md)
Boundary 2).

It authorizes **three** privileged operations, each behind its own action + helper
binary so the privileged attack surface stays one-feature/one-capability:

1. reading the Intel i915/xe GPU performance-monitoring unit via
   `perf_event_open` (`taskmanager-privilege-helper`); and
2. opening an `AF_PACKET` raw socket (`CAP_NET_RAW`) for per-process network
   accounting, then handing the fd to the unprivileged app via `SCM_RIGHTS`
   (`taskmanager-net-launcher`, ADR-024/025); and
3. applying one identity-checked control operation to a selected foreign-uid
   process (`taskmanager-process-control-helper`).

In all cases the main TaskForest app never holds privilege: it receives typed
results (the PMU read or process-control result) or an owned fd (the AF_PACKET
socket) and runs the rest unprivileged.

## Files

- `com.taskforest.perf-helper.policy.in` — polkit action for the GPU PMU helper
  (`com.taskforest.perf-helper`).
- `com.taskforest.net-launcher.policy.in` — polkit action for the AF_PACKET
  launcher (`com.taskforest.net-launcher`).
- `com.taskforest.process-control.policy.in` — polkit action for one selected
  foreign-process control operation (`com.taskforest.process-control`).

The `.in` suffix marks these as templates ready for distribution packaging; the
contents are already valid polkit XML and may be installed verbatim under the
`.policy` name.

## Installation authority

These files are package inputs, not a manual installation interface. Do not run
ad-hoc `install`, `chown`, service reload, or policy registration commands from
this README. The allowlist, exact source hashes, owners/modes, conflict policy,
removal procedure, and host receipt are authoritative in
[`../docs/SYSTEM_INSTALL_MANIFEST.md`](../docs/SYSTEM_INSTALL_MANIFEST.md).

The `org.freedesktop.policykit.exec.path` annotation in each policy and the
helper argument must match exactly. A package or approved install manager may
publish the three declared policy/helper pairs only after their manifest rows
and receipts exist.

## Why `pkexec` + polkit (security rationale)

TaskForest runs **unprivileged by default** on every platform (Boundary 2). It
carries no blanket capability — no `setcap`, no `setuid` on the main binary —
and never launches elevated. The few telemetry domains an unprivileged user
cannot read directly (Boundary 3) are reached through **small privileged
helpers** invoked via the OS-native escalation prompt:

- on Linux, polkit `.policy` + `pkexec` (this directory);
- on Windows, an elevated helper + UAC manifest (planned);
- on macOS, an authorization-required helper (planned).

Each helper performs **only** its privileged op and returns safe typed data (or,
for the net launcher, an owned fd via `SCM_RIGHTS`). The main app never receives
a raw capability. The attack surface is minimal:

- one audited privileged binary per feature, one action, one capability each;
- `auth_admin_keep` on the active session means a single prompt authorizes the
  helper for the session, while inert (`allow_any`) sessions are denied outright;
- the net launcher is one-shot (open + pass fd + exit); the unprivileged app
  then holds the fd and runs the capture loop — no long-lived privileged daemon;
- the process-control helper is one-shot, accepts only a fixed operation, and
  revalidates the PID's `/proc` start token immediately before the syscall;
- everything else in TaskForest still runs as the normal user.

This is the operationalization of ADR-023; see that ADR and
[`../docs/PERMISSION_MODEL.md`](../docs/PERMISSION_MODEL.md) for the full model.

## Verifying on-box

Only run the following probes after the package/install manager has recorded a
matching manifest receipt; these commands do not authorize or install files.

```sh
# GPU PMU helper — prints one JSON object on stdout, then exits.
pkexec /usr/libexec/taskmanager-privilege-helper

# AF_PACKET launcher — connects to the given abstract-namespace Unix socket
# name (hex on argv; no filesystem path is created), passes the fd via
# SCM_RIGHTS, and exits 0 on ACK. On any failure it prints a typed ERROR JSON
# and exits non-zero (it does NOT pass a bad fd). The app side binds the
# abstract socket, rejects peers whose SO_PEERCRED uid is not 0, and recv's the
# fd; this manual form needs that receiver listening before you run the
# launcher.
```

The TaskForest CLI surface `taskmanager --gpu-engines` runs the PMU path
end-to-end unprivileged: it triggers `pkexec` only when invoked, prints the
typed engine data on success, and a typed honest denial otherwise. The
per-process-network path degrades to `RequiresEscalation(PerProcessNet)`
unprivileged and offers the prompt when the user enables the feature.

The foreign-process path first attempts the normal syscall. Only a typed
permission denial after the selected target was revalidated can trigger the
process-control helper; identity races and unsupported operations never launch
it. A missing helper or a refused prompt remains a typed failure.
