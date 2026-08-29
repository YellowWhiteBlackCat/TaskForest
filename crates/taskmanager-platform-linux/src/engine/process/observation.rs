//! Assembly of typed process-row scalar observations.

use taskmanager_core::{
    FailureKind, ProcessItem, ProcessScalarObservations, ScalarObservation, SourceOutcome,
};

use super::PreviousProcessView;
use super::procfs::{
    FdCount, ProcIoFields, ProcStatFields, clock_ticks_per_second, read_fd_count, read_proc_io,
    read_proc_stat, read_proc_status_memory,
};
use super::rates::{ProcessRateInput, ProcessRateState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ProcessScalarEvidence {
    pub(super) stat: SourceOutcome,
    pub(super) fds: SourceOutcome,
    pub(super) memory: SourceOutcome,
    pub(super) io: SourceOutcome,
    pub(super) rates: SourceOutcome,
}

#[derive(Debug, Clone, Copy)]
struct ProcessScalarInputs {
    pid: u32,
    stat: Result<ProcStatFields, FailureKind>,
    identity_confirmation: Result<u64, FailureKind>,
    /// `None` means the caller deferred the `/proc/<pid>/fd` scan this tick
    /// (fd count is sampled at a lower cadence — see
    /// `FD_COUNT_REFRESH_EVERY_N_TICKS`). The fd field then either reuses the
    /// previous value via `retain_previous` or, with no prior value, becomes a
    /// typed `Unavailable` (never a fabricated 0).
    fds: Option<Result<FdCount, FailureKind>>,
    memory: Result<u64, FailureKind>,
    io: Result<ProcIoFields, FailureKind>,
}

#[derive(Clone, Copy)]
struct ProcessObservationContext<'a, P: PreviousProcessView + ?Sized = ProcessItem> {
    boot_time: &'a Result<u64, FailureKind>,
    clock_ticks: &'a Result<u64, FailureKind>,
    observed_at_ms: u64,
    previous: Option<&'a P>,
}

#[derive(Debug, Clone, Copy)]
struct CurrentProcessInputs {
    pid: u32,
    start_token: u64,
    stat: ProcStatFields,
    memory: Result<u64, FailureKind>,
    io: Result<ProcIoFields, FailureKind>,
}

pub(super) fn observe_process_scalars<P: PreviousProcessView + ?Sized>(
    pid: u32,
    boot_time: &Result<u64, FailureKind>,
    clock_ticks: &Result<u64, FailureKind>,
    observed_at_ms: u64,
    previous: Option<&P>,
    rate_state: &mut ProcessRateState,
    want_fd_count: bool,
) -> (ProcessScalarObservations, ProcessScalarEvidence) {
    let stat = read_proc_stat(pid);
    let fds = if want_fd_count {
        Some(read_fd_count(pid))
    } else {
        None
    };
    let memory = read_proc_status_memory(pid);
    let io = read_proc_io(pid);
    let confirmation = match stat {
        Ok(_) => read_proc_stat(pid).map(|stat| stat.start_ticks),
        Err(failure) => Err(failure),
    };
    observations_from_results(
        ProcessScalarInputs {
            pid,
            stat,
            identity_confirmation: confirmation,
            fds,
            memory,
            io,
        },
        ProcessObservationContext {
            boot_time,
            clock_ticks,
            observed_at_ms,
            previous,
        },
        rate_state,
    )
}

pub(super) fn mark_retained_item_stale(item: &mut ProcessItem, failure: FailureKind) {
    let metadata_failure =
        taskmanager_core::ProcessMetadataFailure::from_inventory_failure(failure);
    item.apply_metadata_observations(
        item.metadata_observations()
            .clone()
            .transition_failure(metadata_failure),
    );
    item.apply_application_identity(
        item.application_identity_observation()
            .clone()
            .transition_failure(metadata_failure),
    );
    item.apply_scalar_observations(item.scalar_observations().transition_failure(failure));
}

