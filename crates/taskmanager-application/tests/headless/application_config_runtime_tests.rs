use std::fs::OpenOptions;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use taskmanager_application::{
    Config, ConfigBootstrap, ConfigClient, ConfigCoordinator, ConfigDrain, ConfigLoadSource,
    ConfigPublication, ConfigPublicationOutcome, ConfigRecoveryNotice, ConfigRuntimeOptions,
    ConfigStore, ConfigSubmissionStatus, ConfigSubmitError,
};

fn test_path(label: &str) -> PathBuf {
    static NEXT_PATH: AtomicU64 = AtomicU64::new(0);
    let sequence = NEXT_PATH.fetch_add(1, Ordering::Relaxed);
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.tmp/application-config-runtime")
        .join(format!("{label}-{}-{sequence}", std::process::id()))
        .join("config.json")
}

fn start(path: &PathBuf) -> (ConfigCoordinator, ConfigClient) {
    let coordinator = ConfigCoordinator::start(ConfigStore::new(path)).expect("start runtime");
    let mut client = coordinator.client();
    assert!(matches!(
        client.wait_for_initial(Duration::from_secs(2)),
        ConfigBootstrap::Published(_)
    ));
    (coordinator, client)
}

fn observe_initial(client: &mut ConfigClient) {
    assert!(matches!(
        client.wait_for_initial(Duration::from_secs(2)),
        ConfigBootstrap::Published(_)
    ));
}

fn changed(client: &ConfigClient, change: impl FnOnce(&mut Config)) -> Config {
    let mut config = client
        .snapshot()
        .expect("client observed initial config")
        .as_ref()
        .clone();
    change(&mut config);
    config
}

fn wait_for(
    client: &mut ConfigClient,
    predicate: impl Fn(&ConfigPublication) -> bool,
) -> std::sync::Arc<ConfigPublication> {
    for _ in 0..8 {
        let drain = client.wait_for_drain(Duration::from_secs(2));
        match drain {
            ConfigDrain::Empty => {}
            ConfigDrain::Publications(publications) => {
                if let Some(publication) = publications
                    .into_iter()
                    .find(|publication| predicate(publication))
                {
                    return publication;
                }
            }
            ConfigDrain::ResyncRequired { latest, .. } if predicate(&latest) => return latest,
            ConfigDrain::ResyncRequired { .. } => {}
        }
    }
    panic!("expected configuration publication was not observed");
}

#[test]
fn stale_clients_merge_disjoint_fields_in_either_acceptance_order() {
    for (label, language_first) in [("language-first", true), ("size-first", false)] {
        let path = test_path(label);
        let (coordinator, mut observer) = start(&path);
        let mut language_client = coordinator.client();
        let mut size_client = coordinator.client();
        observe_initial(&mut language_client);
        observe_initial(&mut size_client);
        let language = changed(&language_client, |config| {
            config.language = Some("zh".into())
        });
        let size = changed(&size_client, |config| config.ui_size = "Large".into());

        if language_first {
            language_client
                .try_submit(language)
                .expect("queue language");
            size_client.try_submit(size).expect("queue size");
        } else {
            size_client.try_submit(size).expect("queue size");
            language_client
                .try_submit(language)
                .expect("queue language");
        }

        let merged = wait_for(&mut observer, |publication| {
            publication.snapshot().language.as_deref() == Some("zh")
                && publication.snapshot().ui_size == "Large"
        });
        assert_eq!(merged.snapshot().language.as_deref(), Some("zh"));
        assert_eq!(merged.snapshot().ui_size, "Large");
        drop(language_client);
        drop(size_client);
        drop(observer);
        drop(coordinator);
        let _ = std::fs::remove_dir_all(path.parent().expect("config parent"));
    }
}

#[test]
fn same_field_uses_command_acceptance_order_and_exact_noop_does_not_publish() {
    let path = test_path("same-field");
    let (coordinator, mut observer) = start(&path);
    let mut first = coordinator.client();
    let mut second = coordinator.client();
    observe_initial(&mut first);
    observe_initial(&mut second);

    first
        .try_submit(changed(&first, |config| {
            config.language = Some("en".into())
        }))
        .expect("queue first language");
    second
        .try_submit(changed(&second, |config| {
            config.language = Some("zh".into())
        }))
        .expect("queue second language");
    let latest = wait_for(&mut observer, |publication| {
        publication.snapshot().language.as_deref() == Some("zh")
    });
    assert_eq!(latest.snapshot().language.as_deref(), Some("zh"));

    let before = observer.revision();
    let unchanged = observer
        .snapshot()
        .expect("observed config")
        .as_ref()
        .clone();
    assert_eq!(
        observer.try_submit(unchanged),
        Ok(ConfigSubmissionStatus::NoChange)
    );
    assert_eq!(observer.drain(), ConfigDrain::Empty);
    assert_eq!(observer.revision(), before);
    drop(first);
    drop(second);
    drop(observer);
    drop(coordinator);
    let _ = std::fs::remove_dir_all(path.parent().expect("config parent"));
}

