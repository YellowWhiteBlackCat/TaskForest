//! Windows process control providers and escalation seam dispatch.

use super::*;

#[cfg(windows)]
pub(crate) fn map_windows_api_failure(
    error: taskmanager_windows_api::WindowsApiError,
) -> ProviderFailure {
    match error {
        taskmanager_windows_api::WindowsApiError::PermissionDenied => {
            ProviderFailure::PermissionDenied
        }
        taskmanager_windows_api::WindowsApiError::IdentityChanged
        | taskmanager_windows_api::WindowsApiError::InvalidInput => {
            ProviderFailure::IdentityChanged
        }
        taskmanager_windows_api::WindowsApiError::Unsupported => ProviderFailure::Unsupported,
        taskmanager_windows_api::WindowsApiError::ResourceLimit
        | taskmanager_windows_api::WindowsApiError::InvalidText
        | taskmanager_windows_api::WindowsApiError::QueryFailed => {
            ProviderFailure::TemporarilyUnavailable
        }
    }
}

// Consumed by the `#[cfg(windows)]` set-priority path and by the mounted
// headless mapping test; dormant elsewhere.
#[cfg(any(windows, test))]
pub(crate) fn windows_priority_class(
    tier: PriorityTier,
) -> taskmanager_windows_api::ProcessPriorityClass {
    match tier {
        PriorityTier::High => taskmanager_windows_api::ProcessPriorityClass::AboveNormal,
        PriorityTier::Normal => taskmanager_windows_api::ProcessPriorityClass::Normal,
        PriorityTier::Low => taskmanager_windows_api::ProcessPriorityClass::BelowNormal,
    }
}

/// Preserve the helper's typed failure vocabulary at the provider boundary.
///
/// Keep this exhaustive over the shared escalation enum so a new helper
/// failure cannot silently collapse into identity or permission semantics.
#[cfg(any(windows, test))]
pub(crate) const fn map_foreign_control_failure(
    failure: taskmanager_escalation::polkit::ForeignProcessControlFailure,
) -> ProviderFailure {
    use taskmanager_escalation::polkit::ForeignProcessControlFailure;
    match failure {
        ForeignProcessControlFailure::IdentityChanged => ProviderFailure::IdentityChanged,
        ForeignProcessControlFailure::PermissionDenied => ProviderFailure::PermissionDenied,
        ForeignProcessControlFailure::Unsupported => ProviderFailure::Unsupported,
        ForeignProcessControlFailure::Rejected => ProviderFailure::Rejected,
        ForeignProcessControlFailure::OperationFailed => ProviderFailure::ProviderFault,
    }
}

/// Map prompt/helper availability without fabricating a user denial.
#[cfg(any(windows, test))]
pub(crate) const fn map_escalation_denial(
    reason: taskmanager_escalation::EscalationDenialReason,
) -> ProviderFailure {
    use taskmanager_escalation::EscalationDenialReason;
    match reason {
        EscalationDenialReason::Unsupported => ProviderFailure::Unsupported,
        EscalationDenialReason::PermissionDenied => ProviderFailure::PermissionDenied,
        EscalationDenialReason::AuthorizationUnavailable => ProviderFailure::TemporarilyUnavailable,
        EscalationDenialReason::HelperUnavailable => ProviderFailure::RequiresEscalation,
        EscalationDenialReason::HelperProtocolViolation => ProviderFailure::ProviderFault,
    }
}

#[cfg(windows)]
pub(crate) fn finish_with_escalation(
    target: &FrozenProcessIdentity,
    operation: taskmanager_escalation::polkit::ForeignProcessControlOperation,
    direct: Result<(), ProviderFailure>,
) -> Result<(), ProviderFailure> {
    let Err(ProviderFailure::PermissionDenied) = direct else {
        return direct;
    };
    let Some(helper_target) = target.authoritative_start_token().and_then(|token| {
        taskmanager_escalation::polkit::ForeignProcessControlTarget::new(target.pid, token)
    }) else {
        return Err(ProviderFailure::IdentityChanged);
    };
    match taskmanager_escalation::polkit::invoke_foreign_process_control(helper_target, operation) {
        taskmanager_escalation::polkit::ForeignProcessControlOutcome::Applied => Ok(()),
        taskmanager_escalation::polkit::ForeignProcessControlOutcome::Failed { kind, .. } => {
            Err(map_foreign_control_failure(kind))
        }
        taskmanager_escalation::polkit::ForeignProcessControlOutcome::Unavailable {
            reason,
            ..
        } => Err(map_escalation_denial(reason)),
    }
}

