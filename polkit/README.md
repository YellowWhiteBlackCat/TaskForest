# TaskForest per-feature escalation policy (polkit)

This directory ships the polkit authorization policies for TaskForest's
per-feature privilege-escalation helpers (ADR-023; [`../docs/PERMISSION_MODEL.md`](../docs/PERMISSION_MODEL.md)
Boundary 2).

It authorizes **six** privileged operations, each behind its own action + helper
binary so the privileged attack surface stays one-feature/one-capability:

1. reading the Intel i915/xe GPU performance-monitoring unit via
   `perf_event_open` (`taskforest-privilege-helper`); and
2. opening an `AF_PACKET` raw socket (`CAP_NET_RAW`) for per-process network
   accounting, then handing the fd to the unprivileged app via `SCM_RIGHTS`
   (`taskforest-net-launcher`, ADR-024/025); and
3. applying one identity-checked control operation to a selected foreign-uid
   process (`taskforest-process-control-helper`); and
4. reading the raw SMBIOS records — type-17 memory devices (module speed,
   slot population, part/serial numbers) plus the type 0/1/2 identity tables
   (board/system serial, product UUID, asset tag) — which the kernel exposes
   root-only (`taskforest-smbios-helper`); and
5. sampling the Intel RAPL package energy counters over one fixed window to
   derive per-package watt figures (`taskforest-rapl-helper`); and
6. reading the verified Intel MSR readouts — package temperature, P-state
   multipliers, P-state core voltage — from the root-only `/dev/cpu/*/msr`
   nodes via plain `pread` file I/O (`taskforest-msr-helper`, ADR-048).

In all cases the main TaskForest app never holds privilege: it receives typed
results (the PMU read or process-control result) or an owned fd (the AF_PACKET
socket) and runs the rest unprivileged.

The installed helper binaries use the `taskforest-*` product prefix under
`/usr/libexec`; the cargo build artifacts keep the internal `taskmanager-*`
crate names and are mapped at install time (see
[`../docs/PRODUCT_IDENTITY.md`](../docs/PRODUCT_IDENTITY.md)).

## Files

- `io.github.YellowWhiteBlackCat.TaskForest.perf-helper.policy.in` — polkit
  action for the GPU PMU helper
  (`io.github.YellowWhiteBlackCat.TaskForest.perf-helper`).
- `io.github.YellowWhiteBlackCat.TaskForest.net-launcher.policy.in` — polkit
  action for the AF_PACKET launcher
  (`io.github.YellowWhiteBlackCat.TaskForest.net-launcher`).
- `io.github.YellowWhiteBlackCat.TaskForest.process-control.policy.in` — polkit
  action for one selected foreign-process control operation
  (`io.github.YellowWhiteBlackCat.TaskForest.process-control`).
- `io.github.YellowWhiteBlackCat.TaskForest.smbios-helper.policy.in` — polkit
  action for the read-only SMBIOS memory-inventory helper
  (`io.github.YellowWhiteBlackCat.TaskForest.smbios-helper`).
- `io.github.YellowWhiteBlackCat.TaskForest.rapl-helper.policy.in` — polkit
  action for the read-only RAPL package-power helper
  (`io.github.YellowWhiteBlackCat.TaskForest.rapl-helper`).
- `io.github.YellowWhiteBlackCat.TaskForest.msr-helper.policy.in` — polkit
  action for the read-only MSR readout helper
  (`io.github.YellowWhiteBlackCat.TaskForest.msr-helper`).

Action ids share the same reverse-DNS namespace as the desktop app ids, so the
`.policy` installed filename always matches its action id. The first-run-setup
action is packaging-owned: its policy ships from
`packaging/linux/io.github.YellowWhiteBlackCat.TaskForest.setup.policy` and
installs under that filename, outside this directory (see
[`../docs/PRODUCT_IDENTITY.md`](../docs/PRODUCT_IDENTITY.md)).

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
publish the six declared policy/helper pairs only after their manifest rows
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
- the SMBIOS and RAPL helpers are read-only one-shot probes of fixed sysfs
  trees (`/sys/firmware/dmi/entries/17-*`, `/sys/class/powercap/intel-rapl:*`)
  with no flags and no file writes; the RAPL helper sleeps one fixed sample
  window between its two counter reads;
- the MSR helper is a read-only one-shot sweep of the root-only
  `/dev/cpu/*/msr` character nodes (`pread` at the register-address offset —
  plain file I/O, so no fifth audited `unsafe` trust root; ADR-048) with no
  flags and no file writes; unimplemented registers decode to typed nulls,
  never fabricated numbers;
- everything else in TaskForest still runs as the normal user.

This is the operationalization of ADR-023; see that ADR and
[`../docs/PERMISSION_MODEL.md`](../docs/PERMISSION_MODEL.md) for the full model.

## Verifying on-box

Only run the following probes after the package/install manager has recorded a
matching manifest receipt; these commands do not authorize or install files.

```sh
# GPU PMU helper — prints one JSON object on stdout, then exits.
pkexec /usr/libexec/taskforest-privilege-helper

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
`taskmanager --memory-smbios`, `taskmanager --package-power` and
`taskmanager --msr` surfaces drive the SMBIOS, RAPL and MSR helpers the same
way. The
per-process-network path degrades to `RequiresEscalation(PerProcessNet)`
unprivileged and offers the prompt when the user enables the feature.

The foreign-process path first attempts the normal syscall. Only a typed
permission denial after the selected target was revalidated can trigger the
process-control helper; identity races and unsupported operations never launch
it. A missing helper or a refused prompt remains a typed failure.
