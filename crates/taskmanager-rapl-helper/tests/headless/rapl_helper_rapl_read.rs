use super::*;
use crate::test_support::repo_temp_dir;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// A unique fixture powercap root under the repository `.tmp/` scratch.
fn fixture_root(tag: &str) -> PathBuf {
    repo_temp_dir().join(format!("tm_rapl_{tag}"))
}

/// Write one `intel-rapl:<index>` package directory with its three files.
fn write_package(root: &Path, index: u32, name: &str, max_range_uj: u64, energy_uj: u64) {
    let dir = root.join(format!("intel-rapl:{index}"));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("name"), format!("{name}\n")).unwrap();
    fs::write(dir.join("max_energy_range_uj"), format!("{max_range_uj}\n")).unwrap();
    fs::write(dir.join("energy_uj"), format!("{energy_uj}\n")).unwrap();
}

/// Rewrite a package's energy counter (what the test's injected pause does in
/// place of the real counter advancing across the sleep).
fn rewrite_energy(root: &Path, index: u32, energy_uj: u64) {
    fs::write(
        root.join(format!("intel-rapl:{index}")).join("energy_uj"),
        format!("{energy_uj}\n"),
    )
    .unwrap();
}

fn package_powers(outcome: ReadOutcome) -> Vec<(u32, String, f32, u64)> {
    match outcome {
        ReadOutcome::Packages { packages } => packages
            .into_iter()
            .map(|package| {
                (
                    package.index,
                    package.name,
                    package.power_w,
                    package.energy_delta_uj,
                )
            })
            .collect(),
        ReadOutcome::Error(error) => {
            panic!("expected packages, got {:?}: {}", error.kind, error.detail)
        }
    }
}

fn error_kind(outcome: ReadOutcome) -> ReadError {
    match outcome {
        ReadOutcome::Error(error) => error,
        ReadOutcome::Packages { packages } => {
            panic!("expected an error, got {} packages", packages.len())
        }
    }
}

