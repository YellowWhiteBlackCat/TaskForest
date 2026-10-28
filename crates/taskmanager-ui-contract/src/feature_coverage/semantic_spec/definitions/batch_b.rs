//! Semantic delivery definitions for Areas 6-10 (batch B).

use crate::feature_coverage::FeatureId;

#[must_use]
pub const fn delivery_definition(feature: FeatureId) -> Option<&'static str> {
    let def = match feature {
        FeatureId::ProcessLogicalPhysicalIo => {
            "A per-process storage surface renders logical versus \
                                      physical I/O accounting from the shared projection."
        }
        FeatureId::DiskDeviceTopology => {
            "A storage surface renders the system storage device and \
                                      partition topology from the shared projection."
        }
        FeatureId::DiskIopsQueueLatency => {
            "The storage surface renders per-disk IOPS, queue depth, \
                                      and latency from the shared projection."
        }
        FeatureId::ProcessIoPriority => {
            "A per-process storage surface renders the observed I/O \
                                      scheduling class and priority level (such as real-time, \
                                      best-effort, or idle) and offers typed I/O priority adjustment."
        }
        FeatureId::StorageFilesystemMountAudit => {
            "A storage health surface renders filesystem mount audit \
                                      entries (mount point, filesystem type, read-only status, \
                                      and integrity status) from the shared storage health \
                                      projection."
        }
        FeatureId::DirtyWritebackFlow => {
            "A storage or memory diagnostics surface renders observed \
                                      dirty page and writeback memory quantities (such as dirty and \
                                      writeback cache pressure), keeping unsupported platforms as an \
                                      honest dash."
        }
        FeatureId::DiskSmartHealth => {
            "A storage surface renders each disk's SMART health \
                                      evidence - reported availability, temperature, percentage \
                                      used, available spare, power-on hours, unsafe shutdowns - \
                                      from the shared device projection."
        }
        FeatureId::StorageSpaceDepletionCountdown => {
            "A storage monitoring surface renders intelligent space and Inode \
                                      exhaustion countdown projections based on consumption rates."
        }
        FeatureId::StorageIoStallDiagnosis => {
            "A storage performance surface detects and diagnoses disk I/O \
                                      stalls, sync hangs, and fsync latency spikes from the \
                                      shared storage projection."
        }
        FeatureId::SwapThroughputRate => {
            "A memory surface renders the system swap-in and swap-out \
                                      throughput rates from the shared memory projection."
        }
        FeatureId::StorageTopAccessedFiles => {
            "A storage forensics surface enumerates and ranks top accessed \
                                      files by read/write activity and throughput across system \
                                      processes."
        }
        FeatureId::VirtualMemoryFsCapacity => {
            "A storage filesystem surface identifies in-memory virtual \
                                      filesystems (such as tmpfs and ramfs) and renders their capacity \
                                      and backing classification from observed mount health."
        }
        FeatureId::StorageTrimDiscardSupport => {
            "A storage inspection surface renders storage device TRIM / \
                                      Discard reclamation capability and health attributes, \
                                      distinguishing supported SSD/NVMe media from unsupported devices."
        }
        FeatureId::RaidPoolTopology => {
            "A storage topology surface renders multi-disk array \
                                      (RAID/LVM/ZFS/Btrfs) pool structure, member disk status, \
                                      rebuild percentage, and degraded flags."
        }
        FeatureId::DiskReadOnlyFailureAlert => {
            "A storage health monitoring surface alerts on emergency \
                                      read-only filesystem remounts and underlying medium I/O \
                                      write failures from filesystem health telemetry."
        }
        FeatureId::HardwareTopologyTree => {
            "A hardware surface renders the socket -> NUMA -> core -> \
                                      thread topology tree from the shared projection."
        }
        FeatureId::CpuCacheTopology => {
            "A CPU surface renders the multi-level cache capacity \
                                      readout from the shared CPU metrics projection: the \
                                      observed L1d (`l1d_cache_kb`), L1i (`l1i_cache_kb`), \
                                      L2 (`l2_cache_kb`), and L3 (`l3_cache_kb`) capacities. \
                                      Cache-sharing topology between cores or NUMA nodes is \
                                      explicitly OUTSIDE this delivery definition: rendering \
                                      only the capacities counts as delivered, and an \
                                      unobserved capacity is an honest absence, never a zero."
        }
        FeatureId::NumaMemoryDistribution => {
            "A hardware surface renders the per-NUMA-node memory \
                                      distribution and locality from the shared hardware \
                                      projection: each node's local memory capacity \
                                      (`local_memory_bytes`) and locality/hit ratio \
                                      (`numa_hit_ratio_pct`), keyed by node identity \
                                      (`numa_node_id`/`numa_node_ids`). A package-level or \
                                      aggregate memory total does NOT satisfy this definition; \
                                      per-node distribution is required."
        }
        FeatureId::ProcessNumaMemoryPolicy => {
            "A memory and processor affinity surface renders and configures \
                                      per-process NUMA memory allocation policies \
                                      (MPOL_DEFAULT/BIND/INTERLEAVE/PREFERRED) and node bindings."
        }
        FeatureId::NumaThrashingAlert => {
            "A NUMA diagnostics surface detects remote memory access \
                                      anomalies and warns when excessive cross-node memory \
                                      access causes interconnect bus contention."
        }
        FeatureId::PcieLinkSpeedDegradation => {
            "A hardware bus topology surface detects negotiated PCIe \
                                      link speed and width degradation against maximum \
                                      device lane capabilities."
        }
        FeatureId::CpuHeterogeneousCoreClass => {
            "A CPU surface renders the heterogeneous core-class \
                                      breakdown (performance / efficiency / low-power core \
                                      counts) from the shared hardware projection."
        }
        FeatureId::CpuCoreFrequency => {
            "A CPU surface renders the observed per-core clock frequency \
                                      from the shared CPU metrics projection."
        }
        FeatureId::CpuThermalBoostStatus => {
            "A processor performance surface renders hardware thermal \
                                      velocity boost, precision boost, and opportunistic \
                                      overdrive active acceleration states."
        }
        FeatureId::CpuInstructionFeatures => {
            "A system hardware surface renders the detected CPU \
                                      instruction-set features and SIMD extensions from the \
                                      shared hardware projection, omitting the row when no \
                                      source reported features."
        }
        FeatureId::SmbiosMemorySlots => {
            "A system memory inventory surface renders the populated and \
                                      empty SMBIOS/DMI memory slots from the privileged memory \
                                      snapshot, keeping unprivileged reads as an honest absence."
        }
        FeatureId::MemoryChannelSymmetry => {
            "A hardware topology surface renders memory channel \
                                      configuration (such as ChannelA and ChannelB slot assignments) \
                                      and verifies multi-channel population from SMBIOS inventory."
        }
        FeatureId::HeterogeneousInterconnectArchitecture => {
            "A hardware architecture surface identifies unified memory \
                                      architectures (UMA) and heterogeneous CPU-GPU-NPU \
                                      interconnect topologies."
        }
        FeatureId::FirmwareSecureBoot => {
            "A system inventory surface renders motherboard chipset, \
                                      firmware vendor/version, and Secure Boot enablement state."
        }
        FeatureId::CpuInterruptAffinityBalance => {
            "A processor diagnostic surface renders per-core hardware \
                                      IRQ and softirq interrupt load distribution matrices across \
                                      system logical cores."
        }
        FeatureId::GpuAdapterEnumeration => {
            "An accelerator surface enumerates the dGPU/iGPU adapters \
                                      from the shared projection."
        }
        FeatureId::NpuTelemetry => {
            "An accelerator surface renders native NPU inventory and \
                                      utilization telemetry from the shared projection."
        }
        FeatureId::GpuEngineUtilization => {
            "The accelerator surface renders the per-engine GPU \
                                      utilization breakdown from the shared projection."
        }
        FeatureId::GpuMemoryReadout => {
            "An accelerator surface renders the shared per-device GPU \
                                      memory usage (observed used bytes against the reported \
                                      total) from the device projection. The dedicated-versus-\
                                      shared VRAM partition is explicitly OUTSIDE this \
                                      definition."
        }
        FeatureId::ProcessGpuAttribution => {
            "A process surface attributes GPU memory and engine usage to \
                                      the owning process from the shared per-process GPU \
                                      projection."
        }
        FeatureId::GpuTemperatureReadout => {
            "An accelerator surface renders the observed GPU core or die \
                                      temperature from the shared GPU telemetry projection, \
                                      keeping unobserved devices as an honest dash."
        }
        FeatureId::GpuPowerDraw => {
            "An accelerator surface renders the observed GPU power draw \
                                      in watts from the shared GPU telemetry projection, keeping \
                                      unobserved devices as an honest dash."
        }
        FeatureId::GpuCoreClockFrequency => {
            "An accelerator surface renders the observed GPU core clock \
                                      frequency in MHz from the shared GPU telemetry projection."
        }
        FeatureId::GpuThrottleReasons => {
            "An accelerator surface renders active GPU performance \
                                      limiters and throttle reasons, omitting when unthrottled."
        }
        FeatureId::GpuCodecUtilization => {
            "An accelerator surface renders dedicated video hardware \
                                      encoder/decoder utilization and processing throughput \
                                      from the shared GPU telemetry projection."
        }
        FeatureId::GpuProcessClients => {
            "An accelerator details surface enumerates active client \
                                      processes, GPU context handles, and attributed VRAM usage \
                                      for the selected graphics adapter."
        }
        FeatureId::NpuInferenceTelemetry => {
            "An accelerator surface renders NPU tensor workflow execution \
                                      queue depth and average inference latency readouts from \
                                      the shared accelerator projection."
        }
        FeatureId::GpuLinkTopology => {
            "An accelerator topology surface renders multi-GPU \
                                      interconnect bridge bandwidth and link status (NVLink, \
                                      CrossFire, PCIe P2P) from the shared GPU projection."
        }
        FeatureId::GpuFirmwareApiRuntime => {
            "An accelerator surface renders GPU driver version, VBIOS \
                                      version, and runtime graphics API versions (OpenGL/Vulkan)."
        }
        FeatureId::GpuResetTdrLogs => {
            "An accelerator reliability surface renders GPU hang events, \
                                      driver timeout detection and recovery (TDR) counts, and \
                                      engine reset occurrences from kernel error telemetry."
        }
        FeatureId::RaplPowerDraw => {
            "A power surface renders the Intel/AMD RAPL package power \
                                      draw from the shared projection."
        }
        FeatureId::EnergyImpactScore => {
            "A process telemetry surface computes and displays a \
                                      relative energy impact score reflecting processor, \
                                      graphics, and background timer wakeup activity per process."
        }
        FeatureId::ThermalZoneSensors => {
            "A thermal surface traverses the system thermal-zone sensors \
                                      and names each reading's source from the shared projection."
        }
        FeatureId::CpuCStateAnalysis => {
            "A CPU surface renders the cpuidle C-state residency/usage \
                                      evidence from the shared CPU projection."
        }
        FeatureId::BatteryPowerInventory => {
            "A power surface renders the power-supply inventory with \
                                      each battery's charge, charge/discharge power, voltage, \
                                      health, and cycle count from the shared power-supply \
                                      projection; an unobserved field stays an honest absence."
        }
        FeatureId::BatteryRemainingTimeEstimate => {
            "A power surface renders the battery remaining time to \
                                      empty or time to full estimate from the shared \
                                      power-supply projection, keeping unestimated or AC-only \
                                      states as an honest dash."
        }
        FeatureId::ThermalThrottleEvents => {
            "A thermal surface renders the CPU package \
                                      thermal-throttle trigger counters (the cumulative \
                                      package and per-core event counts, \
                                      `package_throttle_count` / `core_throttle_count`) \
                                      from the shared CPU projection, together with the \
                                      real-time thermal-status / PROCHOT assertion \
                                      (`MsrThermalStatusReadout::is_throttled`, the \
                                      `IA32_THERM_STATUS` (MSR 0x19C) bit 0) from the \
                                      privileged `telemetry.cpu.msr` lane (Linux-only via \
                                      the root-only `/dev/cpu/N/msr` nodes; \
                                      Windows/macOS answer a typed `Unsupported`). An \
                                      unreadable or unimplemented register is an honest \
                                      dash, and a lane that never ran is an absent fact, \
                                      never a fabricated clear state. The cumulative \
                                      counters stay on the unprivileged \
                                      `telemetry.cpu.throttle` lane."
        }
        FeatureId::FanSpeedTelemetry => {
            "A thermal or system health surface renders chassis and \
                                      cooling fan speeds in RPM from the shared sensor \
                                      projection, omitting fan channels from pure thermal-zone \
                                      views."
        }
        FeatureId::CpuEnergyPerformancePreference => {
            "A processor performance surface renders active energy \
                                      performance preference (EPP) bias profiles and CPU \
                                      scaling governor hints from CPU telemetry."
        }
        FeatureId::PowerProfilesDaemon => {
            "A power telemetry surface renders the active system power \
                                      profile (e.g. performance, balanced, power-saver) and exposes \
                                      profile switching through the platform power profiles integration."
        }
        FeatureId::ProcessWakeupFrequency => {
            "A power optimization surface renders per-process timer \
                                      expiration and wakeup frequency telemetry (wakeups/second) \
                                      from the shared CPU projection."
        }
        FeatureId::SleepInhibitLocks => {
            "A power governance surface detects and displays \
                                      sleep/suspend inhibitor locks and power assertions attributing \
                                      blockers to active processes."
        }
        FeatureId::DisplayBacklightPower => {
            "A power telemetry surface correlates display panel brightness \
                                      levels, refresh rates, and estimated backlight power consumption."
        }
        FeatureId::UsbDeviceRuntimePm => {
            "A power diagnostic surface audits USB and external bus \
                                      device runtime power management (Runtime PM) autosuspend states."
        }
        FeatureId::SessionEnergyCostCalculation => {
            "A power telemetry surface calculates cumulative electrical \
                                      energy consumption (in kWh) and estimated monetary cost for \
                                      the current session from hardware power integration."
        }
        FeatureId::LinuxNamespaceAudit => {
            "A security surface renders the Linux namespace audit \
                                      (per-kind host/isolated inode status) from the shared \
                                      projection."
        }
        FeatureId::PosixCapabilitiesAudit => {
            "The security surface decodes the POSIX capability masks \
                                      and flags dangerous capabilities from the shared projection."
        }
        FeatureId::MandatoryAccessControlLabels => {
            "A security isolation surface decodes and renders mandatory \
                                      access control (MAC) policy labels (such as SELinux context \
                                      or AppArmor profile) for the observed process."
        }
        FeatureId::SeccompFilterAudit => {
            "The security surface renders the seccomp syscall-filter \
                                      audit from the shared projection."
        }
        FeatureId::LandlockSandboxAudit => {
            "A security isolation surface audits active Linux Landlock \
                                      sandbox rulesets, path restrictions, and ABI capability \
                                      levels for inspected processes."
        }
        FeatureId::SecurityNoNewPrivs => {
            "A security isolation surface renders the observed \
                                      no_new_privs bit from the shared process isolation \
                                      projection, keeping unobserved platforms as the shared \
                                      dash."
        }
        FeatureId::ProcessOwnerCredentials => {
            "A process surface renders the owning user or UID \
                                      credentials for each process from canonical metadata."
        }
        FeatureId::BinarySignatureVerification => {
            "A security forensics surface verifies executable binary \
                                      digital signatures, certificate chains, and trust authority \
                                      status across operating systems."
        }
        FeatureId::CompilerHardeningAudit => {
            "A process security inspection surface decodes and renders \
                                      binary compiler hardening mitigations (such as PIE, stack canary, \
                                      RELRO, and NX protection) for the observed process."
        }
        FeatureId::WindowsIntegrityLevels => {
            "A security isolation surface decodes Windows token integrity \
                                      levels (Untrusted, Low, Medium, High, System) and \
                                      AppContainer sandbox boundaries."
        }
        FeatureId::MacosTccPermissions => {
            "A security privacy surface enumerates macOS process \
                                      entitlements and Transparency, Consent & Control (TCC) \
                                      hardware and filesystem permissions."
        }
        FeatureId::PrivilegeAnomalyDetection => {
            "A security heuristics surface alerts on dangerous capability \
                                      escalations, raw disk I/O, or abnormal root-delegated \
                                      process privileges."
        }
        FeatureId::CodeInjectionDetection => {
            "A security forensics surface scans process virtual memory \
                                      maps to detect anomalous anonymous executable pages (rwx memory) \
                                      and DLL injection artifacts."
        }
        FeatureId::SandboxEnvironmentDetection => {
            "A process surface renders the detected sandbox/container \
                                      identity (typed isolation kind plus container id) for the \
                                      selected process from the shared isolation projection."
        }
        FeatureId::ProcessMasqueradingDetection => {
            "The process surface renders the executable-name versus \
                                      command-line identity mismatch warning from the shared \
                                      command-identity comparison."
        }
        _ => return None,
    };
    Some(def)
}
