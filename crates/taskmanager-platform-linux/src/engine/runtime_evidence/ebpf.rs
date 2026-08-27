use super::EbpfObjectBuildIdentity;

/// The pure-safe-Rust build embeds no eBPF object, so build identity is always
/// absent. Kept as a seam so a future audited safe wrapper can supply it.
pub(super) const fn compiled_object_identity(_compiled: bool) -> Option<EbpfObjectBuildIdentity> {
    None
}

#[cfg(target_os = "linux")]
pub(super) fn effective_privilege() -> bool {
    if nix::unistd::Uid::effective().is_root() {
        return true;
    }
    has_capability_set(effective_capability_mask())
}

/// Whether this process can open the Intel i915/xe PMU without an OS-native
/// escalation. `CAP_PERFMON` is the direct capability; `CAP_SYS_ADMIN` is
/// retained for kernels that still use it as the perf permission override.
#[cfg(target_os = "linux")]
pub(super) fn effective_perfmon_privilege() -> bool {
    if nix::unistd::Uid::effective().is_root() {
        return true;
    }
    has_perfmon_capability(effective_capability_mask())
}

#[cfg(target_os = "linux")]
fn effective_capability_mask() -> Option<u64> {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find_map(|line| line.strip_prefix("CapEff:"))
                .and_then(|value| u64::from_str_radix(value.trim(), 16).ok())
        })
}

pub(super) const fn has_capability_set(cap_eff: Option<u64>) -> bool {
    const CAP_SYS_ADMIN: u32 = 21;
    const CAP_PERFMON: u32 = 38;
    const CAP_BPF: u32 = 39;
    let Some(cap_eff) = cap_eff else {
        return false;
    };
    let has_sys_admin = cap_eff & (1_u64 << CAP_SYS_ADMIN) != 0;
    let has_perfmon = cap_eff & (1_u64 << CAP_PERFMON) != 0;
    let has_bpf = cap_eff & (1_u64 << CAP_BPF) != 0;
    has_sys_admin || has_bpf && has_perfmon
}

pub(super) const fn has_perfmon_capability(cap_eff: Option<u64>) -> bool {
    const CAP_SYS_ADMIN: u32 = 21;
    const CAP_PERFMON: u32 = 38;
    let Some(cap_eff) = cap_eff else {
        return false;
    };
    cap_eff & (1_u64 << CAP_SYS_ADMIN) != 0 || cap_eff & (1_u64 << CAP_PERFMON) != 0
}
