use super::{IcedApp, apply_capture_target, capture_device_from_name, capture_page_from_name};
use crate::app::PerfDevice;
use crate::app::{LocalSurfaceKind, Message};
use taskmanager_application::AppPage;
use taskmanager_application::ConfigClient;
use taskmanager_application::ConfigCoordinator;
use taskmanager_application::ConfigDrain;
use taskmanager_application::{ConfigStore, PlatformClient};
use taskmanager_core::core::config::Config;
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::metrics::ScalarAvailability;
use taskmanager_theme::FontAvailability;
use taskmanager_theme::{LightDark, Skin};

/// The shared `TM_SKIN` testing override selects the demo/capture appearance.
/// Unset keeps today's GNOME-dark demo default; a valid token resolves the
/// requested skin/mode through the production config pipeline and repaints the
/// resolved theme.
#[test]
fn demo_capture_tm_skin_override_resolves_the_theme() {
    let default = IcedApp::demo_for_capture_with(None, None);
    assert_eq!(default.theme().skin, Skin::Gnome);
    assert_eq!(
        default.theme().mode,
        LightDark::Dark,
        "unset keeps the GNOME-dark demo default"
    );

    let light = IcedApp::demo_for_capture_with(None, Some((Skin::Gnome, LightDark::Light, false)));
    assert_eq!(light.theme().skin, Skin::Gnome);
    assert_eq!(light.theme().mode, LightDark::Light);
    assert_ne!(
        default.theme().palette().window_backdrop,
        light.theme().palette().window_backdrop,
        "gnome-light must repaint the resolved demo theme"
    );

    let contrast = IcedApp::demo_for_capture_with(None, Some((Skin::Kde, LightDark::Dark, true)));
    assert_eq!(contrast.theme().skin, Skin::Kde);
    assert_eq!(contrast.theme().mode, LightDark::Dark);
    assert!(contrast.theme().hc, "TM_SKIN_HC rides the same override");
}

/// The `TM_SKIN` vocabulary is the shared GPUI contract, not a frontend-local
/// spelling: every skin/mode alias parses and every malformed token is
/// rejected.
#[test]
fn tm_skin_vocabulary_matches_the_shared_contract() {
    for (input, expected) in [
        ("gnome-dark", (Skin::Gnome, LightDark::Dark)),
        ("kde-light", (Skin::Kde, LightDark::Light)),
        ("win-dark", (Skin::Windows, LightDark::Dark)),
        ("windows-dark", (Skin::Windows, LightDark::Dark)),
        ("mac-light", (Skin::Macos, LightDark::Light)),
        ("macos-light", (Skin::Macos, LightDark::Light)),
        ("gnome-eyeforest", (Skin::Gnome, LightDark::EyeForest)),
        ("kde-eye-forest", (Skin::Kde, LightDark::EyeForest)),
        ("GNOME-DARK", (Skin::Gnome, LightDark::Dark)),
    ] {
        assert_eq!(
            crate::app::settings::parse_skin_override(input),
            Some(expected),
            "input {input}"
        );
    }
    for invalid in ["plasma-dark", "kde-vibes", "kde", "kde-dark-extra", ""] {
        assert_eq!(
            crate::app::settings::parse_skin_override(invalid),
            None,
            "input {invalid}"
        );
    }
}

#[test]
fn capture_device_selector_accepts_only_the_complete_performance_vocabulary() {
    for device in PerfDevice::ALL {
        assert_eq!(capture_device_from_name(device.key()), Some(device));
    }
    assert_eq!(capture_device_from_name("services"), None);
    assert_eq!(capture_device_from_name("GPU"), None);
    assert_eq!(
        capture_page_from_name("applications"),
        Some(AppPage::Applications)
    );
    assert_eq!(capture_page_from_name("services"), Some(AppPage::Services));
    assert_eq!(capture_page_from_name("startup"), Some(AppPage::Startup));
    assert_eq!(capture_page_from_name("users"), Some(AppPage::Users));
    assert_eq!(capture_page_from_name("system"), Some(AppPage::System));
    assert_eq!(
        capture_page_from_name("app-history"),
        Some(AppPage::AppHistory)
    );
    assert_eq!(capture_page_from_name("performance"), None);
}

#[test]
fn capture_fixture_has_multi_sample_dynamic_and_engine_data() {
    let app = super::IcedApp::demo_for_capture();
    let snapshot = app
        .shell
        .projection()
        .snapshot
        .as_ref()
        .expect("capture fixture snapshot");
    assert!(snapshot.gpu[0].engines.len() >= 2);
    assert!(
        app.shell
            .history
            .disk_bytes_per_sec_for(
                &snapshot.disks[0].device_id,
                snapshot.disks[0].device_generation.get(),
            )
            .len()
            >= 2
    );
    assert!(
        app.shell
            .history
            .gpu_engine_usage_pct_for(
                &snapshot.gpu[0].device_id,
                snapshot.gpu[0].device_generation.get(),
                &snapshot.gpu[0].engines[0].name,
            )
            .len()
            >= 2
    );
    let battery = app
        .shell
        .projection()
        .power_supplies
        .as_ref()
        .expect("capture fixture battery");
    assert!(
        app.shell
            .history
            .battery_power_w_for(&battery.batteries[0].id)
            .len()
            >= 2
    );
    let fan = app
        .shell
        .projection()
        .sensors
        .as_ref()
        .expect("capture fixture sensors");
    assert!(
        app.shell
            .history
            .fan_temperature_c_for(fan.readings[1].id())
            .len()
            >= 2
    );
}

