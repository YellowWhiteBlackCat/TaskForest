//! The 225-slot blueprint disposition census (ADR-059).
//!
//! [`FeatureId`] is the typed delivery registry: a feature enters it only when
//! it has a semantic specification, a platform binding, and four explicit
//! frontend decisions. This module owns the complete product-roadmap census
//! those typed features sit inside: every one of the 225 blueprint slots is
//! either adopted by exactly one [`FeatureId`] or explicitly unsupported with
//! a public reason. "Not registered" can therefore never hide a product item.
//!
//! The committed TSV is public by construction: slot, area, disposition,
//! feature id and a public reason. The private blueprint titles and mechanisms
//! stay in `.private/`; this table only proves that every slot was dispositioned
//! and that adopted slots resolve to a typed feature in the same area.
//!
//! An item is promoted by adding its [`FeatureId`] and flipping its slot to
//! `Adopted` in the same change. Declaring `Unsupported` here is not a proof of
//! delivery and never becomes a `Ready` claim.

use std::sync::LazyLock;

use super::{FeatureArea, FeatureId};

const BLUEPRINT_DISPOSITIONS_TSV: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../scripts/parity/blueprint_dispositions.tsv"
));

/// The exact column header of the committed disposition table.
const HEADER: &str = "slot\tarea\tdisposition\tfeature\treason";
/// Every blueprint area carries this many slots.
const ITEMS_PER_AREA: usize = 15;
/// The complete blueprint census.
const SLOT_COUNT: usize = 225;
/// The "no value" marker used by the unused feature/reason column.
const DASH: &str = "-";

/// What the product census says about one blueprint slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BlueprintDisposition {
    /// The slot is committed and delivered/typed as this feature.
    Adopted(FeatureId),
    /// The slot is not delivered in the current product contract. The row's
    /// reason states that honestly; it is not a `Ready` claim.
    Unsupported,
}

/// One parsed disposition row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlueprintDispositionRow {
    /// 1-based blueprint slot (`1..=225`).
    pub slot: u8,
    /// The blueprint area the slot belongs to.
    pub area: FeatureArea,
    /// The explicit product disposition.
    pub disposition: BlueprintDisposition,
    /// Public reason for an unsupported slot; `-` for an adopted slot.
    pub reason: &'static str,
    /// 1-based line in the committed table, for diagnostics.
    pub line: usize,
}

/// A structural defect in the disposition census.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlueprintDispositionFinding {
    /// The file has no column header.
    MissingHeader,
    /// The first non-comment line is not the exact header.
    MalformedHeader { line: usize },
    /// The row does not have exactly five tab-separated fields.
    MalformedRow { line: usize },
    /// The slot is not an integer in `1..=225`.
    UnknownSlot { line: usize },
    /// Two rows claim the same slot.
    DuplicateSlot { slot: u8, line: usize },
    /// The area id is not a known [`FeatureArea`].
    UnknownArea { line: usize },
    /// The row's area does not match the slot's area.
    AreaMismatch { slot: u8, line: usize },
    /// The disposition token is neither `adopted` nor `unsupported`.
    UnknownDisposition { line: usize },
    /// An adopted row's feature id is not a known [`FeatureId`].
    UnknownFeature { line: usize },
    /// An adopted row does not name a feature.
    AdoptedWithoutFeature { slot: u8, line: usize },
    /// An unsupported row names a feature.
    UnsupportedWithFeature { slot: u8, line: usize },
    /// An adopted row carries a reason.
    AdoptedWithReason { slot: u8, line: usize },
    /// An unsupported row carries no public reason.
    MissingReason { slot: u8, line: usize },
    /// The adopted feature files under a different area.
    FeatureAreaMismatch {
        slot: u8,
        feature: FeatureId,
        line: usize,
    },
    /// Two slots adopt the same feature.
    DuplicateAdoptedFeature { feature: FeatureId, line: usize },
    /// The table has no data rows.
    EmptyTable,
    /// A slot in `1..=225` has no row.
    MissingSlot { slot: u8 },
}