fn observations_from_results<P: PreviousProcessView + ?Sized>(
    inputs: ProcessScalarInputs,
    context: ProcessObservationContext<'_, P>,
    rate_state: &mut ProcessRateState,
) -> (ProcessScalarObservations, ProcessScalarEvidence) {
    let ProcessScalarInputs {
        pid,
        stat,
        identity_confirmation,
        fds,
        memory,
        io,
    } = inputs;
    let observed_at_ms = context.observed_at_ms;
    let previous = context.previous;
    let identity = confirmed_start_token(&stat, &identity_confirmation);
    let stat_outcome = outcome_from_result(&identity);
    let deferred_fd_tick = fds.is_none();
    let (mut fd_outcome, memory_outcome, io_outcome) = match identity {
        Ok(_) => {
            let fd_outcome = match &fds {
                Some(result) => fd_outcome(result),
                // A deferred tick is resolved after the retain pass below: if a
                // prior fd value is retained for an unchanged identity, the fd
                // source outcome stays Available so the aggregate fd source
                // status does not toggle across the decimation cadence; with no
                // retained value it is honestly Empty (no measurement, not a
                // failure).
                None => SourceOutcome::Empty,
            };
            (fd_outcome, outcome_from_result(&memory), io_outcome(&io))
        }
        Err(failure) => {
            let unavailable = SourceOutcome::Unavailable(failure);
            (unavailable, unavailable, unavailable)
        }
    };
    let mut observations = match (stat, identity) {
        (Ok(stat), Ok(start_token)) => observations_for_current_identity(
            CurrentProcessInputs {
                pid,
                start_token,
                stat,
                memory,
                io,
            },
            context,
            rate_state,
        ),
        (_, Err(failure)) => {
            rate_state.reset(pid);
            unavailable_identity_observations(failure)
        }
        (Err(failure), Ok(_)) => {
            rate_state.reset(pid);
            unavailable_identity_observations(failure)
        }
    };
    if identity.is_ok() {
        observations.fds = match fds {
            Some(Ok(FdCount {
                value,
                partial_failure: Some(failure),
            })) => ScalarObservation::partial(value, observed_at_ms, failure),
            Some(Ok(FdCount {
                value,
                partial_failure: None,
            })) => ScalarObservation::available(value, observed_at_ms),
            Some(Err(failure)) => ScalarObservation::unavailable(failure),
            // Deferred tick: seed an Unavailable so the `retain_previous`
            // pass below bridges the prior fd value when the identity is
            // unchanged (Stale carrying the last_known count). With no prior
            // value this stays a typed Unavailable — never a fabricated 0.
            None => ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable),
        };
    }

    if let (Some(current_token), Some(previous)) =
        (observations.start_token.current_value().copied(), previous)
        && previous.current_start_token() == Some(current_token)
    {
        observations = observations.retain_previous(*previous.scalar_observations());
    }
    if deferred_fd_tick && identity.is_ok() {
        // The per-tick fd source status must not toggle Available/Empty across
        // the `FD_COUNT_REFRESH_EVERY_N_TICKS` decimation cadence (which would
        // flap the aggregate `PROCESS_FD_PROVIDER` 1 Available : 4 Empty). When
        // a deferred tick reuses a prior measured count (Stale carrying a
        // last_known value over an unchanged identity), keep the source
        // Available — there IS a valid last-measured fd count, and the value's
        // Stale availability already conveys "not freshly read this tick". With
        // no retained value (no previous, or identity changed) the column is
        // honestly Empty; an Err identity keeps the Unavailable outcome above.
        fd_outcome = if observations.fds.last_known_value().is_some() {
            SourceOutcome::Available
        } else {
            SourceOutcome::Empty
        };
    }
    let rates_outcome = rate_outcome(&observations);

    (
        observations,
        ProcessScalarEvidence {
            stat: stat_outcome,
            fds: fd_outcome,
            memory: memory_outcome,
            io: io_outcome,
            rates: rates_outcome,
        },
    )
}

fn observations_for_current_identity<P: PreviousProcessView + ?Sized>(
    inputs: CurrentProcessInputs,
    context: ProcessObservationContext<'_, P>,
    rate_state: &mut ProcessRateState,
) -> ProcessScalarObservations {
    let CurrentProcessInputs {
        pid,
        start_token,
        stat,
        memory,
        io,
    } = inputs;
    let ProcessObservationContext {
        boot_time,
        clock_ticks,
        observed_at_ms,
        ..
    } = context;
    let mut observations = observations_from_stat(stat, boot_time, clock_ticks, observed_at_ms);
    observations.start_token = ScalarObservation::available(start_token, observed_at_ms);
    observations.memory_bytes = scalar_from_result(memory, observed_at_ms);
    let (read_bytes, write_bytes) = io_fields(io);
    observations.disk_read_bytes_total = scalar_from_result(read_bytes, observed_at_ms);
    observations.disk_write_bytes_total = scalar_from_result(write_bytes, observed_at_ms);
    let rates = rate_state.observe(ProcessRateInput {
        pid,
        start_token,
        observed_at_ms,
        clock_ticks,
        cpu_ticks: total_cpu_ticks(stat),
        disk_read_bytes: read_bytes,
        disk_write_bytes: write_bytes,
    });
    observations.cpu_percentage = rates.cpu_percentage;
    observations.disk_read_bytes_per_sec = rates.disk_read_bytes_per_sec;
    observations.disk_write_bytes_per_sec = rates.disk_write_bytes_per_sec;
    observations
}

