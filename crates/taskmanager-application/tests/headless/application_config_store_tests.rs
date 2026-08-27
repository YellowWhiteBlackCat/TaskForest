use std::sync::atomic::{AtomicU64, Ordering};

use taskmanager_core::config::{
    ColumnWidthConfig, ProcessViewPresetConfig, STARTUP_PAGE_PROCESSES, STARTUP_PAGE_REMEMBER,
};

use super::*;

fn test_path(label: &str) -> PathBuf {
    static NEXT_PATH: AtomicU64 = AtomicU64::new(0);
    let sequence = NEXT_PATH.fetch_add(1, Ordering::Relaxed);
    crate::test_support::repo_temp_dir()
        .join(format!(
            "taskmanager-config-store-{label}-{}-{sequence}",
            std::process::id()
        ))
        .join("nested")
        .join("config.json")
}

#[test]
fn missing_and_malformed_files_fall_back_without_fabricating_success() {
    let path = test_path("fallback");
    let store = ConfigStore::new(&path);
    assert_eq!(store.load_or_default(), Config::default());
    assert_eq!(
        store.load().unwrap_err().kind(),
        ConfigStoreErrorKind::Missing
    );

    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "{ invalid json").unwrap();
    assert_eq!(store.load_or_default(), Config::default());
    assert_eq!(
        store.load().unwrap_err().kind(),
        ConfigStoreErrorKind::Decode
    );
    std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).unwrap();
}

#[test]
fn injected_path_creates_parent_and_round_trips_config() {
    let path = test_path("round-trip");
    let store = ConfigStore::new(&path);
    let config = Config {
        skin: "KDE".into(),
        mode: "Dark".into(),
        hc: true,
        show_memory: false,
        refresh_ms: 2500,
        last_page: "apps".into(),
        process_col_widths: vec![
            ColumnWidthConfig {
                column: "Memory".into(),
                width: 200.0,
            },
            ColumnWidthConfig {
                column: "CPU".into(),
                width: 40.0,
            },
        ],
        saved_process_views: vec![ProcessViewPresetConfig::new(
            "Investigation".into(),
            String::new(),
            String::new(),
            false,
            Vec::new(),
        )],
        ..Config::default()
    };

    store.save(&config).unwrap();
    assert_eq!(store.path(), path);
    drop(store);
    let clean_store = ConfigStore::new(&path);
    assert_eq!(clean_store.load().unwrap(), config);
    std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).unwrap();
}

#[test]
fn recovery_prefers_last_known_good_backup_over_a_corrupt_primary() {
    let path = test_path("recovery");
    let store = ConfigStore::new(&path);
    let first = Config {
        language: Some("en".to_string()),
        ..Config::default()
    };
    store.save(&first).unwrap();
    let second = Config {
        language: Some("zh".to_string()),
        ..first.clone()
    };
    store.save(&second).unwrap();
    let third = Config {
        language: Some("ja".to_string()),
        ..second.clone()
    };
    store.save(&third).unwrap();

    std::fs::write(&path, "{ damaged").unwrap();
    let recovered = store.load_with_recovery();
    assert_eq!(recovered.source(), ConfigLoadSource::Backup);
    assert_eq!(recovered.config().language.as_deref(), Some("zh"));
    assert_eq!(
        recovered.primary_error(),
        Some(ConfigStoreErrorKind::Decode)
    );
    assert_eq!(recovered.backup_error(), None);

    std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).unwrap();
}

#[test]
fn stale_background_generation_cannot_overwrite_newer_preferences() {
    let path = test_path("generation");
    let store = ConfigStore::new(&path);
    let old_generation = store.next_save_generation();
    let new_generation = store.next_save_generation();
    let old = Config {
        language: Some("en".to_string()),
        ..Config::default()
    };
    let new = Config {
        language: Some("zh".to_string()),
        ..old.clone()
    };

    store.save_at(&new, new_generation).unwrap();
    store.save_at(&old, old_generation).unwrap();
    assert_eq!(store.load().unwrap().language.as_deref(), Some("zh"));

    std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).unwrap();
}

