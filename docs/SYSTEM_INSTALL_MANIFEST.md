# TaskForest system-install manifest

This page defines the safety rules and removal authority for every file
TaskForest may install into the host system or the developer's user data
directories. The machine-readable inventory is
[`system-install-manifest.tsv`](system-install-manifest.tsv); a path may not be
added to a packaging recipe, polkit policy, setup helper, or developer install
script without a row in that TSV first.

## Inventory sources

The current allowlist is `system-install-manifest.tsv`. Host observations,
ownership, hashes and install state belong to a local receipt under
`.private/install-receipts/`; no host receipt is part of the public repository.
This document does not copy host counts, audit dates or receipt status.

## Non-negotiable rules

1. The TSV is the allowlist. There are no wildcard destinations and no
   untracked “temporary” system files.
2. Package-owned files are installed and removed only by the package manager.
   The agent must not manually delete `/usr/bin`, `/usr/share`, or package-owned
   `/usr/libexec` files.
3. Optional root files are installed only by a named helper or the named
   install manager in the TSV. The polkit manager refuses symlinks,
   missing standard parent directories, different existing content, and
   non-root ownership. It publishes files with an atomic same-directory hard
   link, so it never overwrites a race winner.
   It may create only the manifest-listed `/usr/libexec` directory when absent;
   that shared standard directory is never removed by the manager.
4. Every root write has a preflight check, an exact source hash, a post-write
   mode/owner/hash check, and a local host receipt. Removal checks the
   recorded installed hash before deleting anything; a changed or unknown file
   is left in place and reported as a conflict.
5. No helper is setuid or setcap. The main binary remains unprivileged. The
   two process-control files only authorize the fixed `pkexec` action already
   described by ADR-023.
6. Installation never reloads, stops, or edits system services. The optional
   udev rule is a separate, explicit `install`/`revert` action with its own
   exact-content conflict guard.
7. Future installation work must add: a TSV row, purpose/owner, source,
   permissions, conflict policy, install command, removal command, and a
   local-receipt field before any system write is attempted. The manifest guard
   runs in CI to reject drift.

## Inventory by responsibility

| Group | Files and responsibility | Owner / removal authority |
|---|---|---|
| Package base | `/usr/bin/taskmanager` compatibility entry, the `taskforest-g` GPUI binary, the TaskForestG `.desktop` entry, AppStream metadata, SVG icon, setup payload, setup polkit policy, and the generated third-party notices under `/usr/share/licenses/taskforest/` | Root package transaction — the same staged tree ships as the Arch package, the `.deb`, and the `.rpm` (layout authority: `packaging/arch/PKGBUILD`); remove with `pacman -Rns taskforest-git` / `dpkg -r taskforest` / `rpm -e taskforest`, never ad hoc `rm` |
| Optional RAPL setup | `/etc/udev/rules.d/99-taskmanager.rules` | Root, but only through `taskmanager-setup-helper install/revert`; exact content and atomic rollback |
| GPU PMU optional capability | `taskmanager-privilege-helper` plus `com.taskforest.perf-helper.policy` | Package transaction or [`scripts/manage-polkit-install.sh`](../scripts/manage-polkit-install.sh) `perf` transaction |
| Per-process network optional capability | `taskmanager-net-launcher` plus `com.taskforest.net-launcher.policy` | Package transaction or the same manager's `net` transaction |
| Process-control capability | `taskmanager-process-control-helper` plus `com.taskforest.process-control.policy` | Package transaction or the same manager's `process` transaction |
| Developer user integration | User-local TaskForestG/TaskForestI `.desktop` entries, shared SVG, conditional `index.theme`, and one ownership receipt | [`scripts/dev-install-frontends.sh`](../scripts/dev-install-frontends.sh); user-owned and separate from root package files |

The full path, artifact, permission, conflict, and removal fields are kept in
the TSV instead of being inferred from this summary table.

## Polkit helper installation procedure

The manager accepts exactly one feature token: `perf`, `net`, or `process`.
Each transaction owns only that feature's helper and policy pair. For the GPU
PMU feature shown in the UI:

```bash
cargo build --locked --release -p taskmanager-privilege-helper
timeout --kill-after=10s 30s scripts/manage-polkit-install.sh status perf
timeout --kill-after=10s 30s scripts/test-system-install-manager.sh
sudo timeout --kill-after=10s 30s scripts/manage-polkit-install.sh install perf
timeout --kill-after=10s 30s scripts/manage-polkit-install.sh verify perf
```

`install` writes exactly these two destinations:

```text
/usr/libexec/taskmanager-privilege-helper
/usr/share/polkit-1/actions/com.taskforest.perf-helper.policy
```

It does not create a daemon, edit a service, reload polkit, change ownership
of any other file, or touch `/etc/udev`. It refuses package-owned paths,
different existing content, symlinks and missing standard parents. After
installation, record both hashes and metadata in the local receipt. Use the
corresponding Cargo package and feature token for `net` or `process`.

## Removal procedure

For a manager-owned pair:

```bash
sudo timeout --kill-after=10s 30s scripts/manage-polkit-install.sh uninstall perf
timeout --kill-after=10s 30s scripts/manage-polkit-install.sh status perf
```

The manager first checks the recorded receipt and refuses to remove a file if
its content, type, ownership, or path differs from the recorded artifact. A
package-owned artifact is removed through its package manager instead. The
optional RAPL rule is reverted only with the fixed setup helper; the developer
files are removed only through the developer script. This distinction prevents
an upgrade or an unrelated administrator file from being mistaken for
TaskForest residue.

The developer frontend installer follows the same ownership rule without root:
it may adopt pre-existing TaskForestG/TaskForestI entries and the shared icon
only when their bytes exactly match the requested install. It records hashes in
`$XDG_DATA_HOME/taskforest/dev-install-frontends.tsv`. A pre-existing hicolor
`index.theme` is recorded as external and is never changed or removed; when the
index is absent, the script installs and records its own copy. Uninstall first
verifies every managed hash, then removes only the two current desktop entries,
the shared icon, its receipt, and an index recorded as managed. The public
installer owns only current TaskForest entries and never removes shared assets
it did not record.

History persistence is deliberately absent from the installation manifest. The
active frontend session owns its writer and replay worker at runtime; developer
and distribution installs create no service, activation link, LaunchAgent, or
registry value for history.

The current writer emits receipt schema 2. Its reader accepts schema 1 receipts
only for the same hash verification and removal rules; installation does not
extend a receipt with any runtime service ownership.

## Manifest maintenance

Any helper, desktop asset, udev rule, policy, cache, registry, or user-local
file must be added to the same manifest before installation;
“temporary”, “just a cache”, and “cleanup later” are not exceptions.