/// The parsed 225-slot census.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BlueprintDispositionTable {
    rows: Vec<BlueprintDispositionRow>,
    findings: Vec<BlueprintDispositionFinding>,
}

impl BlueprintDispositionTable {
    /// Parse the committed table. Never panics: every malformed shape becomes
    /// a finding and the gate stays fail-closed.
    #[must_use]
    pub fn parse(text: &'static str) -> Self {
        let mut table = Self::default();
        let mut seen = [false; SLOT_COUNT];
        let mut adopted = Vec::new();
        let mut header_seen = false;

        for (index, raw) in text.lines().enumerate() {
            let line = index + 1;
            let trimmed = raw.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if !header_seen {
                header_seen = true;
                if raw.trim_end() != HEADER {
                    table
                        .findings
                        .push(BlueprintDispositionFinding::MalformedHeader { line });
                }
                continue;
            }
            let fields: Vec<&'static str> = raw.split('\t').map(str::trim).collect();
            if fields.len() != 5 {
                table
                    .findings
                    .push(BlueprintDispositionFinding::MalformedRow { line });
                continue;
            }
            let Ok(slot) = fields[0].parse::<u8>() else {
                table
                    .findings
                    .push(BlueprintDispositionFinding::UnknownSlot { line });
                continue;
            };
            let Some(slot_index) = usize::from(slot).checked_sub(1) else {
                table
                    .findings
                    .push(BlueprintDispositionFinding::UnknownSlot { line });
                continue;
            };
            if slot_index >= SLOT_COUNT {
                table
                    .findings
                    .push(BlueprintDispositionFinding::UnknownSlot { line });
                continue;
            }
            let expected_area = FeatureArea::ALL[slot_index / ITEMS_PER_AREA];
            let Some(area) = FeatureArea::ALL
                .iter()
                .copied()
                .find(|area| area.id() == fields[1])
            else {
                table
                    .findings
                    .push(BlueprintDispositionFinding::UnknownArea { line });
                continue;
            };
            if area != expected_area {
                table
                    .findings
                    .push(BlueprintDispositionFinding::AreaMismatch { slot, line });
                continue;
            }
            if seen[slot_index] {
                table
                    .findings
                    .push(BlueprintDispositionFinding::DuplicateSlot { slot, line });
                continue;
            }
            seen[slot_index] = true;

            match fields[2] {
                "adopted" => {
                    if fields[3] == DASH {
                        table
                            .findings
                            .push(BlueprintDispositionFinding::AdoptedWithoutFeature {
                                slot,
                                line,
                            });
                        continue;
                    }
                    let feature = FeatureId::ALL
                        .iter()
                        .copied()
                        .find(|feature| feature.id() == fields[3]);
                    let Some(feature) = feature else {
                        table
                            .findings
                            .push(BlueprintDispositionFinding::UnknownFeature { line });
                        continue;
                    };
                    if feature.area() != area {
                        table
                            .findings
                            .push(BlueprintDispositionFinding::FeatureAreaMismatch {
                                slot,
                                feature,
                                line,
                            });
                        continue;
                    }
                    if adopted.contains(&feature)
                        && !feature.blueprint_alias_slots().contains(&slot)
                    {
                        table
                            .findings
                            .push(BlueprintDispositionFinding::DuplicateAdoptedFeature {
                                feature,
                                line,
                            });
                        continue;
                    }
                    if fields[4] != DASH {
                        table
                            .findings
                            .push(BlueprintDispositionFinding::AdoptedWithReason { slot, line });
                        continue;
                    }
                    adopted.push(feature);
                    table.rows.push(BlueprintDispositionRow {
                        slot,
                        area,
                        disposition: BlueprintDisposition::Adopted(feature),
                        reason: DASH,
                        line,
                    });
                }
                "unsupported" => {
                    if fields[3] != DASH {
                        table
                            .findings
                            .push(BlueprintDispositionFinding::UnsupportedWithFeature {
                                slot,
                                line,
                            });
                        continue;
                    }
                    if fields[4].is_empty() || fields[4] == DASH {
                        table
                            .findings
                            .push(BlueprintDispositionFinding::MissingReason { slot, line });
                        continue;
                    }
                    table.rows.push(BlueprintDispositionRow {
                        slot,
                        area,
                        disposition: BlueprintDisposition::Unsupported,
                        reason: fields[4],
                        line,
                    });
                }
                _ => table
                    .findings
                    .push(BlueprintDispositionFinding::UnknownDisposition { line }),
            }
        }

        if !header_seen {
            table
                .findings
                .push(BlueprintDispositionFinding::MissingHeader);
        }
        if table.rows.is_empty() {
            table.findings.push(BlueprintDispositionFinding::EmptyTable);
        }
        for slot in 1..=SLOT_COUNT as u8 {
            if !seen[usize::from(slot) - 1] {
                table
                    .findings
                    .push(BlueprintDispositionFinding::MissingSlot { slot });
            }
        }
        table.rows.sort_by_key(|row| row.slot);
        table
    }

