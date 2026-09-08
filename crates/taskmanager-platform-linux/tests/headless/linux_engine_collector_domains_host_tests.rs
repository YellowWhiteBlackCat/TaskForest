use super::*;
use taskmanager_core::{ScalarAvailability, SystemObservationState};

fn fixture_root(name: &str) -> PathBuf {
    crate::test_support::repo_temp_dir().join(format!(
        "taskmanager-host-domain-{}-{name}",
        std::process::id()
    ))
}

#[test]
fn three_host_scalars_retain_independent_truth() {
    let root = fixture_root("partial");
    fs::create_dir_all(root.join("42")).expect("create process fixture");
    fs::write(root.join("uptime"), "123.50 0.00\n").expect("write uptime");
    fs::write(root.join("loadavg"), "2.00 1.00 0.50 1/10 42\n").expect("write loadavg");
    fs::write(root.join("42/stat"), "42 (fixture) malformed\n").expect("write bad stat");
    let mut collector =
        LinuxHostTelemetryCollector::with_proc_root_and_processor_counts(root.clone(), 4, None);

    let observed = collector.observe(10);
    let facts = observed
        .current_value()
        .expect("uptime and process inventory remain current");

    assert_eq!(facts.uptime_secs.current_value(), Some(&123));
    assert_eq!(facts.processes.current_value(), Some(&1));
    let load = facts
        .load_average
        .current_value()
        .expect("load average fixture is current");
    assert_eq!(load.logical_processors, 4);
    assert_eq!(load.one_minute, 2.0);
    assert_eq!(load.normalized_one_minute, 0.5);
    assert!(facts.threads.current_value().is_none());
    assert_eq!(
        facts.threads.availability(),
        ScalarAvailability::Unavailable(FailureKind::ProviderFault)
    );
    assert!(matches!(
        observed.state(),
        SystemObservationState::Partial { .. }
    ));
    fs::remove_dir_all(root).expect("remove fixture");
}

#[test]
fn complete_host_failure_is_stale_only_after_a_real_success() {
    let root = fixture_root("stale");
    fs::create_dir_all(&root).expect("create proc fixture");
    fs::write(root.join("uptime"), "0.25 0.00\n").expect("write measured zero");
    fs::write(root.join("loadavg"), "0.00 0.00 0.00 0/0 0\n").expect("write idle loadavg");
    let mut collector =
        LinuxHostTelemetryCollector::with_proc_root_and_processor_counts(root.clone(), 4, None);
    let first = collector.observe(10);
    let first_facts = first.current_value().expect("empty proc is authoritative");
    assert_eq!(first_facts.uptime_secs.current_value(), Some(&0));
    assert_eq!(first_facts.processes.current_value(), Some(&0));
    assert_eq!(first_facts.threads.current_value(), Some(&0));

    fs::remove_dir_all(&root).expect("remove proc fixture");
    let failed = collector.observe(20);

    assert!(matches!(
        failed.state(),
        SystemObservationState::Stale {
            last_success_ms: 10,
            ..
        }
    ));
    assert!(failed.current_value().is_none());
    assert!(failed.last_known_value().is_some());
}

#[test]
fn load_average_uses_physical_core_denominator_when_provider_proves_it() {
    let load = parse_load_average("8.00 4.00 2.00 1/8 42\n", 8, Some(4))
        .expect("load fixture should parse");
    assert_eq!(load.normalized_one_minute, 2.0);
    assert_eq!(load.physical_cores, Some(4));
    assert_eq!(load.normalization_processors(), 4);
}