#[test]
fn reserved_snapshot_keeps_its_base_when_another_clone_loads_newer_disk_state() {
    let path = test_path("reservation-base");
    ConfigStore::new(&path)
        .save(&Config::default())
        .expect("seed config");
    let periodic_writer = ConfigStore::new(&path);
    let mut periodic = periodic_writer.load().expect("periodic base");
    periodic.last_page = "apps".to_owned();
    let generation = periodic_writer.next_save_generation();

    let settings_writer = ConfigStore::new(&path);
    let mut settings = settings_writer.load().expect("settings base");
    settings.ui_size = "Large".to_owned();
    settings_writer.save(&settings).expect("settings commit");

    periodic_writer
        .load()
        .expect("shared clone observes the settings commit");
    periodic_writer
        .save_at(&periodic, generation)
        .expect("reserved periodic commit");

    let merged = ConfigStore::new(&path).load().expect("merged config");
    assert_eq!(merged.last_page, "apps");
    assert_eq!(merged.ui_size, "Large");
    std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).unwrap();
}

#[test]
fn independent_writers_merge_disjoint_fields_in_either_commit_order() {
    for (label, language_first) in [("language-first", true), ("size-first", false)] {
        let path = test_path(label);
        let seed = ConfigStore::new(&path);
        seed.save(&Config::default()).expect("seed config");

        let language_writer = ConfigStore::new(&path);
        let size_writer = ConfigStore::new(&path);
        let mut language = language_writer.load().expect("language base");
        let mut size = size_writer.load().expect("size base");
        language.language = Some("zh".to_owned());
        size.ui_size = "Large".to_owned();

        if language_first {
            language_writer.save(&language).expect("language commit");
            size_writer.save(&size).expect("size commit");
        } else {
            size_writer.save(&size).expect("size commit");
            language_writer.save(&language).expect("language commit");
        }

        let merged = ConfigStore::new(&path).load().expect("merged config");
        assert_eq!(merged.language.as_deref(), Some("zh"));
        assert_eq!(merged.ui_size, "Large");
        std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).unwrap();
    }
}

#[test]
fn independent_writers_resolve_the_same_field_by_commit_order() {
    for (label, english_first, expected) in [
        ("same-field-zh-last", true, "zh"),
        ("same-field-en-last", false, "en"),
    ] {
        let path = test_path(label);
        ConfigStore::new(&path)
            .save(&Config::default())
            .expect("seed config");
        let english_writer = ConfigStore::new(&path);
        let chinese_writer = ConfigStore::new(&path);
        let mut english = english_writer.load().expect("english base");
        let mut chinese = chinese_writer.load().expect("chinese base");
        english.language = Some("en".to_owned());
        chinese.language = Some("zh".to_owned());

        if english_first {
            english_writer.save(&english).expect("english commit");
            chinese_writer.save(&chinese).expect("chinese commit");
        } else {
            chinese_writer.save(&chinese).expect("chinese commit");
            english_writer.save(&english).expect("english commit");
        }

        assert_eq!(
            ConfigStore::new(&path)
                .load()
                .expect("last committed config")
                .language
                .as_deref(),
            Some(expected)
        );
        std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).unwrap();
    }
}

#[test]
fn unchanged_periodic_snapshot_never_reverts_a_remote_field() {
    let path = test_path("periodic-unchanged");
    ConfigStore::new(&path)
        .save(&Config::default())
        .expect("seed config");
    let periodic_writer = ConfigStore::new(&path);
    let settings_writer = ConfigStore::new(&path);
    let mut periodic = periodic_writer.load().expect("periodic base");
    let mut settings = settings_writer.load().expect("settings base");
    periodic.last_page = "apps".to_owned();
    settings.ui_size = "Large".to_owned();

    periodic_writer.save(&periodic).expect("periodic commit");
    settings_writer.save(&settings).expect("settings commit");
    periodic_writer
        .save(&periodic)
        .expect("unchanged periodic commit");

    let merged = ConfigStore::new(&path).load().expect("merged config");
    assert_eq!(merged.last_page, "apps");
    assert_eq!(merged.ui_size, "Large");
    std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).unwrap();
}