#[test]
fn stale_same_value_commit_is_a_lock_level_noop_without_an_extra_publication() {
    let path = test_path("stale-same-value");
    let (coordinator, mut observer) = start(&path);
    let mut first = coordinator.client();
    let mut stale = coordinator.client();
    observe_initial(&mut first);
    observe_initial(&mut stale);
    let first_local = changed(&first, |config| config.ui_size = "Large".into());
    let stale_local = changed(&stale, |config| config.ui_size = "Large".into());

    first.try_submit(first_local).expect("queue first value");
    let saved = wait_for(&mut observer, |publication| {
        publication.snapshot().ui_size == "Large"
    });
    stale.try_submit(stale_local).expect("queue stale value");
    stale
        .synchronize(Duration::from_secs(2))
        .expect("wait for stale commit");

    assert_eq!(observer.drain(), ConfigDrain::Empty);
    assert_eq!(observer.revision(), Some(saved.revision()));
    drop(first);
    drop(stale);
    drop(observer);
    drop(coordinator);
    let _ = std::fs::remove_dir_all(path.parent().expect("config parent"));
}

#[test]
fn full_command_lane_returns_typed_backpressure_without_blocking() {
    let path = test_path("backpressure");
    let coordinator = ConfigCoordinator::start_with_options(
        ConfigStore::new(&path),
        ConfigRuntimeOptions {
            command_capacity: 1,
            publication_capacity: 4,
            refresh_interval: Duration::from_secs(60),
        },
    )
    .expect("start runtime");
    let mut client = coordinator.client();
    observe_initial(&mut client);
    std::fs::create_dir_all(path.parent().expect("config parent")).expect("create parent");
    let lock_path = path.with_extension("json.lock");
    let holder = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)
        .expect("open lock holder");
    holder.try_lock().expect("hold writer lock");

    let attempts = [
        client.try_submit(changed(&client, |config| config.skin = "KDE".into())),
        client.try_submit(changed(&client, |config| config.mode = "Dark".into())),
        client.try_submit(changed(&client, |config| config.ui_size = "Large".into())),
    ];
    assert!(attempts.contains(&Err(ConfigSubmitError::Backpressure)));
    drop(holder);
    drop(client);
    drop(coordinator);
    let _ = std::fs::remove_dir_all(path.parent().expect("config parent"));
}

#[test]
fn save_failure_is_published_without_advancing_success_revision() {
    let path = test_path("save-failure");
    let blocking_parent = path.parent().expect("config parent");
    std::fs::create_dir_all(blocking_parent.parent().expect("fixture root"))
        .expect("create fixture root");
    std::fs::write(blocking_parent, "not a directory").expect("create blocking file");
    let (coordinator, mut client) = start(&path);
    let revision = client.revision().expect("initial revision");
    client
        .try_submit(changed(&client, |config| config.ui_size = "Large".into()))
        .expect("queue save");
    let failed = wait_for(&mut client, |publication| {
        matches!(
            publication.outcome(),
            ConfigPublicationOutcome::SaveFailed { .. }
        )
    });
    assert_eq!(failed.revision(), revision);
    assert!(failed.snapshot().ui_size.is_empty());
    drop(client);
    drop(coordinator);
    let _ = std::fs::remove_file(blocking_parent);
}