#[test]
fn system_and_npu_capture_targets_seed_complete_typed_npu_facts() {
    let mut system = IcedApp::demo();
    assert!(system.shell.projection().npu_inventory.is_none());
    apply_capture_target(&mut system, "system");
    let inventory = system
        .shell
        .projection()
        .npu_inventory
        .as_ref()
        .expect("system capture NPU inventory");
    let device = &inventory.devices[0];
    assert_eq!(device.utilization_pct.current_value(), Some(&38.0));
    assert_eq!(device.engines.len(), 1);
    assert_eq!(
        device.engines[0].utilization_pct.current_value(),
        Some(&61.0)
    );
    assert_eq!(
        device.memory.dedicated_total_bytes.current_value(),
        Some(&0)
    );
    assert!(device.memory.shared_total_bytes.current_value().is_none());
    assert_eq!(
        device.memory.shared_total_bytes.availability(),
        ScalarAvailability::Unavailable(FailureKind::Unsupported)
    );

    let mut npu = IcedApp::demo();
    apply_capture_target(&mut npu, "npu");
    assert_eq!(npu.performance.selected_device, PerfDevice::Npu(0));
    assert_eq!(
        npu.shell.projection().npu_inventory,
        system.shell.projection().npu_inventory
    );

    let mut services = IcedApp::demo();
    apply_capture_target(&mut services, "services");
    assert!(services.shell.projection().npu_inventory.is_none());
}

#[test]
fn health_capture_target_opens_the_local_surface_through_the_message_reducer() {
    // `health` is a local-surface token: no Performance device and no shared
    // page may claim it, so the surface branch is the only possible match.
    assert_eq!(capture_device_from_name("health"), None);
    assert_eq!(capture_page_from_name("health"), None);

    let mut app = IcedApp::demo();
    assert_eq!(app.local_surface_kind(), None);
    apply_capture_target(&mut app, "health");
    assert_eq!(app.local_surface_kind(), Some(LocalSurfaceKind::Health));
    // The modal rides Performance with the default device selected; the
    // surface is the target, not a page or a device.
    assert_eq!(app.shell.page(), AppPage::Performance);
    assert_eq!(app.performance.selected_device, PerfDevice::Cpu);

    // The toolbar trigger's own reducer path owns the same surface state.
    let mut toolbar = IcedApp::demo();
    let _ = toolbar.update(Message::OpenHealth);
    assert_eq!(toolbar.local_surface_kind(), app.local_surface_kind());
}

impl IcedApp {
    /// Build the frontend with an injected configuration-store path (tests
    /// and alternate composition edges). The store is used by
    /// [`Self::load_config`] and the settings flow; the path is never read
    /// during rendering.
    pub(crate) fn with_config_store(platform: Option<PlatformClient>, store: ConfigStore) -> Self {
        Self::with_config_store_and_font_availability(
            platform,
            store,
            crate::font_catalog::system(),
        )
    }

    pub(crate) fn with_config_store_and_font_availability(
        platform: Option<PlatformClient>,
        store: ConfigStore,
        font_availability: FontAvailability,
    ) -> Self {
        let coordinator = ConfigCoordinator::start(store).expect("start injected config runtime");
        let mut app = Self::new(platform);
        app.configuration = crate::app::configuration_state::IcedConfiguration::new(
            Some(coordinator.client()),
            font_availability,
        );
        app.load_config();
        app
    }

    pub(crate) fn wait_for_config_where(&mut self, predicate: impl Fn(&Config) -> bool) {
        if self
            .configuration
            .client()
            .and_then(ConfigClient::snapshot)
            .is_some_and(|snapshot| predicate(snapshot))
        {
            return;
        }
        for _ in 0..64 {
            let drain = self
                .configuration
                .client_mut()
                .expect("injected config client")
                .wait_for_drain(std::time::Duration::from_secs(2));
            match drain {
                ConfigDrain::Empty => {
                    panic!("expected configuration publication")
                }
                ConfigDrain::Publications(publications) => {
                    for publication in publications {
                        self.apply_config_publication(&publication);
                    }
                }
                ConfigDrain::ResyncRequired { latest, .. } => {
                    self.apply_config_publication(&latest);
                }
            }
            if self
                .configuration
                .client()
                .and_then(ConfigClient::snapshot)
                .is_some_and(|snapshot| predicate(snapshot))
            {
                return;
            }
        }
        panic!("configuration predicate was not published");
    }
}