/// Process control: End/Kill and SetPriority use the audited native
/// exact-identity boundary (the neutral `PriorityTier` maps onto
/// `SetPriorityClass`). Suspend/Resume use the audited per-thread path
/// (ToolHelp32 snapshot + `OpenThread(THREAD_SUSPEND_RESUME)` +
/// `SuspendThread`/`ResumeThread`); the undocumented `NtSuspendProcess` /
/// `NtResumeProcess` pair is deliberately not used (ADR-018) because the two
/// mechanisms are not interchangeable.
pub struct WinProcessControlProvider {
    #[cfg(not(windows))]
    system: sysinfo::System,
}

impl WinProcessControlProvider {
    pub fn new() -> Self {
        Self {
            #[cfg(not(windows))]
            system: sysinfo::System::new(),
        }
    }

    #[cfg(not(windows))]
    fn refresh_for_control(&mut self) {
        self.system
            .refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    }

    #[cfg(windows)]
    fn refresh_for_control(&mut self) {
        // `terminate_process_exact` opens the target itself, re-reads the
        // kernel creation time on that owned handle, and only then terminates
        // it. A preceding full sysinfo scan adds latency but no safety.
    }

    /// Terminate the exact frozen process from the CURRENT snapshot. The
    /// batch hot path refreshes once before the loop; the Windows build then
    /// re-checks the kernel creation-time token on the same owned handle so a
    /// PID reuse can never authorize the replacement process.
    pub(crate) fn kill_process_from_snapshot(
        &self,
        target: &FrozenProcessIdentity,
    ) -> Result<(), ProviderFailure> {
        #[cfg(windows)]
        {
            let expected = target
                .authoritative_start_token()
                .ok_or(ProviderFailure::IdentityChanged)?;
            let direct = taskmanager_windows_api::terminate_process_exact(target.pid, expected)
                .map_err(map_windows_api_failure);
            finish_with_escalation(
                target,
                taskmanager_escalation::polkit::ForeignProcessControlOperation::Kill,
                direct,
            )
        }

        #[cfg(not(windows))]
        {
            let Some(expected) = target.authoritative_start_token() else {
                return Err(ProviderFailure::IdentityChanged);
            };
            let Some(process) = self
                .system
                .processes()
                .get(&sysinfo::Pid::from_u32(target.pid))
            else {
                return Err(ProviderFailure::IdentityChanged);
            };
            if process.start_time() != expected {
                return Err(ProviderFailure::IdentityChanged);
            }
            if process.kill() {
                Ok(())
            } else {
                Err(ProviderFailure::PermissionDenied)
            }
        }
    }

    fn set_threads_suspended_from_snapshot(
        &self,
        target: &FrozenProcessIdentity,
        suspend: bool,
    ) -> Result<(), ProviderFailure> {
        #[cfg(windows)]
        {
            // Mechanism constraint (ADR-018): the documented per-thread path is
            // a ToolHelp32 snapshot of the target's threads plus
            // OpenThread(THREAD_SUSPEND_RESUME) + SuspendThread/ResumeThread;
            // NtSuspendProcess/NtResumeProcess are undocumented and not
            // interchangeable with per-thread suspend (psutil #1379 lesson).
            // The pid-only boundary primitive is bracketed by creation-token
            // validation on both sides so a PID reuse cannot publish a
            // silently-wrong suspension as success.
            let expected = validate_process_target(target)?;
            let direct = if suspend {
                taskmanager_windows_api::suspend_process_threads(target.pid)
            } else {
                taskmanager_windows_api::resume_process_threads(target.pid)
            }
            .map(|_| ())
            .map_err(map_windows_api_failure)
            .and_then(|()| validate_process_target_after(target, expected));
            finish_with_escalation(
                target,
                if suspend {
                    taskmanager_escalation::polkit::ForeignProcessControlOperation::Suspend
                } else {
                    taskmanager_escalation::polkit::ForeignProcessControlOperation::Resume
                },
                direct,
            )
        }

        #[cfg(not(windows))]
        {
            let _ = (target, suspend);
            Err(ProviderFailure::Unsupported)
        }
    }

