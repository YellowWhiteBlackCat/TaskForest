//! Process-manager source and cache regression tests.

use super::*;

#[test]
fn per_process_stat_failures_cannot_become_an_available_source() {
    let mut summary = FieldSourceSummary::default();
    summary.record(SourceOutcome::Available);
    summary.record(SourceOutcome::Unavailable(
        FailureKind::TemporarilyUnavailable,
    ));
    summary.record(SourceOutcome::Partial(FailureKind::PermissionDenied));
    assert_eq!(
        summary.outcome(3),
        SourceOutcome::Partial(FailureKind::PermissionDenied)
    );

    let mut unavailable = FieldSourceSummary::default();
    unavailable.record(SourceOutcome::Unavailable(FailureKind::ProviderFault));
    assert_eq!(
        unavailable.outcome(1),
        SourceOutcome::Unavailable(FailureKind::ProviderFault)
    );
    assert_eq!(
        FieldSourceSummary::default().outcome(0),
        SourceOutcome::Empty
    );
}

#[test]
fn metadata_sources_cannot_report_healthy_when_every_observation_failed() {
    let mut identity = FieldSourceSummary::default();
    let mut label = FieldSourceSummary::default();
    let mut executable = FieldSourceSummary::default();
    for _ in 0..3 {
        identity.record(SourceOutcome::Unavailable(FailureKind::IdentityChanged));
        label.record(SourceOutcome::Unavailable(FailureKind::PermissionDenied));
        executable.record(SourceOutcome::Unavailable(FailureKind::ProviderFault));
    }

    assert_eq!(
        identity.outcome(3),
        SourceOutcome::Unavailable(FailureKind::IdentityChanged)
    );
    assert_eq!(
        label.outcome(3),
        SourceOutcome::Unavailable(FailureKind::PermissionDenied)
    );
    assert_eq!(
        executable.outcome(3),
        SourceOutcome::Unavailable(FailureKind::ProviderFault)
    );

    let mut absent_labels = FieldSourceSummary::default();
    for _ in 0..3 {
        absent_labels.record(SourceOutcome::Empty);
    }
    assert_eq!(absent_labels.outcome(3), SourceOutcome::Empty);
    assert_eq!(absent_labels.populated, 0);
}

#[test]
fn failed_boot_time_source_cannot_make_unknown_identity_look_available() {
    assert_eq!(
        boot_time_outcome(&Err(FailureKind::PermissionDenied), 4),
        SourceOutcome::Unavailable(FailureKind::PermissionDenied)
    );
    assert_eq!(
        boot_time_outcome(&Ok(1_720_000_000), 4),
        SourceOutcome::Available
    );
}

#[test]
fn previous_items_index_matches_the_right_previous_tick_row_for_thousand_plus_pids() {
    let items: Vec<ProcessItem> = (0..1200)
        .map(|index| {
            taskmanager_test_support::ProcessItemFixtureBuilder::new()
                .pid(index + 1)
                .name(format!("worker-{index}"))
                .scalar_observations(taskmanager_core::ProcessScalarObservations {
                    start_token: taskmanager_core::ScalarObservation::available(
                        u64::from(index) + 1000,
                        10,
                    ),
                    ..taskmanager_core::ProcessScalarObservations::default()
                })
                .build()
        })
        .collect();
    let mut previous = PreviousItems::default();
    previous.sync_from(&items);

    for item in &items {
        let matched = previous.find(item.pid).expect("indexed pid must resolve");
        assert_eq!(matched.pid, item.pid);
        assert_eq!(
            matched.name, item.name,
            "previous row must belong to the same pid"
        );
        assert_eq!(
            matched.scalar_observations().start_token.current_value(),
            item.scalar_observations().start_token.current_value(),
            "previous row must carry that pid's last-tick value"
        );
    }
    assert_eq!(previous.find(1_000_000), None, "unknown pid must not match");
    assert_eq!(previous.find(0), None, "pid zero must not match");
}

#[test]
fn previous_items_index_preserves_first_match_semantics_for_duplicate_pids() {
    let items = vec![
        taskmanager_test_support::ProcessItemFixtureBuilder::new()
            .pid(7)
            .name("first".to_owned())
            .build(),
        taskmanager_test_support::ProcessItemFixtureBuilder::new()
            .pid(7)
            .name("second".to_owned())
            .build(),
    ];
    let mut previous = PreviousItems::default();
    previous.sync_from(&items);

    assert_eq!(
        previous.find(7).map(|item| item.name.as_str()),
        Some("first"),
        "the index must keep the linear find() first-match semantics"
    );
}