#[test]
fn forward_counter_delta_becomes_watts() {
    let root = fixture_root("delta");
    fs::create_dir_all(&root).unwrap();
    write_package(&root, 0, "Core", 65_000_000, 1_000_000);
    let mut rewrite = || rewrite_energy(&root, 0, 2_500_000);

    let powers = package_powers(sample_packages_with_pause(&root, 1000, &mut rewrite));
    assert_eq!(powers.len(), 1);
    let (index, name, power_w, delta) = &powers[0];
    assert_eq!(*index, 0);
    assert_eq!(name, "Core", "the sysfs name is trimmed");
    assert_eq!(*delta, 1_500_000);
    assert!(
        (power_w - 1.5).abs() < 1e-6,
        "1_500_000 uJ over 1 s = 1.5 W"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn wrapped_counter_uses_the_advertised_range() {
    let root = fixture_root("wrap");
    fs::create_dir_all(&root).unwrap();
    // Counter at 900_000 wraps at 1_000_000 and restarts at 100_000: the true
    // delta is (1_000_000 - 900_000) + 100_000.
    write_package(&root, 0, "Core", 1_000_000, 900_000);
    let mut rewrite = || rewrite_energy(&root, 0, 100_000);

    let powers = package_powers(sample_packages_with_pause(&root, 1000, &mut rewrite));
    assert_eq!(powers.len(), 1);
    let (_, _, power_w, delta) = &powers[0];
    assert_eq!(*delta, 200_000);
    assert!((power_w - 0.2).abs() < 1e-6, "200_000 uJ over 1 s = 0.2 W");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn wrapped_counter_without_range_is_skipped_not_guessed() {
    let root = fixture_root("wrap_no_range");
    fs::create_dir_all(&root).unwrap();
    write_package(&root, 0, "Core", 0, 900_000);
    let mut rewrite = || rewrite_energy(&root, 0, 100_000);

    let powers = package_powers(sample_packages_with_pause(&root, 1000, &mut rewrite));
    assert!(
        powers.is_empty(),
        "an unknowable delta must drop the package, not guess zero"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn packages_are_sorted_by_index_across_creation_order() {
    let root = fixture_root("sorting");
    fs::create_dir_all(&root).unwrap();
    for (index, energy) in [(2u32, 1_000u64), (0, 2_000), (1, 3_000)] {
        write_package(&root, index, "Core", 65_000_000, energy);
    }
    let mut rewrite = || {
        rewrite_energy(&root, 2, 2_000);
        rewrite_energy(&root, 0, 4_000);
        rewrite_energy(&root, 1, 6_000);
    };

    let powers = package_powers(sample_packages_with_pause(&root, 1000, &mut rewrite));
    let indexes: Vec<u32> = powers.iter().map(|(index, ..)| *index).collect();
    assert_eq!(indexes, [0, 1, 2]);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn only_top_level_packages_are_sampled() {
    let root = fixture_root("toplevel");
    fs::create_dir_all(&root).unwrap();
    write_package(&root, 0, "Core", 65_000_000, 1_000);
    // A sub-domain package (core/uncore/dram child) and unrelated class
    // entries must not be sampled as packages.
    fs::create_dir_all(root.join("intel-rapl:0/intel-rapl:0:0")).unwrap();
    fs::create_dir_all(root.join("thermal_zone0")).unwrap();
    let mut rewrite = || rewrite_energy(&root, 0, 2_000);

    let powers = package_powers(sample_packages_with_pause(&root, 1000, &mut rewrite));
    assert_eq!(powers.len(), 1, "sub-domains and foreign dirs are skipped");
    assert_eq!(powers[0].0, 0);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn missing_root_is_no_rapl() {
    let error = error_kind(sample_packages_with_pause(
        &fixture_root("absent"),
        1000,
        &mut || {},
    ));
    assert_eq!(error.kind, ErrorKindJson::NoRapl);
    assert!(
        error.detail.contains("absent"),
        "detail names the missing root: {}",
        error.detail
    );
}

#[test]
fn present_root_without_top_level_packages_is_no_rapl() {
    for tag in ["empty", "junk"] {
        let root = fixture_root(tag);
        fs::create_dir_all(root.join("cpu0")).unwrap();
        fs::create_dir_all(root.join("intel-rapl:0:0")).unwrap();

        let error = error_kind(sample_packages_with_pause(&root, 1000, &mut || {}));
        assert_eq!(error.kind, ErrorKindJson::NoRapl, "tag {tag}");
        let _ = fs::remove_dir_all(&root);
    }
}

#[test]
fn missing_counter_file_is_read_failed() {
    let root = fixture_root("missing_counter");
    fs::create_dir_all(&root).unwrap();
    let dir = root.join("intel-rapl:0");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("name"), "Core\n").unwrap();
    fs::write(dir.join("max_energy_range_uj"), "65000000\n").unwrap();
    // No energy_uj.

    let error = error_kind(sample_packages_with_pause(&root, 1000, &mut || {}));
    assert_eq!(error.kind, ErrorKindJson::ReadFailed);
    assert!(
        error.detail.contains("energy_uj"),
        "detail names the missing counter: {}",
        error.detail
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn unparsable_counter_content_is_read_failed() {
    let root = fixture_root("unparsable");
    fs::create_dir_all(&root).unwrap();
    let dir = root.join("intel-rapl:0");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("name"), "Core\n").unwrap();
    fs::write(dir.join("max_energy_range_uj"), "65000000\n").unwrap();
    fs::write(dir.join("energy_uj"), "not-a-number\n").unwrap();

    let error = error_kind(sample_packages_with_pause(&root, 1000, &mut || {}));
    assert_eq!(error.kind, ErrorKindJson::ReadFailed);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn denied_energy_counter_is_permission_denied() {
    let root = fixture_root("denied");
    fs::create_dir_all(&root).unwrap();
    write_package(&root, 0, "Core", 65_000_000, 1_000);
    let energy = root.join("intel-rapl:0").join("energy_uj");
    fs::set_permissions(&energy, fs::Permissions::from_mode(0o000)).unwrap();

    let outcome = sample_packages_with_pause(&root, 1000, &mut || {});
    let _ = fs::set_permissions(&energy, fs::Permissions::from_mode(0o644));
    let error = match outcome {
        ReadOutcome::Error(error) => error,
        ReadOutcome::Packages { .. } => {
            // Root bypasses DAC modes; the denial classification is
            // observable only on unprivileged runners.
            eprintln!("SKIP: running privileged; the 0o000 energy file still read");
            let _ = fs::remove_dir_all(&root);
            return;
        }
    };
    assert_eq!(error.kind, ErrorKindJson::PermissionDenied);
    assert!(
        error.detail.contains("energy_uj"),
        "detail names the denied counter: {}",
        error.detail
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_failure_on_the_second_read_is_a_typed_error() {
    let root = fixture_root("second_read");
    fs::create_dir_all(&root).unwrap();
    write_package(&root, 0, "Core", 65_000_000, 1_000);
    let mut break_counter = || {
        let _ = fs::remove_file(root.join("intel-rapl:0").join("energy_uj"));
    };

    let error = error_kind(sample_packages_with_pause(&root, 1000, &mut break_counter));
    assert_eq!(error.kind, ErrorKindJson::ReadFailed);
    let _ = fs::remove_dir_all(&root);
}

/// The pure reduction, independent of any filesystem: exact rate math over a
/// shorter window, zero delta, and the unknowable/zero-window skips.
#[test]
fn compute_package_power_pure_rate_math() {
    let before = PackageState {
        index: 3,
        name: "Core".to_owned(),
        max_energy_range_uj: 65_000_000,
        energy_uj: 1_000_000,
    };
    let after = PackageState {
        energy_uj: 3_000_000,
        ..before.clone()
    };
    let power = compute_package_power(&before, &after, 500).expect("forward delta");
    assert_eq!(power.index, 3);
    assert_eq!(power.name, "Core");
    assert_eq!(power.energy_delta_uj, 2_000_000);
    assert!(
        (power.power_w - 4.0).abs() < 1e-6,
        "2_000_000 uJ over 0.5 s = 4 W"
    );

    let unchanged = compute_package_power(&before, &before.clone(), 1000).expect("zero delta");
    assert_eq!(unchanged.power_w, 0.0);
    assert_eq!(unchanged.energy_delta_uj, 0);

    // A zero window cannot form a rate: skip rather than emit inf/NaN.
    assert_eq!(compute_package_power(&before, &after, 0), None);

    // Wrapped with no advertised range: unknowable.
    let blind = PackageState {
        max_energy_range_uj: 0,
        ..before.clone()
    };
    let rewound = PackageState {
        energy_uj: 0,
        ..blind.clone()
    };
    assert_eq!(compute_package_power(&blind, &rewound, 1000), None);
}

/// The production entry point (real sleep) against a static fixture: the
/// counter does not move, so the honest reading is exactly zero watts. A
/// 10 ms window keeps the test fast; nothing is asserted about wall time.
#[test]
fn real_sleep_sampling_reads_a_static_counter_as_zero() {
    let root = fixture_root("real_sleep");
    fs::create_dir_all(&root).unwrap();
    write_package(&root, 0, "Core", 65_000_000, 42_000_000);

    let powers = package_powers(sample_packages(&root, 10));
    assert_eq!(powers.len(), 1);
    let (index, _, power_w, delta) = &powers[0];
    assert_eq!(*index, 0);
    assert_eq!(*delta, 0, "the fixture counter never advances");
    assert_eq!(*power_w, 0.0);
    assert!(power_w.is_finite() && *power_w >= 0.0);
    let _ = fs::remove_dir_all(&root);
}

/// The snapshot reader on its own: sorted, trimmed name, parsed counters.
#[test]
fn read_package_states_parses_and_sorts() {
    let root = fixture_root("states");
    fs::create_dir_all(&root).unwrap();
    write_package(&root, 1, "Core", 61_000_000, 7);
    write_package(&root, 0, "Package 0\n", 65_000_000, 5);

    let states = read_package_states(&root).expect("states");
    assert_eq!(states.len(), 2);
    assert_eq!(states[0].index, 0);
    assert_eq!(states[0].name, "Package 0", "name file content is trimmed");
    assert_eq!(states[0].max_energy_range_uj, 65_000_000);
    assert_eq!(states[0].energy_uj, 5);
    assert_eq!(states[1].index, 1);
    let _ = fs::remove_dir_all(&root);
}