    fn set_priority_from_snapshot(
        &self,
        target: &FrozenProcessIdentity,
        tier: PriorityTier,
    ) -> Result<(), ProviderFailure> {
        #[cfg(windows)]
        {
            let expected = target
                .authoritative_start_token()
                .ok_or(ProviderFailure::IdentityChanged)?;
            let direct = taskmanager_windows_api::set_process_priority_exact(
                target.pid,
                expected,
                windows_priority_class(tier),
            )
            .map_err(map_windows_api_failure);
            finish_with_escalation(
                target,
                taskmanager_escalation::polkit::ForeignProcessControlOperation::SetPriority(
                    tier.canonical_nice(),
                ),
                direct,
            )
        }

        #[cfg(not(windows))]
        {
            let _ = (target, tier);
            Err(ProviderFailure::Unsupported)
        }
    }
}

impl ProcessControlProvider for WinProcessControlProvider {
    fn end_task(&mut self, target: FrozenProcessIdentity) -> Result<(), ProviderFailure> {
        self.refresh_for_control();
        self.kill_process_from_snapshot(&target)
    }

    fn execute_batch(
        &mut self,
        intent: ProcessBatchIntent,
    ) -> Result<ProcessBatchResult, ProviderFailure> {
        // On Windows the exact native boundary revalidates every target on
        // its own handle, so a batch does not pay a redundant full process
        // enumeration. The non-Windows cross-target fallback keeps its
        // sysinfo snapshot through `refresh_for_control`.
        self.refresh_for_control();
        let mut results = Vec::new();
        let targets = intent.targets.clone();
        for target in targets {
            let outcome = match intent.action {
                ProcessBatchAction::End | ProcessBatchAction::Kill => {
                    match self.kill_process_from_snapshot(&target) {
                        Ok(()) => ProcessBatchTargetResult::Applied,
                        Err(failure) => ProcessBatchTargetResult::Failed(failure.kind()),
                    }
                }
                ProcessBatchAction::SetPriority(tier) => {
                    match self.set_priority_from_snapshot(&target, tier) {
                        Ok(()) => ProcessBatchTargetResult::Applied,
                        Err(failure) => ProcessBatchTargetResult::Failed(failure.kind()),
                    }
                }
                // The audited per-thread boundary carries suspend/resume; the
                // non-Windows fallback stays a typed Unsupported result.
                ProcessBatchAction::Suspend | ProcessBatchAction::Resume => {
                    match self.set_threads_suspended_from_snapshot(
                        &target,
                        intent.action == ProcessBatchAction::Suspend,
                    ) {
                        Ok(()) => ProcessBatchTargetResult::Applied,
                        Err(failure) => ProcessBatchTargetResult::Failed(failure.kind()),
                    }
                }
            };
            results.push((target, outcome));
        }
        let batch = ProcessBatchResult {
            intent,
            targets: results,
        };
        Ok(batch)
    }

    fn send_signal(
        &mut self,
        target: &FrozenProcessIdentity,
        signal: ProcessSignal,
    ) -> Result<(), ProviderFailure> {
        match signal {
            ProcessSignal::Terminate | ProcessSignal::Kill => {
                self.refresh_for_control();
                self.kill_process_from_snapshot(target)
            }
            // POSIX job-control stop/continue map onto the audited per-thread
            // suspend/resume boundary on Windows.
            ProcessSignal::Stop | ProcessSignal::Continue => {
                self.refresh_for_control();
                self.set_threads_suspended_from_snapshot(target, signal == ProcessSignal::Stop)
            }
            ProcessSignal::Hangup
            | ProcessSignal::Interrupt
            | ProcessSignal::User1
            | ProcessSignal::User2 => Err(ProviderFailure::Unsupported),
        }
    }
}

pub struct WinProcessAffinityControlProvider;

impl ProcessAffinityControlProvider for WinProcessAffinityControlProvider {
    fn set_affinity(
        &mut self,
        target: &FrozenProcessIdentity,
        cpus: &[u32],
    ) -> Result<(), ProviderFailure> {
        #[cfg(windows)]
        {
            let expected = target
                .authoritative_start_token()
                .ok_or(ProviderFailure::IdentityChanged)?;
            let direct =
                taskmanager_windows_api::set_process_affinity_exact(target.pid, expected, cpus)
                    .map_err(map_windows_api_failure);
            finish_with_escalation(
                target,
                taskmanager_escalation::polkit::ForeignProcessControlOperation::SetAffinity(
                    cpus.to_vec(),
                ),
                direct,
            )
        }

        #[cfg(not(windows))]
        {
            let _ = (target, cpus);
            Err(ProviderFailure::Unsupported)
        }
    }
}

/// Session-scoped job-object resource control (ADR-018 safety seam).
///
/// Limits ride a boundary-owned nested job (Windows 8+), so they tighten only
/// and evaporate when the app exits; they are NOT persistent cgroup writes.
/// Frozen-identity validation brackets the boundary call exactly like
/// kill/priority: the creation token is verified before the boundary opens
/// the target and re-verified after the mutation.
///
pub struct WinProcessResourceControlProvider;

