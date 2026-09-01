//! source-inspection: static-policy
//!
//! Frontend dependency/IO safety firewall.

use std::fs;

use super::{
    FORBIDDEN_NATIVE_IO_AND_COMMANDS, assert_boundary_consumers_are_sanctioned,
    assert_boundary_crate_has_no_workspace_dependencies, boundary_source_files,
    contains_command_constructor, frontend_sources, is_boundary_crate_path, is_crate_root,
    production_workspace_dependencies, read_source, repository, rust_code_without_line_comments,
    strip_line_comments, unsanctioned_dependents, workspace_crate_manifests,
};

#[test]
fn process_connections_split_transport_family_endpoint_and_provider_token_shapes() {
    let repository = repository();
    let model = read_source(
        &repository,
        "crates/taskmanager-core/src/core/process_telemetry/connection.rs",
    );
    for conflated in [
        "pub enum ConnectionProtocol",
        "pub local: SocketAddr",
        "pub remote: SocketAddr",
        "pub provider_key: u64",
    ] {
        assert!(
            !model.contains(conflated),
            "shared process connection still leaks a legacy shape: {conflated}"
        );
    }

    let linux = read_source(
        &repository,
        "crates/taskmanager-platform-linux/src/engine/process/telemetry/network.rs",
    );
    assert!(!linux.contains("ConnectionProtocol"));
    assert!(!linux.contains("SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED)"));

    let ui = read_source(
        &repository,
        "crates/taskmanager-gpui/src/gpui_app/process_insights/view.rs",
    );
    assert!(!ui.contains("ConnectionProtocol"));
}

#[test]
fn frontend_sources_import_domain_owners_directly() {
    // Domain facts and platform-neutral port contracts have one owner each.
    // Frontends import those owners directly; no application/model facade or
    // frontend-local `crate::core` forwarding module is allowed to grow back.
    let repository = repository();
    let (gpui, tui) = frontend_sources(&repository);
    assert!(
        !repository
            .join("crates/taskmanager-application/src/model.rs")
            .exists(),
        "the retired application model forwarding facade must not return"
    );
    for forbidden in ["taskmanager_platform_linux", "taskmanager_ebpf"] {
        assert!(
            !gpui.contains(forbidden),
            "GPUI frontend reached a forbidden platform implementation: {forbidden}"
        );
    }

    for forbidden in [
        "taskmanager_platform_linux",
        "taskmanager_platform_provider",
        "taskmanager_platform_runtime",
        "taskmanager_telemetry_store",
        "taskmanager_history_store",
        "taskmanager_ebpf",
    ] {
        assert!(
            !tui.contains(forbidden),
            "TUI frontend reached a forbidden platform implementation: {forbidden}"
        );
    }

    for (name, root) in [
        // (ADR-051) the root gates host ships no business code and is
        // intentionally absent here; the shared CLI harness consumes core
        // facts for the UI-neutral modes like any composition edge.
        ("Shared CLI", "crates/taskmanager-cli/src"),
        ("Application", "crates/taskmanager-application/src"),
        ("App host", "crates/taskmanager-app-host/src"),
        ("GPUI", "crates/taskmanager-gpui/src"),
        ("Iced", "crates/taskmanager-iced/src"),
        (
            "Platform contract",
            "crates/taskmanager-platform-contract/src",
        ),
        ("Platform native", "crates/taskmanager-platform-native/src"),
        ("TUI", "crates/taskmanager-tui/src"),
        ("Bevy", "crates/taskmanager-bevy-ui/src"),
    ] {
        let code = rust_code_without_line_comments(&repository.join(root));
        assert!(
            code.contains("taskmanager_core"),
            "{name} must consume domain facts from taskmanager-core directly"
        );
        assert!(
            !code.contains("pub use taskmanager_application")
                && !code.contains("pub use taskmanager_core")
                && !code.contains("pub use taskmanager_platform_contract"),
            "{name} restored a cross-layer forwarding re-export"
        );
    }
}

