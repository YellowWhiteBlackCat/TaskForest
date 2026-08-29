//! Typed cross-crate process fixtures.
//!
//! This doc-hidden seam keeps integration fixtures concise without restoring
//! schema-v1 row fields as a second writable model. Every measurement method
//! writes the canonical observation group explicitly.

use std::marker::PhantomData;

use super::{
    ProcessApplicationIdentity, ProcessItem, ProcessMetadataObservation,
    ProcessMetadataObservations, ProcessScalarObservations,
};
use crate::{GroupBaseOpen, NamedOverrides};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::metrics::{ScalarAvailability, ScalarObservation};
use taskmanager_core::core::process::{ProcessMetadataFailure, ProcessOwner, ProcessOwnerIdentity};

const FIXTURE_OBSERVED_AT: u64 = 1;

/// The deterministic provider start token every fixture process carries by
/// default (CORE-01: a fixture row always has a live identity). Tests that
/// assert row identities derive the expected value from this one source.
#[doc(hidden)]
#[must_use]
pub const fn fixture_start_token(pid: u32) -> u64 {
    pid as u64 * 10 + 1
}

/// Builder used by cross-crate behavior fixtures.
///
/// Production providers must use `ProcessItem::new` and named typed assembly.
#[doc(hidden)]
#[derive(Debug)]
pub struct ProcessItemFixtureBuilder<ScalarStage = GroupBaseOpen, MetadataStage = GroupBaseOpen> {
    item: ProcessItem,
    scalars: ProcessScalarObservations,
    scalar_stage: PhantomData<ScalarStage>,
    metadata_stage: PhantomData<MetadataStage>,
}

impl Default for ProcessItemFixtureBuilder {
    fn default() -> Self {
        Self {
            item: ProcessItem::default(),
            scalars: ProcessScalarObservations::default(),
            scalar_stage: PhantomData,
            metadata_stage: PhantomData,
        }
    }
}

impl ProcessItemFixtureBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn from_item(item: ProcessItem) -> Self {
        let scalars = *item.scalar_observations();
        Self {
            item,
            scalars,
            scalar_stage: PhantomData,
            metadata_stage: PhantomData,
        }
    }
}

impl<ScalarStage, MetadataStage> ProcessItemFixtureBuilder<ScalarStage, MetadataStage> {
    fn retag<NextScalar, NextMetadata>(
        self,
    ) -> ProcessItemFixtureBuilder<NextScalar, NextMetadata> {
        ProcessItemFixtureBuilder {
            item: self.item,
            scalars: self.scalars,
            scalar_stage: PhantomData,
            metadata_stage: PhantomData,
        }
    }

    #[must_use]
    pub fn pid(mut self, value: u32) -> Self {
        self.item.pid = value;
        self
    }

    #[must_use]
    pub fn parent_pid(mut self, value: Option<u32>) -> Self {
        self.item.parent_pid = value;
        self
    }

    #[must_use]
    pub fn name(mut self, value: String) -> Self {
        self.item.name = value;
        self
    }

    #[must_use]
    pub fn cmdline(mut self, value: String) -> Self {
        self.item.cmdline = value;
        self
    }

    #[must_use]
    pub fn status(mut self, value: String) -> Self {
        self.item.status = value;
        self
    }

    #[must_use]
    pub fn cpu_history(mut self, value: Vec<f32>) -> Self {
        self.item.cpu_history = value;
        self
    }

    #[must_use]
    pub fn mem_history(mut self, value: Vec<f32>) -> Self {
        self.item.mem_history = value;
        self
    }

    #[must_use]
    pub fn disk_history(mut self, value: Vec<f32>) -> Self {
        self.item.disk_history = value;
        self
    }

    #[must_use]
    pub fn disk_read_history(mut self, value: Vec<f32>) -> Self {
        self.item.disk_read_history = value;
        self
    }

    #[must_use]
    pub fn disk_write_history(mut self, value: Vec<f32>) -> Self {
        self.item.disk_write_history = value;
        self
    }