    /// The committed census.
    #[must_use]
    pub fn committed() -> &'static Self {
        static COMMITTED: LazyLock<BlueprintDispositionTable> =
            LazyLock::new(|| BlueprintDispositionTable::parse(BLUEPRINT_DISPOSITIONS_TSV));
        &COMMITTED
    }

    /// Every dispositioned row, ordered by slot.
    #[must_use]
    pub fn rows(&self) -> &[BlueprintDispositionRow] {
        &self.rows
    }

    /// Every structural defect; empty is the only passing state.
    #[must_use]
    pub fn findings(&self) -> &[BlueprintDispositionFinding] {
        &self.findings
    }

    /// The row for one slot.
    #[must_use]
    pub fn slot(&self, slot: u8) -> Option<&BlueprintDispositionRow> {
        self.rows.iter().find(|row| row.slot == slot)
    }

    /// Number of adopted slots.
    #[must_use]
    pub fn adopted_count(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| matches!(row.disposition, BlueprintDisposition::Adopted(_)))
            .count()
    }

    /// Number of explicitly unsupported slots.
    #[must_use]
    pub fn unsupported_count(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| matches!(row.disposition, BlueprintDisposition::Unsupported))
            .count()
    }
}

/// The one-call gate over the committed census.
#[must_use]
pub fn blueprint_disposition_findings() -> &'static [BlueprintDispositionFinding] {
    BlueprintDispositionTable::committed().findings()
}

