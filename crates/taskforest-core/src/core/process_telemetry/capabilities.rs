//! Linux POSIX capability masks as a platform-neutral process-security fact.
//!
//! The Linux provider owns parsing `/proc/<pid>/status`; this module owns the
//! bit vocabulary, decoding and risk rules. Unknown high bits are ignored by
//! the decoder rather than being silently assigned a dangerous name.

use serde::{Deserialize, Serialize};

use crate::core::device_state::DeviceState;

/// Linux capability numbers currently published by the kernel interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LinuxCapability {
    Chown = 0,
    DacOverride = 1,
    DacReadSearch = 2,
    Fowner = 3,
    Fsetid = 4,
    Kill = 5,
    Setgid = 6,
    Setuid = 7,
    Setpcap = 8,
    LinuxImmutable = 9,
    NetBindService = 10,
    NetBroadcast = 11,
    NetAdmin = 12,
    NetRaw = 13,
    IpcLock = 14,
    IpcOwner = 15,
    SysModule = 16,
    SysRawio = 17,
    SysChroot = 18,
    SysPtrace = 19,
    SysPacct = 20,
    SysAdmin = 21,
    SysBoot = 22,
    SysNice = 23,
    SysResource = 24,
    SysTime = 25,
    SysTtyConfig = 26,
    Mknod = 27,
    Lease = 28,
    AuditWrite = 29,
    AuditControl = 30,
    Setfcap = 31,
    MacOverride = 32,
    MacAdmin = 33,
    Syslog = 34,
    WakeAlarm = 35,
    BlockSuspend = 36,
    AuditRead = 37,
    Perfmon = 38,
    Bpf = 39,
    CheckpointRestore = 40,
}

impl LinuxCapability {
    /// The capability's Linux spelling, including the `CAP_` prefix.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Chown => "CAP_CHOWN",
            Self::DacOverride => "CAP_DAC_OVERRIDE",
            Self::DacReadSearch => "CAP_DAC_READ_SEARCH",
            Self::Fowner => "CAP_FOWNER",
            Self::Fsetid => "CAP_FSETID",
            Self::Kill => "CAP_KILL",
            Self::Setgid => "CAP_SETGID",
            Self::Setuid => "CAP_SETUID",
            Self::Setpcap => "CAP_SETPCAP",
            Self::LinuxImmutable => "CAP_LINUX_IMMUTABLE",
            Self::NetBindService => "CAP_NET_BIND_SERVICE",
            Self::NetBroadcast => "CAP_NET_BROADCAST",
            Self::NetAdmin => "CAP_NET_ADMIN",
            Self::NetRaw => "CAP_NET_RAW",
            Self::IpcLock => "CAP_IPC_LOCK",
            Self::IpcOwner => "CAP_IPC_OWNER",
            Self::SysModule => "CAP_SYS_MODULE",
            Self::SysRawio => "CAP_SYS_RAWIO",
            Self::SysChroot => "CAP_SYS_CHROOT",
            Self::SysPtrace => "CAP_SYS_PTRACE",
            Self::SysPacct => "CAP_SYS_PACCT",
            Self::SysAdmin => "CAP_SYS_ADMIN",
            Self::SysBoot => "CAP_SYS_BOOT",
            Self::SysNice => "CAP_SYS_NICE",
            Self::SysResource => "CAP_SYS_RESOURCE",
            Self::SysTime => "CAP_SYS_TIME",
            Self::SysTtyConfig => "CAP_SYS_TTY_CONFIG",
            Self::Mknod => "CAP_MKNOD",
            Self::Lease => "CAP_LEASE",
            Self::AuditWrite => "CAP_AUDIT_WRITE",
            Self::AuditControl => "CAP_AUDIT_CONTROL",
            Self::Setfcap => "CAP_SETFCAP",
            Self::MacOverride => "CAP_MAC_OVERRIDE",
            Self::MacAdmin => "CAP_MAC_ADMIN",
            Self::Syslog => "CAP_SYSLOG",
            Self::WakeAlarm => "CAP_WAKE_ALARM",
            Self::BlockSuspend => "CAP_BLOCK_SUSPEND",
            Self::AuditRead => "CAP_AUDIT_READ",
            Self::Perfmon => "CAP_PERFMON",
            Self::Bpf => "CAP_BPF",
            Self::CheckpointRestore => "CAP_CHECKPOINT_RESTORE",
        }
    }

    /// Decode a single 64-bit kernel capability mask in numeric order.
    #[must_use]
    pub fn decode_mask(mask: u64) -> Vec<Self> {
        Self::ALL
            .into_iter()
            .filter(|capability| mask & (1_u64 << (*capability as u8)) != 0)
            .collect()
    }

    /// True when the capability grants a high-impact privilege boundary.
    #[must_use]
    pub const fn is_critical(self) -> bool {
        matches!(
            self,
            Self::DacOverride
                | Self::NetAdmin
                | Self::SysRawio
                | Self::SysPtrace
                | Self::SysAdmin
                | Self::Bpf
                | Self::Perfmon
        )
    }

    /// All known capability numbers, in kernel order.
    pub const ALL: [Self; 41] = [
        Self::Chown,
        Self::DacOverride,
        Self::DacReadSearch,
        Self::Fowner,
        Self::Fsetid,
        Self::Kill,
        Self::Setgid,
        Self::Setuid,
        Self::Setpcap,
        Self::LinuxImmutable,
        Self::NetBindService,
        Self::NetBroadcast,
        Self::NetAdmin,
        Self::NetRaw,
        Self::IpcLock,
        Self::IpcOwner,
        Self::SysModule,
        Self::SysRawio,
        Self::SysChroot,
        Self::SysPtrace,
        Self::SysPacct,
        Self::SysAdmin,
        Self::SysBoot,
        Self::SysNice,
        Self::SysResource,
        Self::SysTime,
        Self::SysTtyConfig,
        Self::Mknod,
        Self::Lease,
        Self::AuditWrite,
        Self::AuditControl,
        Self::Setfcap,
        Self::MacOverride,
        Self::MacAdmin,
        Self::Syslog,
        Self::WakeAlarm,
        Self::BlockSuspend,
        Self::AuditRead,
        Self::Perfmon,
        Self::Bpf,
        Self::CheckpointRestore,
    ];
}