fn observations_from_stat(
    stat: ProcStatFields,
    boot_time: &Result<u64, FailureKind>,
    clock_ticks: &Result<u64, FailureKind>,
    observed_at_ms: u64,
) -> ProcessScalarObservations {
    let start_time_secs =
        start_time_observation(stat.start_ticks, boot_time, clock_ticks, observed_at_ms);
    let cpu_time_secs = cpu_time_observation(
        stat.user_ticks,
        stat.system_ticks,
        clock_ticks,
        observed_at_ms,
    );
    ProcessScalarObservations {
        threads: ScalarObservation::available(stat.threads, observed_at_ms),
        start_time_secs,
        cpu_time_secs,
        fds: ScalarObservation::default(),
        nice: ScalarObservation::available(stat.nice, observed_at_ms),
        ..ProcessScalarObservations::default()
    }
}

fn unavailable_identity_observations(failure: FailureKind) -> ProcessScalarObservations {
    ProcessScalarObservations {
        start_token: ScalarObservation::unavailable(failure),
        cpu_percentage: ScalarObservation::unavailable(failure),
        memory_bytes: ScalarObservation::unavailable(failure),
        memory_pss_bytes: ScalarObservation::unavailable(failure),
        swap_bytes: ScalarObservation::unavailable(failure),
        disk_read_bytes_total: ScalarObservation::unavailable(failure),
        disk_write_bytes_total: ScalarObservation::unavailable(failure),
        disk_read_bytes_per_sec: ScalarObservation::unavailable(failure),
        disk_write_bytes_per_sec: ScalarObservation::unavailable(failure),
        threads: ScalarObservation::unavailable(failure),
        start_time_secs: ScalarObservation::unavailable(failure),
        cpu_time_secs: ScalarObservation::unavailable(failure),
        fds: ScalarObservation::unavailable(failure),
        nice: ScalarObservation::unavailable(failure),
    }
}

fn confirmed_start_token(
    stat: &Result<ProcStatFields, FailureKind>,
    confirmation: &Result<u64, FailureKind>,
) -> Result<u64, FailureKind> {
    match (stat, confirmation) {
        (Ok(stat), Ok(token)) if stat.start_ticks == *token => Ok(*token),
        (Ok(_), Ok(_)) => Err(FailureKind::IdentityChanged),
        (Ok(_), Err(failure)) | (Err(failure), _) => Err(*failure),
    }
}

fn total_cpu_ticks(stat: ProcStatFields) -> Result<u64, FailureKind> {
    stat.user_ticks
        .checked_add(stat.system_ticks)
        .ok_or(FailureKind::ProviderFault)
}

fn scalar_from_result<T>(
    result: Result<T, FailureKind>,
    observed_at_ms: u64,
) -> ScalarObservation<T> {
    result.map_or_else(ScalarObservation::unavailable, |value| {
        ScalarObservation::available(value, observed_at_ms)
    })
}

fn io_fields(
    result: Result<ProcIoFields, FailureKind>,
) -> (Result<u64, FailureKind>, Result<u64, FailureKind>) {
    match result {
        Ok(fields) => (fields.read_bytes, fields.write_bytes),
        Err(failure) => (Err(failure), Err(failure)),
    }
}

fn start_time_observation(
    start_ticks: u64,
    boot_time: &Result<u64, FailureKind>,
    clock_ticks: &Result<u64, FailureKind>,
    observed_at_ms: u64,
) -> ScalarObservation<u64> {
    let result = copied_result(boot_time)
        .and_then(|boot_time| {
            usable_clock_ticks(clock_ticks).map(|clock_ticks| (boot_time, clock_ticks))
        })
        .and_then(|(boot_time, clock_ticks)| {
            boot_time
                .checked_add(start_ticks / clock_ticks)
                .ok_or(FailureKind::ProviderFault)
        });
    result.map_or_else(ScalarObservation::unavailable, |value| {
        ScalarObservation::available(value, observed_at_ms)
    })
}