#[test]
fn frontend_production_sources_own_no_history_storage_primitive() {
    let repository = repository();
    for (name, root) in [
        ("GPUI", "crates/taskmanager-gpui/src"),
        ("Iced", "crates/taskmanager-iced/src"),
        ("TUI", "crates/taskmanager-tui/src"),
    ] {
        let code = rust_code_without_line_comments(&repository.join(root));
        for forbidden in [
            "taskmanager_history_store",
            "PersistentHistoryStore",
            "BootEvidenceHistory",
            "HistoryQuery",
        ] {
            assert!(
                !code.contains(forbidden),
                "{name} frontend regained history storage authority through {forbidden}"
            );
        }
    }
}

#[test]
fn frontends_execute_no_native_commands_and_read_no_native_paths() {
    // Linux collection, parsing, and command execution is physically owned by
    // taskmanager-platform-linux. Frontends may format provider data but must
    // never read platform files or spawn native commands themselves.
    let repository = repository();
    let (gpui, tui) = frontend_sources(&repository);
    for (name, code) in [("GPUI", gpui.as_str()), ("TUI", tui.as_str())] {
        for forbidden in FORBIDDEN_NATIVE_IO_AND_COMMANDS {
            assert!(
                !code.contains(forbidden),
                "{name} frontend leaked native I/O or command invocation: {forbidden}"
            );
        }
        assert!(
            !contains_command_constructor(code),
            "{name} frontend selected a native command with an independent `Command::new` call"
        );
    }
    assert!(
        !gpui.contains("std::fs::"),
        "GPUI frontend must not perform filesystem I/O"
    );
}

