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

## Manifest model

The TSV holds exactly one authoritative row per destination. A destination
provided by several packages keeps that single row and lists every provider in
`install_method`, joined by ` or `. Each token is either a repository-relative
recipe path (`packaging/arch/PKGBUILD`, `packaging/debian/build-deb-iced.sh`,
`packaging/rpm/taskforest-i.spec`) or a parameterised Windows build
(`packaging/windows/build-msi.sh ui=G`). For package-managed rows `install_method`
is the authoritative provider list, and `source` names the definition those
providers implement — the provider recipes for Linux packages, or the WiX manifest
`packaging/windows/taskforest.wxs` for MSI rows. For helper and developer rows
`source` names the single script or source artifact that owns the path. The
manifest guard requires every destination written by a scanned recipe to name that
recipe as a provider, so a shared path can no longer be silently credited to one
package.
`removal_method` names every removal authority; the installed copy belongs to
whichever package owns it in its transaction.

The shared hicolor icon set is the current cross-package case. The
`taskforest-common` data package is its single DEB and RPM owner: the frontend
packages depend on it, the Iced and Bevy recipes no longer install icons, and
the GPUI `.deb`/`.rpm` builders remove the manifest-declared common
destinations from the PKGBUILD-derived tree. The monolithic Arch package
(`packaging/arch/PKGBUILD`) still ships the icons for its own single-package
install and is credited as a provider. The manifest guard rejects any
destination installed by both the common package and another DEB or RPM
recipe, so the multi-provider overflow cannot return.

## Windows MSI destinations

Each MSI is a per-machine product for one UI target. The WiX file
[`packaging/windows/taskforest.wxs`](../packaging/windows/taskforest.wxs) is the
file-manifest authority and `packaging/windows/build-msi.sh` binds the UI target.
Non-Linux landing spots are expressed with WiX standard-directory tokens and
Windows separators instead of a `/usr` prefix:

```text
[ProgramFiles64Folder]\TaskForest\   per-UI executable, shared helper, LICENSE, notices
[ProgramMenuFolder]\TaskForest\      per-UI Start Menu shortcut
HKLM\Software\TaskForest\            install markers (StartMenuShortcut, Version)
```

`INSTALLFOLDER` is user-selectable through `WixUI_InstallDir`, so no drive letter
or user profile appears in the destination; the token names the WiX standard root.
The provider token is `packaging/windows/build-msi.sh ui=<G|I|T|B>`. The frontend
executable and Start Menu shortcut are per-UI; the process-control helper,
`LICENSE`, `THIRD-PARTY-NOTICES.txt`, and the family install markers live in
components whose GUID is identical in every product, so Windows Installer
reference-counts them and removes them only when the last product uninstalls.
There is no separate common MSI. The MSI installs no service or autostart entry,
and the Linux polkit helpers are not part of it.

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
| Package base | `/usr/bin/taskmanager` compatibility entry, the `taskforest-g` GPUI binary, the TaskForestG `.desktop` entry, AppStream metadata, setup payload, setup polkit policy, and the generated third-party notices under `/usr/share/licenses/taskforest/` | Root package transaction — the same staged tree ships as the Arch package, the `.deb`, and the `.rpm` (layout authority: `packaging/arch/PKGBUILD`); remove with `pacman -Rns taskforest-git` / `dpkg -r taskforest` / `rpm -e taskforest`, never ad hoc `rm` |
| Shared frontend icons | Hicolor SVG and PNG ladder under `/usr/share/icons/hicolor/`, owned by the `taskforest-common` DEB/RPM data package and shipped by the monolithic Arch package | Package transaction for the owning package (`taskforest-common` for DEB/RPM, `taskforest-git` for Arch); the byte-identical asset is never installed by a second package in the same format |
| Optional RAPL setup | `/etc/udev/rules.d/99-taskforest.rules` | Root, but only through `taskforest-setup-helper install/revert`; exact content and atomic rollback |
| GPU PMU optional capability | `taskforest-privilege-helper` plus `io.github.YellowWhiteBlackCat.TaskForest.perf-helper.policy` | Package transaction or [`scripts/manage-polkit-install.sh`](../scripts/manage-polkit-install.sh) `perf` transaction |
| Per-process network optional capability | `taskforest-net-launcher` plus `io.github.YellowWhiteBlackCat.TaskForest.net-launcher.policy` | Package transaction or the same manager's `net` transaction |
| Process-control capability | `taskforest-process-control-helper` plus `io.github.YellowWhiteBlackCat.TaskForest.process-control.policy` | Package transaction or the same manager's `process` transaction |
| SMBIOS memory optional capability | `taskforest-smbios-helper` plus `io.github.YellowWhiteBlackCat.TaskForest.smbios-helper.policy` | Package transaction or the same manager's `smbios` transaction |
| RAPL package-power optional capability | `taskforest-rapl-helper` plus `io.github.YellowWhiteBlackCat.TaskForest.rapl-helper.policy` | Package transaction or the same manager's `rapl` transaction |
| MSR readout optional capability | `taskforest-msr-helper` plus `io.github.YellowWhiteBlackCat.TaskForest.msr-helper.policy` | Package transaction or the same manager's `msr` transaction |
| Developer user integration | User-local TaskForestG/TaskForestI `.desktop` entries, shared SVG, conditional `index.theme`, and one ownership receipt | [`scripts/dev-install-frontends.sh`](../scripts/dev-install-frontends.sh); user-owned and separate from root package files |
| Windows MSI products | Per-UI `taskforest-<ui>.exe` and Start Menu shortcut, plus the shared process-control helper, license notices, and `HKLM\Software\TaskForest` markers | Windows Installer — one MSI per UI target; remove with `msiexec /x TaskForest-<UI>.msi` |

The full path, artifact, permission, conflict, and removal fields are kept in
the TSV instead of being inferred from this summary table.

## Polkit helper installation procedure

The manager accepts exactly one feature token: `perf`, `net`, `process`,
`smbios`, `rapl`, or `msr`.
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
/usr/libexec/taskforest-privilege-helper
/usr/share/polkit-1/actions/io.github.YellowWhiteBlackCat.TaskForest.perf-helper.policy
```

It does not create a daemon, edit a service, reload polkit, change ownership
of any other file, or touch `/etc/udev`. It refuses package-owned paths,
different existing content, symlinks and missing standard parents. After
installation, record both hashes and metadata in the local receipt. Use the
corresponding Cargo package and feature token for `net`, `process`, `smbios`,
`rapl`, or `msr`.

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