    #[must_use]
    pub fn build(mut self) -> ProcessItem {
        // CORE-01: a fixture process carries a validated live identity by
        // default. Tests needing a token-less (unknown-identity) process
        // install explicit scalar observations instead of relying on the
        // default.
        if self.scalars.start_token.availability() == ScalarAvailability::Unknown {
            self.scalars.start_token = ScalarObservation::available(
                fixture_start_token(self.item.pid),
                FIXTURE_OBSERVED_AT,
            );
        }
        self.item.apply_scalar_observations(self.scalars);
        self.item
    }
}

impl<MetadataStage> ProcessItemFixtureBuilder<GroupBaseOpen, MetadataStage> {
    /// Install the optional scalar base and enter the named-override stage.
    #[must_use]
    pub fn scalar_observations(
        self,
        value: ProcessScalarObservations,
    ) -> ProcessItemFixtureBuilder<NamedOverrides, MetadataStage> {
        let mut next = self.retag();
        next.scalars = value;
        next
    }

    #[must_use]
    pub fn current_cpu_percentage(
        self,
        value: f32,
    ) -> ProcessItemFixtureBuilder<NamedOverrides, MetadataStage> {
        let next: ProcessItemFixtureBuilder<NamedOverrides, MetadataStage> = self.retag();
        next.current_cpu_percentage(value)
    }

    #[must_use]
    pub fn current_memory_bytes(
        self,
        value: u64,
    ) -> ProcessItemFixtureBuilder<NamedOverrides, MetadataStage> {
        let next: ProcessItemFixtureBuilder<NamedOverrides, MetadataStage> = self.retag();
        next.current_memory_bytes(value)
    }

    #[must_use]
    pub fn current_disk_read_bytes_per_sec(
        self,
        value: u64,
    ) -> ProcessItemFixtureBuilder<NamedOverrides, MetadataStage> {
        let next: ProcessItemFixtureBuilder<NamedOverrides, MetadataStage> = self.retag();
        next.current_disk_read_bytes_per_sec(value)
    }

    #[must_use]
    pub fn current_disk_write_bytes_per_sec(
        self,
        value: u64,
    ) -> ProcessItemFixtureBuilder<NamedOverrides, MetadataStage> {
        let next: ProcessItemFixtureBuilder<NamedOverrides, MetadataStage> = self.retag();
        next.current_disk_write_bytes_per_sec(value)
    }

    #[must_use]
    pub fn current_threads(
        self,
        value: u32,
    ) -> ProcessItemFixtureBuilder<NamedOverrides, MetadataStage> {
        let next: ProcessItemFixtureBuilder<NamedOverrides, MetadataStage> = self.retag();
        next.current_threads(value)
    }

    #[must_use]
    pub fn current_start_time_secs(
        self,
        value: u64,
    ) -> ProcessItemFixtureBuilder<NamedOverrides, MetadataStage> {
        let next: ProcessItemFixtureBuilder<NamedOverrides, MetadataStage> = self.retag();
        next.current_start_time_secs(value)
    }

    #[must_use]
    pub fn current_cpu_time_secs(
        self,
        value: u64,
    ) -> ProcessItemFixtureBuilder<NamedOverrides, MetadataStage> {
        let next: ProcessItemFixtureBuilder<NamedOverrides, MetadataStage> = self.retag();
        next.current_cpu_time_secs(value)
    }

    #[must_use]
    pub fn current_fds(
        self,
        value: u32,
    ) -> ProcessItemFixtureBuilder<NamedOverrides, MetadataStage> {
        let next: ProcessItemFixtureBuilder<NamedOverrides, MetadataStage> = self.retag();
        next.current_fds(value)
    }

    #[must_use]
    pub fn current_nice(
        self,
        value: i32,
    ) -> ProcessItemFixtureBuilder<NamedOverrides, MetadataStage> {
        let next: ProcessItemFixtureBuilder<NamedOverrides, MetadataStage> = self.retag();
        next.current_nice(value)
    }
}