#[test]
fn no_workspace_crate_reaches_an_adapter_outside_the_native_composition_edge() {
    // Reverse firewall: the inward list above is exhaustive per crate, but a
    // NEW workspace member is only guarded if it appears there. This generic
    // check scans every manifest in the tree so a fresh crate cannot silently
    // wire itself to an OS adapter (or to a second adapter alongside the
    // platform-native edge) and still pass CI.
    let repository = repository();
    let mut consumers: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for (package, manifest_path) in workspace_crate_manifests(&repository) {
        let manifest = fs::read_to_string(&manifest_path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", manifest_path.display()));
        for dependency in production_workspace_dependencies(&manifest) {
            if matches!(
                dependency.as_str(),
                "taskmanager-platform-linux"
                    | "taskmanager-platform-macos"
                    | "taskmanager-platform-windows"
            ) {
                consumers
                    .entry(dependency)
                    .or_default()
                    .push(package.clone());
            }
        }
    }
    for (adapter, mut dependents) in consumers {
        dependents.sort();
        assert_eq!(
            dependents,
            vec!["taskmanager-platform-native".to_owned()],
            "{adapter} is reachable from a crate other than the platform-native composition edge"
        );
    }
}

pub(super) fn walk_rust_files(root: &std::path::Path) -> Vec<(std::path::PathBuf, String)> {
    fn visit(path: &std::path::Path, out: &mut Vec<(std::path::PathBuf, String)>) {
        let Ok(entries) = std::fs::read_dir(path) else {
            return;
        };
        for entry in entries.flatten() {
            let candidate = entry.path();
            if candidate.is_dir() {
                // Skip build artifacts (the workspace target dir, if ever
                // nested) and integration-test roots: per docs/TEST_LAYOUT.md
                // `tests/` holds test code (including the boundary crates'
                // unsafe-exercising harnesses), not production source.
                if candidate
                    .file_name()
                    .is_some_and(|name| name == "target" || name == "tests")
                {
                    continue;
                }
                visit(&candidate, out);
            } else if candidate
                .extension()
                .is_some_and(|extension| extension == "rs")
                && let Ok(source) = std::fs::read_to_string(&candidate)
            {
                out.push((candidate, source));
            }
        }
    }
    let mut collected = Vec::new();
    visit(root, &mut collected);
    collected
}

#[test]
fn default_build_is_strict_safe_rust_with_zero_unsafe() {
    // The product's headline guarantee: the default build is 100% safe Rust
    // EXCEPT for the audited boundary crates (ADR-022 perf_event_open, ADR-024
    // AF_PACKET, ADR-025 SCM_RIGHTS, ADR-031 Windows system APIs). eBPF (the
    // prior source of `unsafe`) was removed to make the guarantee literally
    // true; these four minimal boundary crates are the documented, audited
    // carve-outs. No OTHER production source
    // under `src/` or `crates/` may contain an `unsafe` construct or
    // `allow(unsafe_code)`, and every non-boundary crate root must forbid/deny
    // unsafe so a new crate cannot silently reintroduce it. The boundary crates'
    // own contract is enforced by
    // `audited_boundary_crate_carries_its_own_unsafe_contract`.
    let repository = repository();
    let mut leaked_unsafe = String::new();
    let mut missing_forbid = Vec::new();
    for directory in ["src", "crates"] {
        for (path, source) in walk_rust_files(&repository.join(directory)) {
            if is_boundary_crate_path(&path, &repository) {
                // Audited trust roots — checked by their own dedicated test.
                continue;
            }
            // Strip `//` line comments (covers `///` and `//!` too) so that
            // doc-comments mentioning the word "unsafe" are not false positives.
            let stripped = strip_line_comments(&source);
            for forbidden in [
                "unsafe {",
                "unsafe impl",
                "unsafe fn",
                "unsafe extern",
                "unsafe trait",
                "allow(unsafe_code)",
            ] {
                if stripped.contains(forbidden) {
                    leaked_unsafe
                        .push_str(&format!("  {} contains `{forbidden}`\n", path.display()));
                }
            }
            if is_crate_root(&path)
                && !source.contains("#![forbid(unsafe_code)]")
                && !source.contains("#![deny(unsafe_code)]")
            {
                missing_forbid.push(path.display().to_string());
            }
        }
    }
    assert!(
        leaked_unsafe.is_empty(),
        "strict safe-Rust build leaked unsafe code into production source:\n{leaked_unsafe}"
    );
    assert!(
        missing_forbid.is_empty(),
        "crate roots missing #![forbid/deny(unsafe_code)]:\n{}",
        missing_forbid.join("\n")
    );
}

/// Single byte is an ASCII identifier character (`[A-Za-z0-9_]`). Used by
/// [`line_contains_identifier_token`] so the Win32 `SOCKET` rule is not tripped
/// by the unrelated Unix constant `libc::SOL_SOCKET`, and `HANDLE` is not
/// tripped by `EventHandler`/`PlatformHandle`.
fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Substring search bounded by non-identifier bytes on both sides, so an
/// identifier-style Win32 raw-handle token (`HANDLE`/`PCWSTR`/`PCSTR`/`PWSTR`/
/// `BSTR`/`SOCKET`) is matched as a WORD and not as a substring of an
/// unrelated identifier. Pointer-type tokens (`*const c_void`, `*mut c_void`)
/// have no identifier ambiguity and stay on plain `.contains()` in the caller.
fn line_contains_identifier_token(line: &str, token: &str) -> bool {
    let bytes = line.as_bytes();
    let mut start = 0;
    while let Some(relative) = line[start..].find(token) {
        let absolute = start + relative;
        let after = absolute + token.len();
        let left_boundary = absolute == 0 || !is_ident_byte(bytes[absolute - 1]);
        let right_boundary = after >= bytes.len() || !is_ident_byte(bytes[after]);
        if left_boundary && right_boundary {
            return true;
        }
        start = absolute + token.len();
    }
    false
}

/// Run the audited-boundary safe-seam scan over one source string and return
/// every violation (one `String` each). Used by
/// `audited_boundary_crate_carries_its_own_unsafe_contract` for each boundary
/// crate, AND by the synthetic-fragment regression test so the scan's
/// OS-neutral widening is pinned against future regressions.
///
/// The scan is OS-neutral by design — it carries TWO dimensions:
/// * the Unix dimension (existing): `as RawFd` + `as *const`/`as *mut` casts,
///   `impl AsRawFd`, and the `RawFd`/`AsRawFd`/`*const`/`*mut` tokens on public
///   items;
/// * the Win32 dimension (added): `as HANDLE`/`as PCWSTR`/`as PCSTR`/
///   `as PWSTR`/`as SOCKET` casts, plus the `HANDLE`/`PCWSTR`/`PCSTR`/`PWSTR`/
///   `BSTR`/`SOCKET`/`*const c_void`/`*mut c_void` tokens on public items.
///
/// Before the Win32 dimension a hypothetical Win32 boundary crate could pass
/// "audited + safe seam" while leaking `HANDLE`/`PCWSTR` across its public API
/// — the CROSSPLATFORM_STRATEGY.md §5.1 P0 blind spot this closes. The three
/// existing Unix boundary crates (perf-ioctl/afpacket/fd-bridge) carry no Win32
/// tokens, so the widening does not change their verdict.
fn scan_safe_seam_violations(source: &str) -> Vec<String> {
    let mut leaks = Vec::new();
    let stripped = strip_line_comments(source);
    // Raw casts — both dimensions. Plain substring match: each cast literal
    // carries the `as ` framing, so there is no identifier ambiguity.
    for cast in [
        "as RawFd",
        "as *const",
        "as *mut",
        // Win32 handle/socket casts (would-be Win32 boundary crate).
        "as HANDLE",
        "as PCWSTR",
        "as PCSTR",
        "as PWSTR",
        "as SOCKET",
    ] {
        if stripped.contains(cast) {
            leaks.push(format!("performed a raw cast `{cast}`"));
        }
    }
    if stripped.contains("impl AsRawFd") {
        leaks.push("implements AsRawFd".to_owned());
    }
    // Keep a small declaration window so a public signature split over lines
    // cannot evade the seam check. We deliberately stop at the first opening
    // brace/semicolon: the function body is private implementation detail,
    // while aggregate fields and nested public items are checked when their
    // own `pub` declaration starts.
    let mut in_public_declaration = false;
    for line in stripped.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("pub ") || trimmed.starts_with("pub(") {
            in_public_declaration = true;
        }
        if !in_public_declaration {
            continue;
        }
        // Substring tokens with no identifier ambiguity: the Unix RawFd family
        // plus the raw-pointer/c_void pointer types (a `*const c_void` line
        // also matches `*const`, which is the existing conservative behavior).
        for token in [
            "RawFd",
            "AsRawFd",
            "*const",
            "*mut",
            "*const c_void",
            "*mut c_void",
        ] {
            if line.contains(token) {
                leaks.push(format!(
                    "public item leaks `{token}` across the safe seam: {line}"
                ));
            }
        }
        // Identifier-style Win32 raw-handle tokens, matched as WORDS so the
        // Unix constant `SOL_SOCKET` does NOT trip the Win32 `SOCKET` rule and
        // `EventHandler`/`PlatformHandle` do NOT trip `HANDLE`.
        for token in ["HANDLE", "PCWSTR", "PCSTR", "PWSTR", "BSTR", "SOCKET"] {
            if line_contains_identifier_token(line, token) {
                leaks.push(format!(
                    "public item leaks `{token}` across the safe seam: {line}"
                ));
            }
        }
        if line.contains('{') || line.contains(';') {
            in_public_declaration = false;
        }
    }
    leaks
}