fn cpu_time_observation(
    user_ticks: u64,
    system_ticks: u64,
    clock_ticks: &Result<u64, FailureKind>,
    observed_at_ms: u64,
) -> ScalarObservation<u64> {
    let result = usable_clock_ticks(clock_ticks).and_then(|clock_ticks| {
        user_ticks
            .checked_add(system_ticks)
            .ok_or(FailureKind::ProviderFault)
            .map(|total| total / clock_ticks)
    });
    result.map_or_else(ScalarObservation::unavailable, |value| {
        ScalarObservation::available(value, observed_at_ms)
    })
}

fn usable_clock_ticks(clock_ticks: &Result<u64, FailureKind>) -> Result<u64, FailureKind> {
    copied_result(clock_ticks).and_then(|clock_ticks| {
        if clock_ticks == 0 {
            Err(FailureKind::ProviderFault)
        } else {
            Ok(clock_ticks)
        }
    })
}

fn copied_result<T: Copy>(result: &Result<T, FailureKind>) -> Result<T, FailureKind> {
    match result {
        Ok(value) => Ok(*value),
        Err(failure) => Err(*failure),
    }
}

fn outcome_from_result<T>(result: &Result<T, FailureKind>) -> SourceOutcome {
    match result {
        Ok(_) => SourceOutcome::Available,
        Err(failure) => SourceOutcome::Unavailable(*failure),
    }
}

fn io_outcome(result: &Result<ProcIoFields, FailureKind>) -> SourceOutcome {
    match result {
        Err(failure) => SourceOutcome::Unavailable(*failure),
        Ok(fields) => result_pair_outcome(&fields.read_bytes, &fields.write_bytes),
    }
}

fn result_pair_outcome<T>(
    left: &Result<T, FailureKind>,
    right: &Result<T, FailureKind>,
) -> SourceOutcome {
    match (left, right) {
        (Ok(_), Ok(_)) => SourceOutcome::Available,
        (Err(left), Err(right)) => SourceOutcome::Unavailable(stronger_failure(*left, *right)),
        (Err(failure), Ok(_)) | (Ok(_), Err(failure)) => SourceOutcome::Partial(*failure),
    }
}

fn rate_outcome(observations: &ProcessScalarObservations) -> SourceOutcome {
    let availability = [
        observations.cpu_percentage.availability(),
        observations.disk_read_bytes_per_sec.availability(),
        observations.disk_write_bytes_per_sec.availability(),
    ];
    let successes = availability
        .iter()
        .filter(|value| value.is_current())
        .count();
    let failure = availability
        .iter()
        .filter_map(|value| value.failure())
        .reduce(stronger_failure);
    match (successes, failure) {
        (3, None) => SourceOutcome::Available,
        (0, Some(failure)) => SourceOutcome::Unavailable(failure),
        (_, Some(failure)) => SourceOutcome::Partial(failure),
        _ => SourceOutcome::Empty,
    }
}

fn stronger_failure(left: FailureKind, right: FailureKind) -> FailureKind {
    if failure_priority(left) >= failure_priority(right) {
        left
    } else {
        right
    }
}

const fn failure_priority(failure: FailureKind) -> u8 {
    match failure {
        FailureKind::RequiresEscalation => 9,
        FailureKind::PermissionDenied => 8,
        FailureKind::MissingDependency => 7,
        FailureKind::TimedOut => 6,
        FailureKind::ProviderFault => 5,
        FailureKind::TemporarilyUnavailable => 4,
        FailureKind::Unsupported => 3,
        FailureKind::IdentityChanged => 2,
        FailureKind::Rejected => 1,
    }
}

fn fd_outcome(result: &Result<FdCount, FailureKind>) -> SourceOutcome {
    match result {
        Ok(FdCount {
            partial_failure: Some(failure),
            ..
        }) => SourceOutcome::Partial(*failure),
        Ok(_) => SourceOutcome::Available,
        Err(failure) => SourceOutcome::Unavailable(*failure),
    }
}

pub(super) fn observe_clock_ticks() -> Result<u64, FailureKind> {
    clock_ticks_per_second()
}
#[cfg(test)]
#[path = "../../../tests/headless/engine/process/observation.rs"]
mod tests;