#[test]
fn passwd_cache_refreshes_only_when_the_interval_expires() {
    let mut cache = PasswdCache::default();
    let loads = std::cell::Cell::new(0);
    let mut load = || {
        loads.set(loads.get() + 1);
        Ok(HashMap::from([(0, "root".to_owned())]))
    };

    let first = cache.labels_or_refresh(1_000, &mut load);
    assert_eq!(loads.get(), 1, "the first refresh must load labels");
    assert_eq!(
        first.as_ref().unwrap().get(&0).map(String::as_str),
        Some("root")
    );

    let cached = cache.labels_or_refresh(1_000 + PASSWD_CACHE_TTL_MS - 1, &mut load);
    assert_eq!(loads.get(), 1, "labels must not be re-read inside the TTL");
    assert_eq!(
        cached.as_ref().unwrap().get(&0).map(String::as_str),
        Some("root")
    );

    let reloaded = cache.labels_or_refresh(1_000 + PASSWD_CACHE_TTL_MS, &mut load);
    assert_eq!(loads.get(), 2, "expired labels must be re-read");
    assert_eq!(
        reloaded.as_ref().unwrap().get(&0).map(String::as_str),
        Some("root")
    );
}

#[test]
fn passwd_cache_retries_failed_loads_on_the_shorter_interval() {
    let mut cache = PasswdCache::default();
    let loads = std::cell::Cell::new(0);
    let mut failing = || {
        loads.set(loads.get() + 1);
        Err(ProcessMetadataFailure::PermissionDenied)
    };

    let first = cache.labels_or_refresh(1_000, &mut failing);
    assert_eq!(loads.get(), 1);
    assert_eq!(*first, Err(ProcessMetadataFailure::PermissionDenied));

    let within_retry = cache.labels_or_refresh(1_000 + PASSWD_CACHE_RETRY_MS - 1, &mut failing);
    assert_eq!(
        loads.get(),
        1,
        "a failed load must not retry before the retry interval"
    );
    assert_eq!(*within_retry, Err(ProcessMetadataFailure::PermissionDenied));

    let retried = cache.labels_or_refresh(1_000 + PASSWD_CACHE_RETRY_MS, &mut failing);
    assert_eq!(
        loads.get(),
        2,
        "a failed load must retry on the short interval"
    );
    assert_eq!(*retried, Err(ProcessMetadataFailure::PermissionDenied));
}

#[test]
fn boot_time_cache_serves_the_cached_value_until_expiry() {
    let mut cache = BootTimeCache::default();
    let loads = std::cell::Cell::new(0);
    let mut boot_time = || {
        loads.set(loads.get() + 1);
        Ok(1_720_000_000)
    };

    assert_eq!(
        cache.value_or_refresh(1_000, &mut boot_time),
        Ok(1_720_000_000)
    );
    assert_eq!(loads.get(), 1, "the first refresh must read boot time");
    assert_eq!(
        cache.value_or_refresh(1_000 + BOOT_TIME_CACHE_TTL_MS - 1, &mut boot_time),
        Ok(1_720_000_000)
    );
    assert_eq!(
        loads.get(),
        1,
        "boot time must not be re-read inside the TTL"
    );
    assert_eq!(
        cache.value_or_refresh(1_000 + BOOT_TIME_CACHE_TTL_MS, &mut boot_time),
        Ok(1_720_000_000)
    );
    assert_eq!(loads.get(), 2, "expired boot time must be re-read");
}