impl<MetadataStage> ProcessItemFixtureBuilder<NamedOverrides, MetadataStage> {
    #[must_use]
    pub fn current_cpu_percentage(mut self, value: f32) -> Self {
        self.scalars.cpu_percentage = ScalarObservation::available(value, FIXTURE_OBSERVED_AT);
        self
    }

    #[must_use]
    pub fn current_memory_bytes(mut self, value: u64) -> Self {
        self.scalars.memory_bytes = ScalarObservation::available(value, FIXTURE_OBSERVED_AT);
        self
    }

    #[must_use]
    pub fn current_disk_read_bytes_per_sec(mut self, value: u64) -> Self {
        self.scalars.disk_read_bytes_per_sec =
            ScalarObservation::available(value, FIXTURE_OBSERVED_AT);
        self
    }

    #[must_use]
    pub fn current_disk_write_bytes_per_sec(mut self, value: u64) -> Self {
        self.scalars.disk_write_bytes_per_sec =
            ScalarObservation::available(value, FIXTURE_OBSERVED_AT);
        self
    }

    #[must_use]
    pub fn current_threads(mut self, value: u32) -> Self {
        self.scalars.threads = ScalarObservation::available(value, FIXTURE_OBSERVED_AT);
        self
    }

    #[must_use]
    pub fn current_start_time_secs(mut self, value: u64) -> Self {
        self.scalars.start_time_secs = ScalarObservation::available(value, FIXTURE_OBSERVED_AT);
        self
    }

    /// Opt out of the default live identity (CORE-01): the fixture carries
    /// no current start token, modeling a legacy or provider-opaque row
    /// whose exact identity is unprovable. Dangerous-action paths must fail
    /// closed on such rows; tests asserting that rule build them this way.
    #[must_use]
    pub fn without_current_start_token(mut self) -> Self {
        self.scalars.start_token = ScalarObservation::unavailable(
            taskmanager_core::core::failure::FailureKind::Unsupported,
        );
        self
    }

    #[must_use]
    pub fn current_cpu_time_secs(mut self, value: u64) -> Self {
        self.scalars.cpu_time_secs = ScalarObservation::available(value, FIXTURE_OBSERVED_AT);
        self
    }

    #[must_use]
    pub fn current_fds(mut self, value: u32) -> Self {
        self.scalars.fds = ScalarObservation::available(value, FIXTURE_OBSERVED_AT);
        self
    }

    #[must_use]
    pub fn current_nice(mut self, value: i32) -> Self {
        self.scalars.nice = ScalarObservation::available(value, FIXTURE_OBSERVED_AT);
        self
    }
}

impl<ScalarStage> ProcessItemFixtureBuilder<ScalarStage, GroupBaseOpen> {
    /// Install the optional metadata base and enter the named-override stage.
    #[must_use]
    pub fn metadata_observations(
        mut self,
        value: ProcessMetadataObservations,
    ) -> ProcessItemFixtureBuilder<ScalarStage, NamedOverrides> {
        self.item.apply_metadata_observations(value);
        self.retag()
    }

    #[must_use]
    pub fn application_identity_observation(
        self,
        value: ProcessMetadataObservation<ProcessApplicationIdentity>,
    ) -> ProcessItemFixtureBuilder<ScalarStage, NamedOverrides> {
        let next: ProcessItemFixtureBuilder<ScalarStage, NamedOverrides> = self.retag();
        next.application_identity_observation(value)
    }
}

impl<ScalarStage> ProcessItemFixtureBuilder<ScalarStage, NamedOverrides> {
    #[must_use]
    pub fn application_identity_observation(
        mut self,
        value: ProcessMetadataObservation<ProcessApplicationIdentity>,
    ) -> Self {
        self.item.apply_application_identity(value);
        self
    }
}