/// Risk summary for a process's effective/permitted capability set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityRiskLevel {
    #[default]
    Unknown,
    None,
    Elevated,
    Critical,
}

impl CapabilityRiskLevel {
    /// Stable neutral label used by renderer adapters.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::None => "none",
            Self::Elevated => "elevated",
            Self::Critical => "critical",
        }
    }
}

/// Typed capability masks and their observation health.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ProcessCapabilities {
    pub state: DeviceState,
    pub inheritable: Option<u64>,
    pub permitted: Option<u64>,
    pub effective: Option<u64>,
    pub bounding: Option<u64>,
    pub ambient: Option<u64>,
}

impl ProcessCapabilities {
    /// Build masks read from one procfs status file.
    #[must_use]
    pub const fn from_masks(
        state: DeviceState,
        inheritable: Option<u64>,
        permitted: Option<u64>,
        effective: Option<u64>,
        bounding: Option<u64>,
        ambient: Option<u64>,
    ) -> Self {
        Self {
            state,
            inheritable,
            permitted,
            effective,
            bounding,
            ambient,
        }
    }

    /// Decode effective capabilities. A missing effective mask returns no
    /// list, preserving the distinction between empty and unreadable.
    #[must_use]
    pub fn effective_capabilities(&self) -> Option<Vec<LinuxCapability>> {
        self.effective.map(LinuxCapability::decode_mask)
    }

    /// Decode permitted capabilities.
    #[must_use]
    pub fn permitted_capabilities(&self) -> Option<Vec<LinuxCapability>> {
        self.permitted.map(LinuxCapability::decode_mask)
    }

    /// Return only effective critical capabilities.
    #[must_use]
    pub fn critical_effective_capabilities(&self) -> Option<Vec<LinuxCapability>> {
        self.effective_capabilities().map(|items| {
            items
                .into_iter()
                .filter(|item| item.is_critical())
                .collect()
        })
    }

    /// Compute risk from effective first, then permitted privileges.
    #[must_use]
    pub fn risk_level(&self) -> CapabilityRiskLevel {
        let Some(effective) = self.effective else {
            return CapabilityRiskLevel::Unknown;
        };
        let permitted = self.permitted.unwrap_or(0);
        let critical_mask = Self::critical_mask();
        if effective & critical_mask != 0 {
            CapabilityRiskLevel::Critical
        } else if permitted & critical_mask != 0 || effective != 0 {
            CapabilityRiskLevel::Elevated
        } else {
            CapabilityRiskLevel::None
        }
    }

    /// The known critical-bit mask used by the risk fold.
    #[must_use]
    pub const fn critical_mask() -> u64 {
        (1_u64 << LinuxCapability::DacOverride as u8)
            | (1_u64 << LinuxCapability::NetAdmin as u8)
            | (1_u64 << LinuxCapability::SysRawio as u8)
            | (1_u64 << LinuxCapability::SysPtrace as u8)
            | (1_u64 << LinuxCapability::SysAdmin as u8)
            | (1_u64 << LinuxCapability::Bpf as u8)
            | (1_u64 << LinuxCapability::Perfmon as u8)
    }
}