#[test]
fn os_writer_lock_timeout_is_typed_without_touching_config() {
    let path = test_path("writer-lock");
    let store = ConfigStore::new(&path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let lock_path = store.lock_path();
    let holder = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .expect("open lock holder");
    holder.try_lock().expect("acquire fixture lock");

    let error = acquire_config_lock(&lock_path, Duration::ZERO).unwrap_err();
    assert_eq!(error.kind(), ConfigStoreErrorKind::Lock);
    assert!(!path.exists());

    drop(holder);
    std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).unwrap();
}

#[test]
fn reserved_newer_generation_blocks_an_older_delayed_snapshot() {
    let path = test_path("reserved-generation");
    let store = ConfigStore::new(&path);
    let old_generation = store.next_save_generation();
    let _new_generation = store.next_save_generation();

    assert_eq!(store.save_at(&Config::default(), old_generation), Ok(()));
    assert_eq!(
        store.load().unwrap_err().kind(),
        ConfigStoreErrorKind::Missing
    );

    let _ = std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
}

#[test]
fn oversized_primary_is_rejected_without_unbounded_read() {
    let path = test_path("oversized");
    let store = ConfigStore::new(&path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let oversized = vec![b' '; MAX_CONFIG_BYTES.saturating_add(1)];
    std::fs::write(&path, oversized).unwrap();
    assert_eq!(
        store.load().unwrap_err().kind(),
        ConfigStoreErrorKind::TooLarge
    );
    assert_eq!(
        store.load_with_recovery().source(),
        ConfigLoadSource::Default
    );
    std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).unwrap();
}

#[test]
fn oversized_existing_primary_is_not_copied_into_a_backup() {
    let path = test_path("oversized-existing");
    let store = ConfigStore::new(&path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, vec![b'x'; MAX_CONFIG_BYTES.saturating_add(1)]).unwrap();

    assert_eq!(
        store.save(&Config::default()).unwrap_err().kind(),
        ConfigStoreErrorKind::TooLarge
    );
    assert!(!store.backup_path().exists());
    assert_eq!(
        std::fs::metadata(&path).unwrap().len(),
        (MAX_CONFIG_BYTES + 1) as u64
    );

    std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).unwrap();
}

#[test]
fn legacy_json_load_modify_save_and_clean_reload_preserves_the_default_matrix() {
    let path = test_path("legacy-migration");
    let store = ConfigStore::new(&path);
    let legacy_json = r#"{
            "skin": "KDE",
            "mode": "Dark",
            "hc": false,
            "show_cpu": true,
            "show_memory": true,
            "show_disks": true,
            "show_network": true,
            "show_gpus": true,
            "refresh_ms": 2500,
            "last_page": "apps"
        }"#;

    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, legacy_json).unwrap();

    let mut config = store.load().expect("legacy JSON must load");
    assert_eq!(config.skin, "KDE");
    assert_eq!(config.mode, "Dark");
    assert_eq!(config.refresh_ms, 2500);
    assert_eq!(config.last_page, "apps");
    assert_eq!(config.startup_page, STARTUP_PAGE_REMEMBER);
    assert_eq!(config.graph_data_points, 60);
    assert_eq!(config.sidebar_width, 260.0);
    assert!(config.show_network_wired);
    assert!(config.show_network_wireless);
    assert!(config.show_network_vpn);
    assert!(config.show_network_virtual);
    assert!(config.show_network_other);

    config.startup_page = STARTUP_PAGE_PROCESSES.to_string();
    config.last_page = "performance".to_string();
    config.graph_data_points = 600;
    config.sliding_graphs = true;
    config.network_dynamic_scaling = false;
    config.sidebar_order = vec!["memory".to_string(), "cpu".to_string()];
    config.sidebar_width = 420.0;

    store.save(&config).expect("modified config must save");
    drop(store);

    let clean_reload = ConfigStore::new(&path)
        .load()
        .expect("a fresh store must reload the saved config");
    assert_eq!(clean_reload, config);
    assert_eq!(clean_reload.startup_page, STARTUP_PAGE_PROCESSES);
    assert_eq!(clean_reload.last_page, "performance");
    assert_eq!(clean_reload.graph_data_points, 600);
    assert!(clean_reload.sliding_graphs);
    assert!(!clean_reload.network_dynamic_scaling);
    assert_eq!(clean_reload.sidebar_order, ["memory", "cpu"]);
    assert_eq!(clean_reload.sidebar_width, 420.0);

    std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).unwrap();
}