/// Typed optional metrics for cross-frontend sort-parity fixtures.
#[doc(hidden)]
#[derive(Default)]
pub struct SortFixtureMetrics {
    pub cpu: Option<f32>,
    pub rss: Option<u64>,
    pub pss: Option<u64>,
    pub swap: Option<u64>,
    pub threads: Option<u32>,
    pub cpu_time: Option<u64>,
    pub disk_read: Option<u64>,
    pub disk_write: Option<u64>,
    pub start_time: Option<u64>,
    pub fds: Option<u32>,
    pub nice: Option<i32>,
}

fn fixture_observation<T>(value: Option<T>) -> ScalarObservation<T> {
    match value {
        Some(value) => ScalarObservation::available(value, FIXTURE_OBSERVED_AT),
        None => ScalarObservation::unavailable(FailureKind::PermissionDenied),
    }
}

/// Build one typed row for cross-frontend sort behavior tests.
#[doc(hidden)]
#[must_use]
pub fn sort_fixture_row(
    pid: u32,
    name: &str,
    user: &str,
    status: &str,
    metrics: SortFixtureMetrics,
) -> ProcessItem {
    let mut process = ProcessItem::new(pid, name);
    process.status = status.to_owned();
    process.apply_metadata_observations(ProcessMetadataObservations {
        owner: ProcessMetadataObservation::available(
            ProcessOwner {
                identity: ProcessOwnerIdentity::Opaque(user.to_owned()),
                label: None,
            },
            FIXTURE_OBSERVED_AT,
        ),
        executable_path: ProcessMetadataObservation::absent(FIXTURE_OBSERVED_AT),
    });
    process.apply_scalar_observations(ProcessScalarObservations {
        cpu_percentage: fixture_observation(metrics.cpu),
        memory_bytes: fixture_observation(metrics.rss),
        memory_pss_bytes: fixture_observation(metrics.pss),
        swap_bytes: fixture_observation(metrics.swap),
        threads: fixture_observation(metrics.threads),
        cpu_time_secs: fixture_observation(metrics.cpu_time),
        disk_read_bytes_per_sec: fixture_observation(metrics.disk_read),
        disk_write_bytes_per_sec: fixture_observation(metrics.disk_write),
        start_time_secs: fixture_observation(metrics.start_time),
        fds: fixture_observation(metrics.fds),
        nice: fixture_observation(metrics.nice),
        ..ProcessScalarObservations::default()
    });
    process
}

/// Four typed rows shared by sort-parity behavior tests.
#[doc(hidden)]
#[must_use]
pub fn sort_parity_fixture() -> Vec<ProcessItem> {
    vec![
        sort_fixture_row(
            11,
            "Alpha",
            "root",
            "S",
            SortFixtureMetrics {
                cpu: Some(5.0),
                rss: Some(500),
                pss: Some(300),
                swap: Some(64),
                threads: Some(4),
                cpu_time: Some(100),
                disk_read: Some(1024),
                disk_write: Some(512),
                start_time: Some(1_000_000),
                fds: Some(30),
                nice: Some(0),
            },
        ),
        sort_fixture_row(
            12,
            "alpha",
            "Root",
            "R",
            SortFixtureMetrics {
                cpu: Some(5.0),
                rss: Some(400),
                threads: Some(2),
                cpu_time: Some(50),
                disk_read: Some(2048),
                disk_write: Some(256),
                start_time: Some(1_000_000),
                fds: Some(20),
                nice: Some(5),
                ..SortFixtureMetrics::default()
            },
        ),
        sort_fixture_row(
            13,
            "daemon",
            "daemon",
            "S",
            SortFixtureMetrics {
                cpu: Some(2.5),
                rss: Some(300),
                pss: Some(900),
                swap: Some(8),
                threads: Some(8),
                cpu_time: Some(900),
                disk_read: Some(0),
                disk_write: Some(0),
                start_time: Some(2_000_000),
                fds: Some(10),
                nice: Some(-5),
            },
        ),
        sort_fixture_row(14, "zombie", "root", "Z", SortFixtureMetrics::default()),
    ]
}