#[test]
fn audited_boundary_crate_carries_its_own_unsafe_contract() {
    // ADR-022 (perf_event_open) + ADR-024 (AF_PACKET) + ADR-025 (SCM_RIGHTS) +
    // ADR-031 (Windows system APIs): the FOUR audited boundary crates are the
    // ONLY places `unsafe` is permitted. Each crate's own invariants — the
    // audited safe-seam contract — are enforced here for ALL FOUR so
    // "audited + safe seam" is a CI contract,
    // not a claim (invariants below).
    let repository = repository();
    let files = boundary_source_files(&repository);
    // (a) the crate root carries `#![deny(unsafe_op_in_unsafe_fn)]` (NOT
    //     `forbid`, which would disallow the audited opt-out);
    //   (b) every `unsafe {` block and every `unsafe fn` has a `// SAFETY:`
    //       comment on the same line or the line immediately before; and
    //   (c) no raw pointer / RawFd / AsRawFd (Unix) and no Win32 raw handle
    //       (HANDLE/PCWSTR/PCSTR/PWSTR/BSTR/SOCKET, plus `*const c_void`/
    //       `*mut c_void`) crosses the PUBLIC API, and no raw CAST
    //       (`as *const`, `as *mut`, `as RawFd`, or the Win32 `as HANDLE`/
    //       `as PCWSTR`/`as PCSTR`/`as PWSTR`/`as SOCKET`) or `impl AsRawFd`
    //       exists anywhere. A private `use ... AsRawFd` for the audited ioctl
    //       on a File the crate owns is the sanctioned internal escape and is
    //       allowed; exposing it publicly would break the seam. The scan is
    //       OS-neutral — see `scan_safe_seam_violations`.
    let mut missing_deny = Vec::new();
    let mut unsafe_without_safety = Vec::new();
    let mut seam_leak = Vec::new();
    for (path, source) in &files {
        if is_crate_root(path) && !source.contains("#![deny(unsafe_op_in_unsafe_fn)]") {
            missing_deny.push(path.display().to_string());
        }
        // (b) every unsafe block/fn/impl/extern has a // SAFETY: comment on the same line
        // or somewhere in the contiguous `//`-comment block immediately above
        // (a "small window" — a multi-line SAFETY justification is normal).
        let lines: Vec<&str> = source.lines().collect();
        for (idx, line) in lines.iter().enumerate() {
            let code = line.split_once("//").map_or(*line, |(code, _)| code);
            if code.contains("unsafe {")
                || code.contains("unsafe fn")
                || code.contains("unsafe impl")
                || code.contains("unsafe extern")
            {
                let mut has_safety = line.contains("// SAFETY:");
                let mut back = idx;
                while !has_safety && back > 0 {
                    let prev = lines[back - 1];
                    if prev.trim_start().starts_with("//") {
                        has_safety = prev.contains("// SAFETY:");
                        back -= 1;
                    } else {
                        break;
                    }
                }
                if !has_safety {
                    unsafe_without_safety.push(format!(
                        "{}:{} needs a // SAFETY: comment: `{}`",
                        path.display(),
                        idx + 1,
                        line.trim()
                    ));
                }
            }
        }
        // (c) safe-seam: delegate to the shared OS-neutral scanner so the
        // synthetic Win32 regression test below exercises the EXACT same
        // logic the boundary crates are held to (no drift between the gate
        // and its pin).
        for violation in scan_safe_seam_violations(source) {
            seam_leak.push(format!("{} {violation}", path.display()));
        }
    }
    assert!(
        missing_deny.is_empty(),
        "boundary crate roots missing #![deny(unsafe_op_in_unsafe_fn)]:\n{}",
        missing_deny.join("\n")
    );
    assert!(
        unsafe_without_safety.is_empty(),
        "boundary crate unsafe block/fn without a // SAFETY: comment:\n{}",
        unsafe_without_safety.join("\n")
    );
    assert!(
        seam_leak.is_empty(),
        "boundary crate leaked a raw handle/pointer across the safe seam:\n{}",
        seam_leak.join("\n")
    );
}