#[test]
fn boot_time_cache_retries_failed_reads_on_the_shorter_interval_and_recovers() {
    let mut cache = BootTimeCache::default();
    let loads = std::cell::Cell::new(0);
    let mut failing = || {
        loads.set(loads.get() + 1);
        Err(FailureKind::ProviderFault)
    };

    assert_eq!(
        cache.value_or_refresh(1_000, &mut failing),
        Err(FailureKind::ProviderFault)
    );
    assert_eq!(loads.get(), 1);
    assert_eq!(
        cache.value_or_refresh(1_000 + BOOT_TIME_CACHE_RETRY_MS - 1, &mut failing),
        Err(FailureKind::ProviderFault)
    );
    assert_eq!(
        loads.get(),
        1,
        "a failed read must not retry before the retry interval"
    );
    assert_eq!(
        cache.value_or_refresh(1_000 + BOOT_TIME_CACHE_RETRY_MS, &mut failing),
        Err(FailureKind::ProviderFault)
    );
    assert_eq!(
        loads.get(),
        2,
        "a failed read must retry on the short interval"
    );

    let recovered = std::cell::Cell::new(0);
    let mut recovering = || {
        recovered.set(recovered.get() + 1);
        Ok(42)
    };
    assert_eq!(
        cache.value_or_refresh(1_000 + BOOT_TIME_CACHE_RETRY_MS * 2 + 1, &mut recovering),
        Ok(42)
    );
    assert_eq!(recovered.get(), 1);
    assert_eq!(
        cache.value_or_refresh(1_000 + BOOT_TIME_CACHE_RETRY_MS * 2 + 2, &mut recovering),
        Ok(42)
    );
    assert_eq!(
        recovered.get(),
        1,
        "a recovered boot time must be cached like a fresh one"
    );
}