impl WinProcessResourceControlProvider {
    /// Translate the platform-neutral limit request onto the boundary's job
    /// dimensions.
    ///
    /// - `LimitValue::Value` maps to the matching job limit.
    /// - `LimitValue::Unlimited` drops the dimension from the job request;
    ///   because job limits only ever tighten, single-dimension relaxation
    ///   is not representable and full relaxation is the all-`None` clear
    ///   path.
    /// - A CPU quota/period pair maps to a rate-control percent only when
    ///   `quota * 100 / period` is an exact integer in 1..=100. A fractional
    ///   quota (for example 2.5 cores), a zero period, a zero quota, or more
    ///   than one whole CPU is typed `Unsupported` for the whole request —
    ///   never silently rounded.
    /// - An all-`None` translation selects the clear path: dropping the
    ///   boundary-owned job evaporates every session-scoped limit.
    #[cfg(any(windows, test))]
    fn job_limit_request(
        limits: &taskmanager_core::ResourceGroupLimitRequest,
    ) -> Result<Option<taskmanager_windows_api::WindowsJobLimitRequest>, ProviderFailure> {
        use taskmanager_core::LimitValue;

        let memory_limit_bytes = match limits.memory {
            Some(LimitValue::Value(bytes)) => Some(bytes),
            Some(LimitValue::Unlimited) | None => None,
        };
        let process_count_limit = match limits.processes {
            Some(LimitValue::Value(count)) => {
                Some(u32::try_from(count).map_err(|_| ProviderFailure::Unsupported)?)
            }
            Some(LimitValue::Unlimited) | None => None,
        };
        let cpu_rate_percent = match limits.cpu {
            Some(cpu) => Self::cpu_rate_percent(&cpu)?,
            None => None,
        };

        let request = taskmanager_windows_api::WindowsJobLimitRequest {
            memory_limit_bytes,
            process_count_limit,
            cpu_rate_percent,
        };
        if request == taskmanager_windows_api::WindowsJobLimitRequest::default() {
            Ok(None)
        } else {
            Ok(Some(request))
        }
    }

    /// Whole-number percent 1..=100 or a typed `Unsupported`; the arithmetic
    /// runs in `u128` so a large quota can never wrap into a bogus rate.
    #[cfg(any(windows, test))]
    fn cpu_rate_percent(
        cpu: &taskmanager_core::ResourceGroupCpuLimit,
    ) -> Result<Option<u32>, ProviderFailure> {
        use taskmanager_core::LimitValue;

        let quota = match cpu.quota {
            LimitValue::Value(quota) => quota,
            LimitValue::Unlimited => return Ok(None),
        };
        if cpu.period_micros == 0 {
            return Err(ProviderFailure::Unsupported);
        }
        let scaled = u128::from(quota) * 100;
        if scaled % u128::from(cpu.period_micros) != 0 {
            return Err(ProviderFailure::Unsupported);
        }
        let percent = scaled / u128::from(cpu.period_micros);
        if !(1..=100).contains(&percent) {
            return Err(ProviderFailure::Unsupported);
        }
        Ok(Some(
            u32::try_from(percent).map_err(|_| ProviderFailure::Unsupported)?,
        ))
    }
}

impl ProcessResourceControlProvider for WinProcessResourceControlProvider {
    fn apply_limits(
        &mut self,
        target: &FrozenProcessIdentity,
        limits: &taskmanager_core::ResourceGroupLimitRequest,
    ) -> Result<(), ProviderFailure> {
        #[cfg(windows)]
        {
            let expected = validate_process_target(target)?;
            let direct = match Self::job_limit_request(limits)? {
                None => {
                    // Every dimension is absent or unlimited: releasing the
                    // boundary-owned job is the honest full relaxation.
                    taskmanager_windows_api::clear_process_job_limits(target.pid, expected)
                        .map(|_| ())
                        .map_err(map_windows_api_failure)
                }
                Some(request) => taskmanager_windows_api::apply_process_job_limits(
                    target.pid, expected, &request,
                )
                .map_err(map_windows_api_failure),
            };
            // The foreign-process escalation vocabulary has no job-limits
            // operation, so a denied open stays a typed `PermissionDenied` —
            // the honest refusal rather than an unrelated escalation lane.
            direct.and_then(|()| validate_process_target_after(target, expected))
        }

        #[cfg(not(windows))]
        {
            let _ = (target, limits);
            Err(ProviderFailure::Unsupported)
        }
    }
}