#[test]
fn safe_seam_scan_is_os_neutral_and_catches_win32_handle_and_pcwstr_leaks() {
    // P0 regression for the dependency_firewall HANDLE/PCWSTR blind spot
    // (CROSSPLATFORM_STRATEGY.md §5.1): before the widening the seam scan only
    // knew the Unix dimension (`as RawFd` + `*const`/`*mut` + the RawFd/AsRawFd
    // public-API tokens), so a hypothetical Win32 boundary crate could pass
    // "audited + safe seam" while leaking `HANDLE`/`PCWSTR`/`SOCKET` across its
    // public API. This pins the OS-neutral behavior of `scan_safe_seam_violations`
    // against future regressions, in BOTH directions:
    //   (a) every Win32 type leak on the public API is caught,
    //   (b) every Win32 cast is caught,
    //   (c) the Unix constant `libc::SOL_SOCKET` does NOT trip the Win32
    //       `SOCKET` rule (identifier-boundary matching — the false positive
    //       that would otherwise block a legitimate Unix boundary crate), and
    //   (d) the existing Unix dimension still fires (no RawFd regression).
    // The fragment is an in-test string — NO real Win32 code or dependency is
    // introduced; this is test-only governance.
    let synthetic_win32 = "\
pub fn take_handle(h: HANDLE) {}
pub fn multiline_handle(
    h: HANDLE,
) {}
pub fn return_pcwstr() -> PCWSTR { std::ptr::null() }
pub fn take_pcstr(s: PCSTR) {}
pub fn take_pwstring(p: PWSTR) {}
pub fn take_bstr(b: BSTR) {}
pub fn return_socket() -> SOCKET { 0 }
pub fn c_void_ptr(p: *const c_void) {}
pub fn c_void_mut(p: *mut c_void) {}
fn private_cast(h: usize) { let _ = h as HANDLE; let _ = 0 as PCWSTR; let _ = 0 as SOCKET; }
pub const SOL_SOCKET_LEVEL: c_int = libc::SOL_SOCKET;
";
    let leaks = scan_safe_seam_violations(synthetic_win32);

    // (a) public-API type leaks — every Win32 raw-handle token is caught.
    for token in [
        "HANDLE",
        "PCWSTR",
        "PCSTR",
        "PWSTR",
        "BSTR",
        "SOCKET",
        "*const c_void",
        "*mut c_void",
    ] {
        assert!(
            leaks
                .iter()
                .any(|line| line.contains(&format!("public item leaks `{token}`"))),
            "expected seam scan to catch the public-API leak of `{token}`, got: {leaks:?}"
        );
    }
    assert!(
        leaks
            .iter()
            .any(|line| line.contains("public item leaks `HANDLE`") && line.contains("h: HANDLE")),
        "expected the multiline public signature to be checked, got: {leaks:?}"
    );

    // (b) Win32 casts — every audited cast form is caught (the `as SOCKET`
    // here lives in a private body, so only the cast scan — not the public-API
    // token scan — can catch it).
    for cast in ["as HANDLE", "as PCWSTR", "as SOCKET"] {
        assert!(
            leaks
                .iter()
                .any(|line| line.contains(&format!("raw cast `{cast}`"))),
            "expected seam scan to catch the cast `{cast}`, got: {leaks:?}"
        );
    }

    // (c) NO false positive: the Unix constant `libc::SOL_SOCKET` on a public
    // item must not be reported as a Win32 `SOCKET` leak. The only `SOCKET`
    // match must be the genuine one on `return_socket`, not the `SOL_SOCKET`
    // const line.
    let socket_leaks: Vec<_> = leaks
        .iter()
        .filter(|line| line.contains("public item leaks `SOCKET`"))
        .collect();
    assert_eq!(
        socket_leaks.len(),
        1,
        "expected exactly one genuine SOCKET leak (from return_socket), got: {socket_leaks:?}"
    );
    assert!(
        !socket_leaks[0].contains("SOL_SOCKET"),
        "scan false positive: the Unix constant SOL_SOCKET tripped the Win32 SOCKET rule"
    );

    // (d) the Unix dimension still fires — the widening must not weaken RawFd.
    let synthetic_unix = "\
pub fn leak(fd: RawFd) -> *const u8 { std::ptr::null() }
fn body() { let _ = 1 as RawFd; }
";
    let unix_leaks = scan_safe_seam_violations(synthetic_unix);
    assert!(
        unix_leaks
            .iter()
            .any(|line| line.contains("public item leaks `RawFd`")),
        "Unix dimension regressed: RawFd public-API leak no longer caught"
    );
    assert!(
        unix_leaks
            .iter()
            .any(|line| line.contains("raw cast `as RawFd`")),
        "Unix dimension regressed: `as RawFd` cast no longer caught"
    );
}

