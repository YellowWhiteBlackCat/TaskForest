use super::{IcedApp, apply_capture_target, capture_device_from_name, capture_page_from_name};
use crate::app::PerfDevice;
use taskmanager_application::{ConfigStore, PlatformClient};

#[test]
fn capture_device_selector_accepts_only_the_complete_performance_vocabulary() {
    let expected = [
        ("cpu", PerfDevice::Cpu),
        ("memory", PerfDevice::Memory),
        ("disk", PerfDevice::Disk(0)),
        ("network", PerfDevice::Network(0)),
        ("gpu", PerfDevice::Gpu(0)),
        ("battery", PerfDevice::Battery(0)),
        ("fan", PerfDevice::Fan(0)),
    ];
    for (name, device) in expected {
        assert_eq!(capture_device_from_name(name), Some(device));
    }
    assert_eq!(capture_device_from_name("services"), None);
    assert_eq!(capture_device_from_name("GPU"), None);
    assert_eq!(
        capture_page_from_name("applications"),
        Some(taskmanager_application::AppPage::Applications)
    );
    assert_eq!(
        capture_page_from_name("services"),
        Some(taskmanager_application::AppPage::Services)
    );
    assert_eq!(
        capture_page_from_name("startup"),
        Some(taskmanager_application::AppPage::Startup)
    );
    assert_eq!(
        capture_page_from_name("users"),
        Some(taskmanager_application::AppPage::Users)
    );
    assert_eq!(
        capture_page_from_name("system"),
        Some(taskmanager_application::AppPage::System)
    );
    assert_eq!(
        capture_page_from_name("app-history"),
        Some(taskmanager_application::AppPage::AppHistory)
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
fn only_the_system_capture_target_seeds_complete_typed_npu_facts() {
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
        taskmanager_application::ScalarAvailability::Unavailable(
            taskmanager_application::FailureKind::Unsupported
        )
    );

    let mut services = IcedApp::demo();
    apply_capture_target(&mut services, "services");
    assert!(services.shell.projection().npu_inventory.is_none());
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
        font_availability: taskmanager_theme::FontAvailability,
    ) -> Self {
        let coordinator = taskmanager_application::ConfigCoordinator::start(store)
            .expect("start injected config runtime");
        let mut app = Self::new(platform);
        app.configuration = crate::app::configuration_state::IcedConfiguration::new(
            Some(coordinator.client()),
            font_availability,
        );
        app.load_config();
        app
    }

    pub(crate) fn wait_for_config_where(
        &mut self,
        predicate: impl Fn(&taskmanager_application::Config) -> bool,
    ) {
        if self
            .configuration
            .client()
            .and_then(taskmanager_application::ConfigClient::snapshot)
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
                taskmanager_application::ConfigDrain::Empty => {
                    panic!("expected configuration publication")
                }
                taskmanager_application::ConfigDrain::Publications(publications) => {
                    for publication in publications {
                        self.apply_config_publication(&publication);
                    }
                }
                taskmanager_application::ConfigDrain::ResyncRequired { latest, .. } => {
                    self.apply_config_publication(&latest);
                }
            }
            if self
                .configuration
                .client()
                .and_then(taskmanager_application::ConfigClient::snapshot)
                .is_some_and(|snapshot| predicate(snapshot))
            {
                return;
            }
        }
        panic!("configuration predicate was not published");
    }
}