impl FeatureId {
    /// The 1-based blueprint slot this typed feature adopts.
    ///
    /// `None` is reserved for the one deliberate extra feature that is not a
    /// blueprint slot (`MemoryProcessSwapCharge`). The census gate proves this
    /// mapping and the committed table agree in both directions.
    #[must_use]
    pub const fn blueprint_slot(self) -> Option<u8> {
        match self {
            // The generated arms below are the authored slot mapping.
            Self::ProcessSchedulerPolicy => Some(1),
            Self::ProcessPriorityMapping => Some(2),
            Self::ProcessEfficiencyMode => Some(3),
            Self::ProcessAffinityMask => Some(4),
            Self::ProcessTreeKill => Some(5),
            Self::ProcessTwoPhaseTermination => Some(6),
            Self::ProcessCgroupResourceLimiting => Some(7),
            Self::ProcessOomScore => Some(8),
            Self::ProcessTrimWorkingSet => Some(9),
            Self::ProcessCoreDump => Some(10),
            Self::ProcessAutoRestartRule => Some(11),
            Self::ProcessFreezeThawControl => Some(12),
            Self::ProcessCustomLabel => Some(13),
            Self::ProcessAncestorLineage => Some(14),
            Self::ProcessCompoundFilter => Some(15),
            Self::MemoryBreakdownRssPss => Some(16),
            Self::MemorySmapsRollupTiering => Some(17),
            Self::MemoryVmaMap => Some(18),
            Self::MemoryPageFaults => Some(19),
            Self::SharedMemorySegmentCrossAudit => Some(20),
            Self::MemoryDirtyPagePressure => Some(21),
            Self::MemoryCompressionZswap => Some(22),
            Self::MemoryTransparentHugePages => Some(23),
            Self::UserspaceAllocatorInference => Some(24),
            Self::WindowsCommitKernelPools => Some(25),
            Self::MacosMemoryClassification => Some(26),
            Self::MemoryLeakTrend => Some(27),
            Self::MemoryReadOnlyStringProbing => Some(28),
            Self::VmaSnapshotDiff => Some(29),
            Self::CodeSectionIntegrityAudit => Some(30),
            Self::HandleEnumeration => Some(31),
            Self::HandleTypeClassification => Some(32),
            Self::FileLocksAudit => Some(33),
            Self::DeletedFileHandleWatch => Some(34),
            Self::HandleFdLimitSaturation => Some(35),
            Self::PipePeerPidLookup => Some(36),
            Self::UdsPeerAssociation => Some(37),
            Self::DeviceNodeHandleInspection => Some(38),
            Self::HandleReversePathSearch => Some(39),
            Self::EmergencyCloseHandleAction => Some(40),
            Self::HandleLeakTrendAnalysis => Some(41),
            Self::WindowsAccessTokenAudit => Some(42),
            Self::MemfdSealsAudit => Some(43),
            Self::MountNamespaceRootLookup => Some(44),
            Self::EpollSubscriptionHotspots => Some(45),
            Self::ThreadTopologyEnumeration => Some(46),
            Self::ThreadContextSwitchRates => Some(47),
            Self::ThreadUninterruptibleSleepDiagnosis => Some(48),
            Self::ThreadWaitChannelClassification => Some(49),
            Self::ThreadCallstackSampling => Some(50),
            Self::ThreadFutexContention => Some(51),
            Self::ThreadStackLayoutGuard => Some(52),
            Self::ThreadSchedulingNiceAffinity => Some(53),
            Self::ThreadPoolWorkerHeuristics => Some(54),
            Self::ThreadSuspendResumeControl => Some(55),
            Self::ThreadZombieDetection => Some(56),
            Self::ThreadRunqueueLatency => Some(57),
            Self::ThreadLocalStorageAudit => Some(58),
            Self::ThreadChurnWarning => Some(59),
            Self::ThreadCpuSliceHeatmap => Some(60),
            Self::ProcessNetworkThroughput => Some(61),
            Self::SocketInventory => Some(62),
            Self::SocketStateTracking => Some(63),
            Self::ListeningPortTopology => Some(64),
            Self::SocketRttMetrics => Some(65),
            Self::SocketQueueBacklog => Some(66),
            Self::DnsReverseLookup => Some(67),
            Self::GeoIpAsnLookup => Some(68),
            Self::SocketConnectionAuditFeed => Some(69),
            Self::MaliciousConnectionDetection => Some(70),
            Self::NetworkSocketReset => Some(71),
            Self::NetworkTrafficShaping => Some(72),
            Self::ApplicationProtocolInference => Some(73),
            Self::OrphanSocketLeakDetection => Some(74),
            Self::NetworkRollingBandwidth => Some(75),
            Self::ProcessLogicalPhysicalIo => Some(76),
            Self::DiskDeviceTopology => Some(77),
            Self::DiskIopsQueueLatency => Some(78),
            Self::ProcessIoPriority => Some(79),
            Self::StorageFilesystemMountAudit => Some(80),
            Self::DirtyWritebackFlow => Some(81),
            Self::DiskSmartHealth => Some(82),
            Self::StorageSpaceDepletionCountdown => Some(83),
            Self::StorageIoStallDiagnosis => Some(84),
            Self::SwapThroughputRate => Some(85),
            Self::StorageTopAccessedFiles => Some(86),
            Self::VirtualMemoryFsCapacity => Some(87),
            Self::StorageTrimDiscardSupport => Some(88),
            Self::RaidPoolTopology => Some(89),
            Self::DiskReadOnlyFailureAlert => Some(90),
            Self::HardwareTopologyTree => Some(91),
            Self::CpuHeterogeneousCoreClass => Some(92),
            Self::CpuCacheTopology => Some(93),
            Self::NumaMemoryDistribution => Some(94),
            Self::ProcessNumaMemoryPolicy => Some(95),
            Self::NumaThrashingAlert => Some(96),
            Self::PcieLinkSpeedDegradation => Some(97),
            Self::CpuCoreFrequency => Some(98),
            Self::CpuThermalBoostStatus => Some(99),
            Self::CpuInstructionFeatures => Some(100),
            Self::SmbiosMemorySlots => Some(101),
            Self::MemoryChannelSymmetry => Some(102),
            Self::HeterogeneousInterconnectArchitecture => Some(103),
            Self::FirmwareSecureBoot => Some(104),
            Self::CpuInterruptAffinityBalance => Some(105),
            Self::GpuAdapterEnumeration => Some(106),
            Self::NpuTelemetry => Some(107),
            Self::GpuEngineUtilization => Some(108),
            Self::GpuMemoryReadout => Some(109),
            Self::ProcessGpuAttribution => Some(110),
            Self::GpuTemperatureReadout => Some(111),
            Self::GpuPowerDraw => Some(112),
            Self::GpuCoreClockFrequency => Some(113),
            Self::GpuThrottleReasons => Some(114),
            Self::GpuCodecUtilization => Some(115),
            Self::GpuProcessClients => Some(116),
            Self::NpuInferenceTelemetry => Some(117),
            Self::GpuLinkTopology => Some(118),
            Self::GpuFirmwareApiRuntime => Some(119),
            Self::GpuResetTdrLogs => Some(120),
            Self::RaplPowerDraw => Some(121),
            Self::EnergyImpactScore => Some(122),
            Self::BatteryPowerInventory => Some(123),
            Self::BatteryRemainingTimeEstimate => Some(124),
            Self::ThermalZoneSensors => Some(125),
            Self::ThermalThrottleEvents => Some(126),
            Self::CpuCStateAnalysis => Some(127),
            Self::CpuEnergyPerformancePreference => Some(128),
            Self::FanSpeedTelemetry => Some(129),
            Self::ProcessWakeupFrequency => Some(130),
            Self::PowerProfilesDaemon => Some(131),
            Self::SleepInhibitLocks => Some(132),
            Self::DisplayBacklightPower => Some(133),
            Self::UsbDeviceRuntimePm => Some(134),
            Self::SessionEnergyCostCalculation => Some(135),
            Self::LinuxNamespaceAudit => Some(136),
            Self::SandboxEnvironmentDetection => Some(137),
            Self::PosixCapabilitiesAudit => Some(138),
            Self::MandatoryAccessControlLabels => Some(139),
            Self::SeccompFilterAudit => Some(140),
            Self::LandlockSandboxAudit => Some(141),
            Self::SecurityNoNewPrivs => Some(142),
            Self::ProcessOwnerCredentials => Some(143),
            Self::BinarySignatureVerification => Some(144),
            Self::CompilerHardeningAudit => Some(145),
            Self::WindowsIntegrityLevels => Some(146),
            Self::MacosTccPermissions => Some(147),
            Self::PrivilegeAnomalyDetection => Some(148),
            Self::ProcessMasqueradingDetection => Some(149),
            Self::CodeInjectionDetection => Some(150),
            Self::ServiceInventoryStatus => Some(151),
            Self::UserServiceAggregation => Some(152),
            Self::ServiceStateMachine => Some(153),
            Self::ServiceUnitFileState => Some(154),
            Self::ServiceLifecycleControl => Some(155),
            Self::ServiceCgroupResourceLimits => Some(156),
            Self::SystemdDependencyDag => Some(157),
            Self::ServiceFailureDiagnosis => Some(158),
            Self::ServiceLogStream => Some(159),
            Self::ServiceRestartChurn => Some(160),
            Self::ServiceTimerSchedule => Some(161),
            Self::ServiceSocketActivation => Some(162),
            Self::ServiceUnitFileDropIns => Some(163),
            Self::PsiMultiWindowTelemetry => Some(166),
            Self::MemoryThrashingHealthScore => Some(168),
            Self::PressureLoadAverageNormalized => Some(169),
            Self::SoftirqSaturationAnalysis => Some(170),
            Self::PressureSaturationGrade => Some(167),
            Self::PressureDStateRatio => Some(171),
            Self::FilesystemLockLatency => Some(172),
            Self::MemoryCompactionStall => Some(173),
            Self::OomImminentWarning => Some(174),
            Self::GlobalFdPoolCapacity => Some(175),
            Self::IpcBufferSaturation => Some(176),
            Self::PressureNativeAlertIntegration => Some(180),
            Self::UseBottleneckAttribution => Some(177),
            Self::UnresponsiveAppDetection => Some(178),
            Self::DbusServiceTopology => Some(181),
            Self::DbusIntrospection => Some(182),
            Self::DbusTrafficMonitoring => Some(183),
            Self::SharedMemorySegments => Some(184),
            Self::PosixSysvMessageQueues => Some(185),
            Self::NamedSemaphoresInventory => Some(186),
            Self::PipeGraphVisualization => Some(187),
            Self::UdsPeerTopology => Some(188),
            Self::WindowsNamedPipes => Some(189),
            Self::PipeDeadlockDiagnosis => Some(190),
            Self::KernelEventNotifiers => Some(191),
            Self::SharedMemorySecurityAudit => Some(192),
            Self::DbusActivationLatency => Some(193),
            Self::OrphanIpcReclamation => Some(194),
            Self::TopIpcThroughputProducers => Some(195),
            Self::PmuCounterAbstraction => Some(196),
            Self::SyscallDistributionProfiling => Some(197),
            Self::SlowSyscallTrap => Some(198),
            Self::OffCpuBlockingAnalysis => Some(199),
            Self::ProcessFileActivityTrace => Some(200),
            Self::SocketLifecycleAuditStream => Some(201),
            Self::ProcessEventTrace => Some(202),
            Self::CpuFlameGraph => Some(203),
            Self::DynamicAllocationProfiling => Some(204),
            Self::MutexContentionProfiler => Some(205),
            Self::PageFaultCallchainBacktrace => Some(206),
            Self::ContextSwitchTriggerAnalysis => Some(207),
            Self::FlightRecordingModeHook => Some(208),
            Self::TraceDataStandardExport => Some(209),
            Self::DynamicTracingCapabilityGate => Some(210),
            Self::MultiResolutionRingBuffer => Some(211),
            Self::TimeTravelScrubber => Some(212),
            Self::GhostProcessesCapture => Some(213),
            Self::SystemSnapshotDiff => Some(214),
            Self::BlackboxCrashPreservation => Some(215),
            Self::OfflineSessionRecording => Some(216),
            Self::MultiFormatExport => Some(217),
            Self::TelemetryPercentileAggregation => Some(218),
            Self::HistoryLttbDownsampling => Some(219),
            Self::EventTimelineBookmarks => Some(220),
            Self::LeaderboardRankEvolution => Some(221),
            Self::BenchmarkAbComparison => Some(222),
            Self::HistoryChartImageExport => Some(223),
            Self::VariableSpeedReplayEngine => Some(224),
            Self::EnterprisePrivacyRedaction => Some(225),
            Self::StartupImpactAssessment => Some(164),
            Self::StartupItemControl => Some(165),
            Self::MemoryProcessSwapCharge => None,
        }
    }

    /// Blueprint slots a typed feature covers in addition to its primary
    /// [`FeatureId::blueprint_slot`]. Empty for every one-slot feature; an
    /// alias is valid only when the feature's semantic specification covers
    /// the slot, and the census gate checks the row in both directions.
    #[must_use]
    pub const fn blueprint_alias_slots(self) -> &'static [u8] {
        match self {
            // The system health score (slot 179) is the same delivered surface
            // as the thrashing/health-score feature (slot 168).
            Self::MemoryThrashingHealthScore => &[179],
            _ => &[],
        }
    }
}

#[cfg(test)]
#[path = "../../tests/headless/ui_blueprint_dispositions.rs"]
mod tests;