#[test]
fn initial_publication_classifies_backup_and_default_recovery() {
    let backup_path = test_path("backup-recovery");
    let store = ConfigStore::new(&backup_path);
    let first = Config {
        language: Some("en".into()),
        ..Config::default()
    };
    store.save(&first).expect("save first");
    let second = Config {
        language: Some("zh".into()),
        ..first
    };
    store.save(&second).expect("save second");
    std::fs::write(&backup_path, "{ damaged").expect("damage primary");
    let coordinator = ConfigCoordinator::start(ConfigStore::new(&backup_path)).expect("start");
    let mut client = coordinator.client();
    let ConfigBootstrap::Published(publication) = client.wait_for_initial(Duration::from_secs(2))
    else {
        panic!("expected recovered publication");
    };
    let ConfigPublicationOutcome::Loaded(recovery) = publication.outcome() else {
        panic!("expected initial load outcome");
    };
    assert_eq!(recovery.source(), ConfigLoadSource::Backup);
    assert_eq!(recovery.initial_notice(), ConfigRecoveryNotice::Recovered);
    assert_eq!(publication.snapshot().language.as_deref(), Some("en"));

    let missing_path = test_path("default-recovery");
    let missing = ConfigCoordinator::start(ConfigStore::new(&missing_path)).expect("start missing");
    let mut missing_client = missing.client();
    let ConfigBootstrap::Published(publication) =
        missing_client.wait_for_initial(Duration::from_secs(2))
    else {
        panic!("expected default publication");
    };
    let ConfigPublicationOutcome::Loaded(recovery) = publication.outcome() else {
        panic!("expected initial load outcome");
    };
    assert_eq!(recovery.source(), ConfigLoadSource::Default);
    assert!(recovery.is_pristine_default());
    assert_eq!(recovery.initial_notice(), ConfigRecoveryNotice::None);
    assert_eq!(publication.snapshot().as_ref(), &Config::default());
    drop(client);
    drop(coordinator);
    drop(missing_client);
    drop(missing);
    let _ = std::fs::remove_dir_all(backup_path.parent().expect("config parent"));
}

#[test]
fn external_writer_is_observed_but_broken_refresh_retains_last_good_snapshot() {
    let path = test_path("external-refresh");
    let coordinator = ConfigCoordinator::start_with_options(
        ConfigStore::new(&path),
        ConfigRuntimeOptions {
            refresh_interval: Duration::from_millis(10),
            ..ConfigRuntimeOptions::default()
        },
    )
    .expect("start runtime");
    let mut client = coordinator.client();
    observe_initial(&mut client);
    let writer = ConfigStore::new(&path);
    let mut external = writer.load_or_default();
    external.ui_size = "Large".into();
    writer.save(&external).expect("external save");
    let refreshed = wait_for(&mut client, |publication| {
        matches!(
            publication.outcome(),
            ConfigPublicationOutcome::Refreshed(_)
        ) && publication.snapshot().ui_size == "Large"
    });
    let revision = refreshed.revision();

    std::fs::write(&path, "{ damaged").expect("damage primary");
    client.try_refresh().expect("queue broken refresh");
    let failed = wait_for(&mut client, |publication| {
        matches!(
            publication.outcome(),
            ConfigPublicationOutcome::RefreshFailed(_)
        )
    });
    assert_eq!(failed.revision(), revision);
    assert_eq!(failed.snapshot().ui_size, "Large");
    drop(client);
    drop(coordinator);
    let _ = std::fs::remove_dir_all(path.parent().expect("config parent"));
}

#[test]
fn slow_client_gets_typed_resync_with_latest_full_snapshot() {
    let path = test_path("lagged");
    let coordinator = ConfigCoordinator::start_with_options(
        ConfigStore::new(&path),
        ConfigRuntimeOptions {
            publication_capacity: 2,
            ..ConfigRuntimeOptions::default()
        },
    )
    .expect("start runtime");
    let mut writer = coordinator.client();
    let mut slow = coordinator.client();
    observe_initial(&mut writer);
    observe_initial(&mut slow);

    for size in ["Small", "Standard", "Large"] {
        writer
            .try_submit(changed(&writer, |config| config.ui_size = size.into()))
            .expect("queue size");
        wait_for(&mut writer, |publication| {
            publication.snapshot().ui_size == size
        });
    }

    let ConfigDrain::ResyncRequired {
        missed_publications,
        latest,
    } = slow.drain()
    else {
        panic!("slow cursor must require resync");
    };
    assert!(missed_publications > 0);
    assert_eq!(latest.snapshot().ui_size, "Large");
    assert_eq!(slow.snapshot().expect("resynced snapshot").ui_size, "Large");
    drop(writer);
    drop(slow);
    drop(coordinator);
    let _ = std::fs::remove_dir_all(path.parent().expect("config parent"));
}

#[test]
fn last_runtime_handle_drop_stops_worker_through_independent_control_lane() {
    let path = test_path("shutdown");
    let coordinator = ConfigCoordinator::start(ConfigStore::new(&path)).expect("start runtime");
    let monitor = coordinator.monitor();
    let mut client = coordinator.client();
    observe_initial(&mut client);
    drop(coordinator);
    assert_ne!(
        monitor.state(),
        taskmanager_application::ConfigWorkerState::Stopped
    );
    drop(client);
    assert!(monitor.wait_stopped(Duration::from_secs(2)));
}
