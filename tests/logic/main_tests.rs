//! Tests for the binary entry point: argv dispatch into the compiled-in
//! frontend modes and the live end-to-end JSON snapshot honesty proofs.

use super::*;

#[test]
fn no_args_launches_the_gui() {
    assert_eq!(
        cli::parse_args(Vec::<String>::new()),
        Ok(CliMode::Gui {
            app_id: None,
            demo: false
        })
    );
    assert_eq!(
        cli::parse_args(["--json".to_string()]),
        Ok(CliMode::JsonSnapshot)
    );
}

#[test]
fn live_json_snapshot_collects_valid_typed_json() {
    // End-to-end: spawn the real native runtime (this module is the only
    // place allowed to), drive one collection cycle, and assert the JSON
    // shape + the honesty invariant on the live host. On a normal Linux
    // machine every /proc-fed domain reports within milliseconds; the
    // generous timeout only guards a wedged provider. The runtime is spawned
    // through the native seam directly — the app-host production constructor
    // (real user config/history paths) is never touched by tests.
    let client = taskmanager_platform_native::NativePlatformRuntime::spawn()
        .map(taskmanager_application::PlatformClient::new)
        .expect("native runtime spawns on a Linux host with /proc");
    let json = cli::collect_json_snapshot_from_client(client, std::time::Duration::from_secs(10))
        .expect("collects a complete six-domain snapshot on this host");

    let value: serde_json::Value = serde_json::from_str(&json).expect("collected JSON must parse");
    let snapshot = value
        .get("snapshot")
        .expect("collected JSON must carry a snapshot envelope");
    for key in [
        "timestamp_ms",
        "cpu",
        "memory",
        "disks",
        "networks",
        "gpu",
        "uptime_secs",
    ] {
        assert!(
            snapshot.get(key).is_some(),
            "collected snapshot must include top-level domain {key}"
        );
    }
    // The process list is always an array (possibly empty when permission
    // denied enumerating /proc, but never absent).
    assert!(
        value["processes"].is_array(),
        "collected JSON must carry a processes array"
    );
    // Honesty check: an unobserved CPU temperature must round-trip as null,
    // never as a fabricated 0; an observed one must be a JSON number.
    match snapshot["cpu"].get("temperature_c") {
        None | Some(serde_json::Value::Null) => {
            // Explicitly assert the honest null survives end-to-end: the
            // earlier `unavailable_fields_serialize_as_null_not_zero` unit
            // test proves the serializer's null rule in isolation; this
            // asserts the LIVE path didn't fabricate a 0 either.
            assert!(
                snapshot["cpu"]["temperature_c"].is_null(),
                "an unobserved temperature must serialize as JSON null, not a fabricated number"
            );
        }
        Some(observed) => {
            observed
                .as_f64()
                .expect("an observed CPU temperature must be a JSON number");
        }
    }
    // Stronger honesty guard: CPU package power (`cpu_power_w`) requires
    // RAPL access (`/sys/class/powercap/intel-rapl/.../energy_uj`), which is
    // permission-denied to unprivileged code on this host and virtually every
    // CI runner. The field MUST serialize as JSON null — a fabricated 0 here
    // would mean someone replaced an unavailable Option with a zero on the
    // live path, and this assertion would fail. This is the leaf-level proof
    // the temperature check above cannot give (temperature may genuinely be
    // observed on some hosts, so it can only assert null-vs-number, not null
    // outright).
    assert!(
        snapshot["cpu"]["cpu_power_w"].is_null(),
        "cpu_power_w must serialize as JSON null when RAPL is unavailable, not a fabricated 0"
    );
}