/// Windows has no per-process byte-accounting escalation path (no AF_PACKET /
/// SCM_RIGHTS chain off-Linux): the honest `Unsupported` answer.
pub struct PendingProcessNetworkEscalationProvider;

impl ProcessNetworkEscalationProvider for PendingProcessNetworkEscalationProvider {
    fn request_capture_escalation(&mut self) -> Result<(), ProviderFailure> {
        Err(ProviderFailure::Unsupported)
    }
}

/// Windows process control providers.
pub struct WinProcessControlProviders {
    pub(crate) affinity_control: ProviderRegistration<
        ProcessAffinityControlRequest,
        Box<dyn ProcessAffinityControlProvider>,
    >,
    pub(crate) resource_control: ProviderRegistration<
        ProcessResourceControlRequest,
        Box<dyn ProcessResourceControlProvider>,
    >,
    pub(crate) network_escalation: ProviderRegistration<
        ProcessNetworkEscalationRequest,
        Box<dyn ProcessNetworkEscalationProvider>,
    >,
    pub(crate) control:
        ProviderRegistration<ProcessControlRequest, Box<dyn ProcessControlProvider>>,
}

impl WinProcessControlProviders {
    #[must_use]
    pub fn new<A, R, E, C>(
        affinity_control: ProviderRegistration<ProcessAffinityControlRequest, A>,
        resource_control: ProviderRegistration<ProcessResourceControlRequest, R>,
        network_escalation: ProviderRegistration<ProcessNetworkEscalationRequest, E>,
        control: ProviderRegistration<ProcessControlRequest, C>,
    ) -> Self
    where
        A: ProcessAffinityControlProvider,
        R: ProcessResourceControlProvider,
        E: ProcessNetworkEscalationProvider,
        C: ProcessControlProvider,
    {
        Self {
            affinity_control: affinity_control.map_provider(|provider| {
                Box::new(provider) as Box<dyn ProcessAffinityControlProvider>
            }),
            resource_control: resource_control.map_provider(|provider| {
                Box::new(provider) as Box<dyn ProcessResourceControlProvider>
            }),
            network_escalation: network_escalation.map_provider(|provider| {
                Box::new(provider) as Box<dyn ProcessNetworkEscalationProvider>
            }),
            control: control
                .map_provider(|provider| Box::new(provider) as Box<dyn ProcessControlProvider>),
        }
    }

    pub(crate) fn into_runtime(self) -> ProcessControlExecutors {
        let Self {
            affinity_control,
            resource_control,
            network_escalation,
            control,
        } = self;
        let mut affinity_control = affinity_control.into_provider();
        let mut resource_control = resource_control.into_provider();
        let mut network_escalation = network_escalation.into_provider();
        let mut control = control.into_provider();
        use taskmanager_platform_runtime::ProcessControlCompletion;
        ProcessControlExecutors::new(
            move |target, cpus| affinity_control.set_affinity(&target, &cpus),
            move |request| match request {
                taskmanager_application::ProcessControlRequest::EndTask(target) => {
                    control.end_task(target.clone())?;
                    Ok(ProcessControlCompletion::EndTask(target))
                }
                taskmanager_application::ProcessControlRequest::ExecuteBatch(intent) => Ok(
                    ProcessControlCompletion::Batch(control.execute_batch(intent)?),
                ),
                taskmanager_application::ProcessControlRequest::SendSignal { target, signal } => {
                    control.send_signal(&target, signal)?;
                    Ok(ProcessControlCompletion::Signal { target, signal })
                }
                // The neutral suspend/resume concepts map onto the job-control
                // stop/continue signals at this adapter edge, which the Windows
                // provider backs with the audited per-thread boundary; the
                // completion rides the signal event (same shape as macOS).
                taskmanager_application::ProcessControlRequest::Suspend { target } => {
                    control.send_signal(&target, ProcessSignal::Stop)?;
                    Ok(ProcessControlCompletion::Signal {
                        target,
                        signal: ProcessSignal::Stop,
                    })
                }
                taskmanager_application::ProcessControlRequest::Resume { target } => {
                    control.send_signal(&target, ProcessSignal::Continue)?;
                    Ok(ProcessControlCompletion::Signal {
                        target,
                        signal: ProcessSignal::Continue,
                    })
                }
            },
            move |target, limits| resource_control.apply_limits(&target, &limits),
            move || network_escalation.request_capture_escalation(),
        )
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/platform_windows_provider_process_control.rs"]
mod tests;