fn fixture_identity(display_name: &str) -> ProcessApplicationIdentity {
    ProcessApplicationIdentity::new("org.example.taskforest-fixture", display_name, None)
        .unwrap_or_else(|| ProcessApplicationIdentity {
            launcher_id: "org.example.taskforest-fixture".to_owned(),
            display_name: "fixture".to_owned(),
            icon_token: None,
            icon_asset: None,
            icon_failure: None,
        })
}

fn category_fixture_item(
    pid: u32,
    name: &str,
    cpu_usage: f32,
    memory_bytes: u64,
    application_identity: ProcessMetadataObservation<ProcessApplicationIdentity>,
    memory_pss_bytes: Option<u64>,
) -> ProcessItem {
    let mut scalars = ProcessScalarObservations {
        cpu_percentage: ScalarObservation::available(cpu_usage, 42),
        memory_bytes: ScalarObservation::available(memory_bytes, 42),
        ..Default::default()
    };
    if let Some(pss) = memory_pss_bytes {
        scalars.memory_pss_bytes = ScalarObservation::available(pss, 42);
    }
    ProcessItemFixtureBuilder::new()
        .pid(pid)
        .name(name.to_owned())
        .application_identity_observation(application_identity)
        .scalar_observations(scalars)
        .build()
}

fn available_identity(
    display_name: &str,
) -> ProcessMetadataObservation<ProcessApplicationIdentity> {
    ProcessMetadataObservation::available(fixture_identity(display_name), 10)
}

fn partial_identity(display_name: &str) -> ProcessMetadataObservation<ProcessApplicationIdentity> {
    ProcessMetadataObservation::partial(
        fixture_identity(display_name),
        10,
        ProcessMetadataFailure::NotFound,
    )
}

fn stale_identity(display_name: &str) -> ProcessMetadataObservation<ProcessApplicationIdentity> {
    ProcessMetadataObservation::available(fixture_identity(display_name), 10)
        .transition_failure(ProcessMetadataFailure::ProviderFault)
}

/// Typed rows covering every application-identity availability state.
#[doc(hidden)]
#[must_use]
pub fn mixed_availability_category_fixture() -> Vec<ProcessItem> {
    vec![
        category_fixture_item(
            11,
            "fixture-editor",
            9.0,
            400,
            available_identity("Fixture Editor"),
            Some(250),
        ),
        category_fixture_item(
            12,
            "fixture-helper",
            5.0,
            100,
            partial_identity("Fixture Helper"),
            None,
        ),
        category_fixture_item(
            30,
            "fixture-daemon",
            4.0,
            250,
            ProcessMetadataObservation::absent(10),
            None,
        ),
        category_fixture_item(
            40,
            "fixture-unknown",
            3.0,
            300,
            ProcessMetadataObservation::default(),
            None,
        ),
        category_fixture_item(
            41,
            "fixture-stale",
            2.0,
            20,
            stale_identity("Fixture Stale"),
            None,
        ),
        category_fixture_item(
            31,
            "fixture-worker",
            1.0,
            50,
            ProcessMetadataObservation::absent(10),
            None,
        ),
        category_fixture_item(
            42,
            "fixture-denied",
            0.5,
            10,
            ProcessMetadataObservation::unavailable(ProcessMetadataFailure::PermissionDenied),
            None,
        ),
    ]
}

/// Typed category rows with no unclassified identity state.
#[doc(hidden)]
#[must_use]
pub fn category_fixture_with_empty_bucket() -> Vec<ProcessItem> {
    vec![
        category_fixture_item(
            11,
            "fixture-editor",
            9.0,
            400,
            available_identity("Fixture Editor"),
            None,
        ),
        category_fixture_item(
            12,
            "fixture-helper",
            5.0,
            100,
            partial_identity("Fixture Helper"),
            None,
        ),
        category_fixture_item(
            30,
            "fixture-daemon",
            4.0,
            250,
            ProcessMetadataObservation::absent(10),
            None,
        ),
        category_fixture_item(
            31,
            "fixture-worker",
            1.0,
            50,
            ProcessMetadataObservation::absent(10),
            None,
        ),
    ]
}