/// On-box receipt for the round-2 fd-decimation source-stability fix.
///
/// Round-1 B5 decimated per-process `/proc/<pid>/fd` scans to every Nth tick
/// (`FD_COUNT_REFRESH_EVERY_N_TICKS`). Round-2 changed the skip-tick fd source
/// outcome from `Empty` to retained `Available` (when a prior value exists) so
/// the aggregate `PROCESS_FD_PROVIDER` stops flapping 1-Available : 4-Empty per
/// decimation cycle. That per-row fix is unit-tested in `observation::tests`;
/// this test drives the REAL `ProcessManager::refresh` path across many ticks
/// (2× the decimation window + a 3-tick margin, spanning several skip-ticks and
/// two full-read boundaries) against a live process — the test's own pid, whose
/// `/proc/<pid>/fd` is always self-readable — and asserts end-to-end:
///   * the aggregate `PROCESS_FD_PROVIDER` source NEVER drops to `Empty` across
///     the skip-ticks (the pre-round-2 bug flapped it to Empty on 4-of-5 ticks);
///   * our own pid's per-row fd count is retained across every skip-tick as a
///     `Stale` last-known value equal to the last full-read count (concrete
///     end-to-end proof of retain-Available, not just a per-row unit fixture).
///
/// `#[ignore]` because it is host-dependent (reads the live `/proc` tree). Run:
/// `cargo nextest run --locked -p taskmanager-platform-linux --all-targets -j 4 \
/// -E 'test(fd_source_stays_available_across_decimation_skip_ticks_on_box)' \
/// --run-ignored only --no-capture`
#[ignore = "host-dependent: drives the live /proc process collector against the test's own pid"]
#[test]
fn fd_source_stays_available_across_decimation_skip_ticks_on_box() {
    use taskmanager_core::ScalarAvailability;

    let own_pid = std::process::id();
    let window = FD_COUNT_REFRESH_EVERY_N_TICKS;
    // 2 full decimation windows + a 3-tick margin. With N=5 this is 13 ticks:
    // full(0) deferred(1..=4) full(5) deferred(6..=9) full(10) deferred(11..=12)
    // — three full reads and ten skip-ticks spanning two decimation boundaries.
    let tick_count = (usize::try_from(window).unwrap_or(1))
        .saturating_mul(2)
        .saturating_add(3);

    let mut manager = ProcessManager::new();
    let mut aggregate_outcomes: Vec<SourceOutcome> = Vec::with_capacity(tick_count);
    let mut own_row_avail: Vec<ScalarAvailability> = Vec::with_capacity(tick_count);
    let mut own_row_values: Vec<Option<u32>> = Vec::with_capacity(tick_count);
    let mut own_pid_missing = 0usize;

    for tick in 0..tick_count {
        let snapshot: PartialSourceSnapshot<ProcessItem> = manager.refresh();

        let fd_source = snapshot
            .sources
            .iter()
            .find(|status| status.provider == PROCESS_FD_PROVIDER)
            .map(|status| status.outcome)
            .unwrap_or(SourceOutcome::Empty);
        aggregate_outcomes.push(fd_source);

        match snapshot.items.iter().find(|item| item.pid == own_pid) {
            Some(item) => {
                own_row_avail.push(item.scalar_observations().fds.availability());
                own_row_values.push(item.scalar_observations().fds.last_known_value().copied());
            }
            None => {
                own_pid_missing = own_pid_missing.saturating_add(1);
                own_row_avail.push(ScalarAvailability::Unknown);
                own_row_values.push(None);
            }
        }

        let want_read = (tick as u32).rem_euclid(window) == 0;
        let value_label = own_row_values
            .last()
            .copied()
            .flatten()
            .map(|value| value.to_string())
            .unwrap_or_else(|| "-".to_owned());
        eprintln!(
            "[fd-decimation on-box] pid={own_pid} tick={tick:02}/{tick_count} want_fd_read={want_read} \
             aggregate_fd_source={fd_source:?} own_row_fds={:?} own_fd_last_known={value_label}",
            own_row_avail.last().copied().unwrap_or_default(),
        );
    }

    // (1) The test's own pid must be enumerated on every tick — otherwise the
    // per-row retain assertions below would be vacuous.
    assert_eq!(
        own_pid_missing, 0,
        "own pid {own_pid} dropped out of the live /proc scan on {own_pid_missing} ticks"
    );

    // (2) The aggregate PROCESS_FD_PROVIDER source must NEVER be Empty on any
    // tick — the headline round-2 guarantee. Pre-round-2 every deferred tick
    // produced aggregate Empty (4-of-5 ticks flapped Available/Empty).
    let empty_ticks: Vec<usize> = aggregate_outcomes
        .iter()
        .enumerate()
        .filter(|(_, outcome)| matches!(outcome, SourceOutcome::Empty))
        .map(|(index, _)| index)
        .collect();
    assert!(
        empty_ticks.is_empty(),
        "round-2 retain-Available regression: PROCESS_FD_PROVIDER went Empty on \
         ticks {empty_ticks:?} (full per-tick outcomes = {aggregate_outcomes:?}); \
         a deferred tick must keep the aggregate fd source non-Empty"
    );

    // (3) The deferred skip-ticks specifically must be non-Empty (direct
    // end-to-end proof the round-2 retain-Available holds across decimation
    // boundaries, not just on full-read ticks).
    for (tick, outcome) in aggregate_outcomes.iter().enumerate() {
        if (tick as u32).rem_euclid(window) != 0 {
            assert_ne!(
                *outcome,
                SourceOutcome::Empty,
                "deferred tick {tick}: aggregate fd source must not toggle back to Empty"
            );
        }
    }

    // (4) Per-row evidence for our own pid: tick 0 is a full read, so a real fd
    // count is established (a live test process always holds stdin/stdout/stderr
    // plus the read_dir handle — never 0). Every subsequent tick must carry a
    // retained-or-fresh count (never None, never a fabricated 0). Within each
    // decimation window the deferred ticks must reuse the window's full-read
    // count exactly (retain_previous chains the value); the full-read value may
    // legitimately drift by a couple of fds across windows (the process can
    // open/close fds between scans), so only intra-window equality is asserted.
    let first_value =
        own_row_values[0].expect("tick 0 is a full fd read of self and must produce a fresh count");
    assert_ne!(
        first_value, 0,
        "a live test process always has open fds (stdin/stdout/stderr at minimum)"
    );
    let mut last_full_value: Option<u32> = None;
    for (tick, (availability, value)) in own_row_avail.iter().zip(own_row_values.iter()).enumerate()
    {
        let is_full_read = (tick as u32).rem_euclid(window) == 0;
        assert!(
            value.is_some(),
            "tick {tick}: own-pid fd last_known_value must always be present after the first read"
        );
        if is_full_read {
            last_full_value = *value;
            assert!(
                availability.is_current(),
                "full-read tick {tick}: own-pid fd availability must be current (freshly read), got {availability:?}"
            );
        } else {
            assert_eq!(
                *value, last_full_value,
                "deferred tick {tick}: own-pid fd value must be the retained last full-read count (no fabrication, no drift mid-window)"
            );
            assert!(
                matches!(availability, ScalarAvailability::Stale(_)),
                "deferred tick {tick}: own-pid fd availability must be Stale (retained last-known), got {availability:?}"
            );
        }
    }
}