#[test]
fn audited_perf_boundary_crate_is_depended_on_only_by_the_linux_adapter_and_helper() {
    // ADR-022 / ADR-023 reverse firewall: the audited perf boundary crate (the
    // workspace's perf `unsafe` trust root) is a permitted dependency of exactly
    // TWO workspace crates — taskmanager-platform-linux (the unprivileged Linux
    // adapter, which probes the PMU and degrades typed when denied) and
    // taskmanager-privilege-helper (the privileged helper binary that performs
    // the actual perf read through the OS-native escalation prompt). No other
    // workspace crate may wire to the unsafe trust root. The boundary crate
    // itself has zero workspace dependencies (only libc).
    let repository = repository();
    let (dependents, unsanctioned) = unsanctioned_dependents(
        &repository,
        "taskmanager-perf-ioctl",
        &["taskmanager-platform-linux", "taskmanager-privilege-helper"],
    );
    assert!(
        unsanctioned.is_empty(),
        "taskmanager-perf-ioctl has unsanctioned dependents: {unsanctioned:?}"
    );
    assert_eq!(
        dependents,
        vec![
            "taskmanager-platform-linux".to_owned(),
            "taskmanager-privilege-helper".to_owned()
        ],
        "taskmanager-perf-ioctl must be reachable only from the Linux adapter and the privileged helper"
    );
    assert_boundary_crate_has_no_workspace_dependencies(
        &repository,
        "crates/taskmanager-perf-ioctl",
    );
}

#[test]
fn audited_afpacket_boundary_crate_is_depended_on_only_by_sanctioned_consumers() {
    // ADR-024 reverse firewall: the SECOND audited `unsafe` boundary crate
    // (`taskmanager-afpacket`, the AF_PACKET socket seam for per-process network
    // byte accounting) may be wired only from the sanctioned consumers — the
    // Linux adapter (the unprivileged capture + attribution loop, reaching the
    // seam via `PacketSource::open`/`from_owned_fd`) and the net launcher (the
    // privileged `open_packet_fd` side). This is a SUBSET check (not
    // exact-equality), so an unsanctioned crate wiring in still fails. Mirrors
    // the perf-ioctl reverse firewall above.
    assert_boundary_consumers_are_sanctioned(
        &repository(),
        "taskmanager-afpacket",
        &["taskmanager-platform-linux", "taskmanager-net-launcher"],
    );
}

#[test]
fn audited_fd_bridge_boundary_crate_is_depended_on_only_by_sanctioned_consumers() {
    // ADR-025 reverse firewall: the THIRD audited `unsafe` boundary crate
    // (`taskmanager-fd-bridge`, the SCM_RIGHTS sendmsg/recvmsg seam) may be wired
    // only from the sanctioned consumers — taskmanager-escalation (the
    // unprivileged recv side: invoke_net_launcher → recv_fd), the net launcher
    // (the privileged send side: send_fd), and the process-control helper
    // (the pinned pidfd it routes foreign signals through, 4dfe73ef). This is
    // a SUBSET check (not exact-equality), so the crate existing with zero/one
    // dependents passes and an unsanctioned crate wiring in fails. Mirrors the
    // afpacket firewall.
    assert_boundary_consumers_are_sanctioned(
        &repository(),
        "taskmanager-fd-bridge",
        &[
            "taskmanager-escalation",
            "taskmanager-net-launcher",
            "taskmanager-process-control-helper",
        ],
    );
}

#[test]
fn audited_windows_api_boundary_is_depended_on_only_by_windows_adapter() {
    // ADR-031 reverse firewall: native Windows ABI calls stay behind the
    // dedicated safe wrapper and are reachable only from the Windows adapter
    // and the process-control helper.
    // The wrapper exposes typed values/errors, never handles, pointers, or
    // UTF-16 buffers, so a second consumer would silently widen the trust root.
    assert_boundary_consumers_are_sanctioned(
        &repository(),
        "taskmanager-windows-api",
        &[
            "taskmanager-platform-windows",
            "taskmanager-process-control-helper",
        ],
    );
}

#[test]
fn windows_adapter_has_no_command_interpreter_telemetry_path() {
    // Windows production telemetry is native-first: mature safe crates first,
    // then a deliberately tiny audited boundary. A command interpreter must
    // never become a hidden fallback because it couples unrelated capability
    // lanes, is hard to bound semantically, and makes locale/error behavior
    // opaque. Keep this as a negative source gate so future enrichments cannot
    // silently reintroduce the old path.
    let repository = repository();
    let source_root = repository.join("crates/taskmanager-platform-windows/src");
    let sources = walk_rust_files(&source_root);
    assert!(
        !sources.is_empty(),
        "Windows adapter source tree is missing"
    );
    let forbidden = [
        "powershell",
        "pwsh",
        "get-counter",
        "get-ciminstance",
        "get-netadapter",
        "get-process",
        "get-winevent",
    ];
    let mut violations = Vec::new();
    for (path, source) in sources {
        let lower = source.to_ascii_lowercase();
        for token in forbidden {
            if lower.contains(token) {
                violations.push(format!("{} contains `{token}`", path.display()));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "Windows adapter reintroduced a command-interpreter telemetry path:\n{}",
        violations.join("\n")
    );
}
