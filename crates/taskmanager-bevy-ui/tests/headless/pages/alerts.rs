//! test-intent: behavior
//!
//! Alerts-page tests:
//!
//! - pure row projections: the active-alert line carries the shared
//!   threshold/hysteresis semantics (value ≥ threshold, clears ≤ threshold −
//!   hysteresis joined from the canonical rule set; a missing rule omits the
//!   clear band instead of inventing one) and the per-metric units;
//! - the shell-edit semantics the toggle applier performs: participation
//!   counts flip through the canonical edit entry, disabled rules stay
//!   visible;
//! - the wired observer chain on the real plugin composition: a checkbox
//!   activation flows through `ShellApp::edit_alert_rules` (guarded against
//!   double flips) and remounts the page with the fresh projection; a fold
//!   event remounts; idle frames redraw nothing.
//!
//! The notification-submission half of the fold observer rides the
//! `queue_effect` semantics already locked by the shell/TUI seam tests; the
//! pending queue is only seedable through a full correlated telemetry batch,
//! so it is not re-asserted here (五问 #5: the cheaper guarantee exists).

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::Assets;
use bevy::ecs::entity::Entity;
use bevy::ecs::query::{Has, With};
use bevy::ecs::world::World;
use bevy::scene::WorldSceneExt;
use bevy::text::Font;
use bevy::ui::Checked;
use bevy::ui::widget::Text;
use taskmanager_application::{
    HostTelemetryRequest, PlatformClient, PlatformEvent, PlatformFacets, PlatformHandle,
    SystemFacets,
};
use taskmanager_core::core::alerts::{Alert, AlertMetric, AlertSeverity};
use taskmanager_platform_contract::{
    CapabilityCatalog, CapabilityDescriptor, CapabilityId, CapabilitySnapshot, CapabilityStatus,
    EventEnvelope, EventPort, EventPortError, RequestPort, SubmissionError,
};

use taskmanager_shell::ShellApp;
use taskmanager_theme::Theme;

use super::{AlertRuleToggleTarget, active_alert_line, alerts_summary, managed_rule_line};
use crate::app::{FrontendTrack, Page, PageContent, Route};
use crate::drain::ShellProjectionFolded;
use crate::palette::ui_palette;
use crate::window::FrontendWindowPlugin;
use crate::window::tests::HeadlessFrontendPlugins;

// ---- scripted platform client (the headless shell-app composition) ----

struct FixedCapabilities(CapabilitySnapshot);

impl CapabilityCatalog for FixedCapabilities {
    fn snapshot(&self) -> CapabilitySnapshot {
        self.0.clone()
    }
}

struct QuietEvents;

impl EventPort for QuietEvents {
    type Event = PlatformEvent;

    fn try_recv(&self) -> Result<Option<EventEnvelope<Self::Event>>, EventPortError> {
        Ok(None)
    }
}

struct QuietRequests;

impl RequestPort for QuietRequests {
    type Request = HostTelemetryRequest;

    fn try_submit(
        &self,
        _request: taskmanager_platform_contract::RequestEnvelope<Self::Request>,
    ) -> Result<(), SubmissionError> {
        Ok(())
    }
}

fn scripted_runtime() -> &'static crate::runtime::SharedRuntime {
    let snapshot = CapabilitySnapshot::from_descriptors([CapabilityDescriptor {
        id: CapabilityId::TELEMETRY_HOST,
        status: CapabilityStatus::Available,
        providers: Vec::new(),
        observed_at_ms: 1,
        last_success_at_ms: None,
    }]);
    let client = PlatformClient::new(PlatformHandle::new(
        std::sync::Arc::new(FixedCapabilities(snapshot)),
        std::sync::Arc::new(QuietEvents),
        PlatformFacets::default()
            .with_system(SystemFacets::default().with_host(std::sync::Arc::new(QuietRequests))),
    ));
    let cache: &'static crate::runtime::RuntimeCache =
        Box::leak(Box::new(crate::runtime::RuntimeCache::new()));
    cache
        .get_or_init(move || Ok(client))
        .expect("the scripted runtime always starts")
}

fn headless_shell_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(HeadlessFrontendPlugins);
    app.add_plugins(FrontendWindowPlugin {
        runtime: scripted_runtime(),
        palette: ui_palette(&Theme::dark()),
    });
    app.init_resource::<Assets<Font>>();
    app
}

/// Mount the Alerts page through the programmatic route seam (the resource
/// move + `RouteChanged` trigger pair the app module documents).
fn mount_alerts(app: &mut App) {
    // Set the route BEFORE the first update so the page mounts directly —
    // no intermediate page ever mounts in this fixture.
    app.world_mut().resource_mut::<Route>().page = Page::Alerts;
    app.update();
    assert_eq!(
        app.world_mut()
            .query_filtered::<&PageContent, ()>()
            .single(app.world())
            .expect("exactly one page content mounts")
            .page,
        Page::Alerts,
        "the alerts page is mounted"
    );
}

fn mounted_page_entity(world: &mut World) -> Entity {
    world
        .query_filtered::<Entity, With<PageContent>>()
        .single(world)
        .expect("exactly one page content mounts")
}

fn rule_target_entity(world: &mut World, rule_id: &str) -> Entity {
    let mut targets = world.query_filtered::<(Entity, &AlertRuleToggleTarget), ()>();
    targets
        .iter(world)
        .find(|(_, target)| target.0 == rule_id)
        .map(|(entity, _)| entity)
        .unwrap_or_else(|| panic!("the {rule_id} toggle row must be mounted"))
}

fn alert(metric: AlertMetric, rule_id: &str, target: &str, value: f32, threshold: f32) -> Alert {
    Alert {
        instance_id: format!("{rule_id}:{target}"),
        rule_id: rule_id.to_owned(),
        target: target.to_owned(),
        metric,
        severity: AlertSeverity::Warning,
        value,
        threshold,
        active_since_ms: 12_345,
    }
}

// ---- pure row projections ----

#[test]
fn active_alert_line_joins_the_clear_band_from_the_canonical_rule() {
    let shell = ShellApp::new();
    let rules = shell.projection().alert_center.managed_rules();
    // The default "cpu-high" rule is threshold 90 with hysteresis 10.
    let line = active_alert_line(
        &alert(AlertMetric::CpuUsagePercent, "cpu-high", "CPU", 96.0, 90.0),
        rules,
    );
    assert_eq!(
        line, "Warning · CPU — 96.0% ≥ 90.0% (clears ≤ 80.0%)",
        "the row states the firing comparison and the hysteresis clear band"
    );
}

#[test]
fn active_alert_line_uses_the_metric_unit_and_target() {
    let shell = ShellApp::new();
    let rules = shell.projection().alert_center.managed_rules();
    // The default "disk-temperature" rule is threshold 70 with hysteresis 5.
    let line = active_alert_line(
        &alert(
            AlertMetric::DiskTemperatureC,
            "disk-temperature",
            "nvme0n1",
            75.0,
            70.0,
        ),
        rules,
    );
    assert_eq!(
        line, "Warning · nvme0n1 — 75.0°C ≥ 70.0°C (clears ≤ 65.0°C)",
        "temperature rows carry °C and the disk target, not a fabricated %"
    );
}

#[test]
fn active_alert_line_omits_the_clear_band_when_the_rule_left_the_set() {
    let line = active_alert_line(
        &alert(
            AlertMetric::CpuUsagePercent,
            "removed-rule",
            "CPU",
            96.0,
            90.0,
        ),
        &[],
    );
    assert!(
        line.contains("rule not in set"),
        "a missing rule is stated, not smoothed over: {line}"
    );
    assert!(
        !line.contains("clears ≤"),
        "no clear band is invented without the rule's hysteresis"
    );
}

#[test]
fn alerts_summary_counts_participation_through_the_canonical_edit_entry() {
    let mut shell = ShellApp::new();
    assert_eq!(
        alerts_summary(&shell),
        "0 active · 5/5 rules enabled",
        "a fresh shell reports the honest zero"
    );
    shell
        .edit_alert_rules(taskmanager_application::ManagedAlertRuleEdit::Toggle {
            rule_id: "cpu-high".to_owned(),
        })
        .expect("the default rule id resolves");
    assert_eq!(
        alerts_summary(&shell),
        "0 active · 4/5 rules enabled",
        "one canonical toggle flips exactly one participation flag"
    );
}

#[test]
fn managed_rule_line_labels_disabled_rules_as_present_not_deleted() {
    let mut shell = ShellApp::new();
    let cpu_line = managed_rule_line(
        shell
            .projection()
            .alert_center
            .managed_rules()
            .iter()
            .find(|managed| managed.rule.id == "cpu-high")
            .expect("the default cpu rule exists"),
    );
    assert_eq!(cpu_line, "Warning · ≥ 90.0% — enabled");
    shell
        .edit_alert_rules(taskmanager_application::ManagedAlertRuleEdit::Toggle {
            rule_id: "cpu-high".to_owned(),
        })
        .expect("the toggle applies");
    let disabled_line = managed_rule_line(
        shell
            .projection()
            .alert_center
            .managed_rules()
            .iter()
            .find(|managed| managed.rule.id == "cpu-high")
            .expect("a disabled rule stays in the canonical set"),
    );
    assert!(disabled_line.contains("disabled"), "{disabled_line}");
}

// ---- wired observer chain ----

#[test]
fn alerts_page_renders_the_empty_state_and_canonical_rules() {
    let mut app = headless_shell_app();
    mount_alerts(&mut app);
    let world = app.world_mut();
    let texts = world
        .query::<&Text>()
        .iter(world)
        .map(|text| text.0.clone())
        .collect::<Vec<String>>();
    assert!(
        texts.iter().any(|text| text == "No active alerts"),
        "the empty state is explicit, not a blank success"
    );
    assert!(
        texts
            .iter()
            .any(|text| text == "0 active · 5/5 rules enabled"),
        "the summary counts render: {texts:?}"
    );
    assert!(
        texts.iter().any(|text| text == "No recent alert events"),
        "the notification-history empty state is honest about its state"
    );
    let mut toggles = world.query_filtered::<(Entity, &AlertRuleToggleTarget), ()>();
    let rows = toggles.iter(world).collect::<Vec<_>>();
    assert_eq!(rows.len(), 5, "one toggle row per canonical rule");
    let mut checked = world.query_filtered::<Has<Checked>, With<AlertRuleToggleTarget>>();
    for entity in toggles.iter(world).map(|(entity, _)| entity) {
        assert!(
            checked.get(world, entity).unwrap_or(false),
            "every default-enabled rule renders its checked state"
        );
    }
}

#[test]
fn checkbox_activation_applies_the_shell_edit_and_remounts_fresh_state() {
    let mut app = headless_shell_app();
    mount_alerts(&mut app);
    let before = mounted_page_entity(app.world_mut());
    let cpu_toggle = rule_target_entity(app.world_mut(), "cpu-high");

    // Deactivate through the same event the official checkbox emits.
    app.world_mut()
        .commands()
        .trigger(bevy::ui_widgets::ValueChange::<bool> {
            source: cpu_toggle,
            value: false,
            is_final: true,
        });
    app.update();

    let track = app.world().non_send::<FrontendTrack>();
    let cpu_enabled = track
        .shell
        .projection()
        .alert_center
        .managed_rules()
        .iter()
        .find(|managed| managed.rule.id == "cpu-high")
        .expect("the rule stays in the canonical set")
        .enabled;
    assert!(
        !cpu_enabled,
        "the activation flowed through the shell's edit entry"
    );
    assert_ne!(
        mounted_page_entity(app.world_mut()),
        before,
        "the page remounted so the fresh projection renders"
    );
    // The remounted tree mirrors the fresh projection, not the event: the
    // cpu-high checkbox lost its Checked marker, the others kept theirs.
    let cpu_toggle = rule_target_entity(app.world_mut(), "cpu-high");
    let mut checked = app
        .world_mut()
        .query_filtered::<Has<Checked>, With<AlertRuleToggleTarget>>();
    assert!(
        !checked.get(app.world(), cpu_toggle).unwrap_or(false),
        "the disabled rule renders unchecked"
    );
    let memory_toggle = rule_target_entity(app.world_mut(), "memory-high");
    assert!(
        checked.get(app.world(), memory_toggle).unwrap_or(false),
        "untouched rules keep their checked state"
    );

    // A repeated identical activation is a guarded no-op, not a double flip.
    app.world_mut()
        .commands()
        .trigger(bevy::ui_widgets::ValueChange::<bool> {
            source: cpu_toggle,
            value: false,
            is_final: true,
        });
    app.update();
    let track = app.world().non_send::<FrontendTrack>();
    let cpu_enabled = track
        .shell
        .projection()
        .alert_center
        .managed_rules()
        .iter()
        .find(|managed| managed.rule.id == "cpu-high")
        .expect("the rule is still present")
        .enabled;
    assert!(
        !cpu_enabled,
        "re-sending the same desired state must not flip the rule back on"
    );
}

#[test]
fn fold_event_remounts_the_page_and_idle_frames_redraw_nothing() {
    let mut app = headless_shell_app();
    mount_alerts(&mut app);
    let mounted = mounted_page_entity(app.world_mut());

    app.world_mut().commands().trigger(ShellProjectionFolded);
    app.update();
    let refreshed = mounted_page_entity(app.world_mut());
    assert_ne!(
        refreshed, mounted,
        "a fold event asks the mount system for a fresh projection read"
    );

    // Idle: no folded batches, no activations — the mounted page must not
    // churn (the page observer is the only alerts-page refresh trigger).
    app.update();
    app.update();
    assert_eq!(
        mounted_page_entity(app.world_mut()),
        refreshed,
        "idle frames redraw nothing"
    );
}

// ---- bare-world assembly (the shared page-census shape, scoped here
// because the shared test currently cannot pass for unrelated pages) ----

#[test]
fn pages_assemble_and_despawn_in_a_bare_scene_world() {
    // The shared page census spawns every page scene in a bare
    // MinimalPlugins world with no app resources; a page whose scene needs a
    // resource at spawn or observer-dispatch time breaks it. The page
    // embed only event-triggered observers, so they must assemble, census,
    // and despawn cleanly with no resources present.
    let mut app = App::new();
    app.add_plugins(bevy::MinimalPlugins);
    app.add_plugins((
        bevy::asset::AssetPlugin::default(),
        bevy::scene::ScenePlugin,
    ));
    app.init_resource::<Assets<Font>>();
    let world = app.world_mut();

    let fixture_shell = ShellApp::new();
    let fixture_palette = ui_palette(&Theme::dark());
    let fixture_history = crate::pages::history::HistoryProjectionResource::default();
    let process_tree_expansion = crate::pages::process_tree::ProcessTreeExpansion::default();
    let context = crate::app::PageContext {
        shell: &fixture_shell,
        process_tree_expansion: &process_tree_expansion,
        palette: &fixture_palette,
        history: &fixture_history.0,
    };
    for page in [Page::Alerts, Page::Settings] {
        let scene = crate::app::page_scene(page, &context);
        let root = world
            .spawn_scene(scene)
            .expect("the page scene resolves with no app resources")
            .id();
        let texts = world
            .query::<&Text>()
            .iter(world)
            .map(|text| text.0.clone())
            .collect::<Vec<String>>();
        assert!(
            texts.iter().any(|text| *text == page.title()),
            "the {} page renders its title bare",
            page.nav_label()
        );
        assert!(
            world.despawn(root),
            "the {} page despawns cleanly",
            page.nav_label()
        );
    }
}

#[test]
fn event_history_renders_recent_events() {
    use taskmanager_core::core::alerts::{
        Alert, AlertEvent, AlertEventKind, AlertMetric, AlertSeverity,
    };

    let alert = Alert {
        instance_id: "inst-1".into(),
        rule_id: "cpu-rule".into(),
        metric: AlertMetric::CpuUsagePercent,
        severity: AlertSeverity::Warning,
        target: "Global".into(),
        value: 85.0,
        threshold: 80.0,
        active_since_ms: 1000,
    };
    let event = AlertEvent {
        id: 1,
        kind: AlertEventKind::Activated,
        alert,
        observed_at_ms: 1050,
    };

    let line = super::event_history_line(&event);
    assert!(line.contains("[Activated]"));
    assert!(line.contains("Warning"));
    assert!(line.contains("85.0%"));
}

#[test]
fn alert_rule_export_and_import_round_trip() {
    use taskmanager_application::AlertRuleImportMode;
    use taskmanager_core::core::alerts::{AlertMetric, AlertRule, AlertSeverity};

    let mut shell = ShellApp::new();
    let json = super::export_alert_rules(&shell).expect("export rules");
    assert!(json.contains("cpu"));

    let custom = AlertRule::new(
        "bevy-custom",
        AlertMetric::CpuUsagePercent,
        AlertSeverity::Warning,
        90.0,
        std::time::Duration::from_secs(5),
        2.0,
    );
    let entries = [taskmanager_core::core::alerts::AlertRuleTransferEntry::new(
        custom, true,
    )];
    let custom_json = taskmanager_core::core::alerts::export_alert_rules_json(&entries).unwrap();

    let outcome = super::import_alert_rules(&mut shell, &custom_json, AlertRuleImportMode::Replace)
        .expect("import");
    assert_eq!(
        outcome,
        taskmanager_application::ManagedAlertRuleEditOutcome::Applied
    );
    assert_eq!(shell.projection().alert_center.managed_rules().len(), 1);
}

#[test]
fn alert_rule_authoring_creates_and_edits_rules() {
    let mut shell = ShellApp::new();
    let initial_count = shell.projection().alert_center.managed_rules().len();

    // 1. Create rule
    let outcome = super::create_alert_rule(
        &mut shell,
        "custom-cpu-rule",
        AlertMetric::CpuUsagePercent,
        AlertSeverity::Critical,
        88.0,
        4.0,
    )
    .expect("create rule");
    assert_eq!(
        outcome,
        taskmanager_application::ManagedAlertRuleEditOutcome::Applied
    );
    let rules = shell.projection().alert_center.managed_rules();
    assert_eq!(rules.len(), initial_count + 1);
    let created = rules
        .iter()
        .find(|r| r.rule.id == "custom-cpu-rule")
        .unwrap();
    assert_eq!(created.rule.threshold, 88.0);
    assert_eq!(created.rule.severity, AlertSeverity::Critical);

    // 2. Edit rule
    let edit_outcome = super::edit_alert_rule(
        &mut shell,
        "custom-cpu-rule".to_string(),
        AlertMetric::CpuUsagePercent,
        AlertSeverity::Warning,
        95.0,
        5.0,
    )
    .expect("edit rule");
    assert_eq!(
        edit_outcome,
        taskmanager_application::ManagedAlertRuleEditOutcome::Applied
    );
    let updated_rules = shell.projection().alert_center.managed_rules();
    let updated = updated_rules
        .iter()
        .find(|r| r.rule.id == "custom-cpu-rule")
        .unwrap();
    assert_eq!(updated.rule.threshold, 95.0);
    assert_eq!(updated.rule.severity, AlertSeverity::Warning);

    // 3. Line formatting
    let line =
        super::rule_authoring_line(AlertMetric::CpuUsagePercent, 85.0, AlertSeverity::Warning);
    assert!(line.contains("CPU Usage"));
    assert!(line.contains("≥ 85.0%"));
    assert!(line.contains("Warning"));
}

#[test]
fn alert_rule_authoring_intent_declared() {
    let declaration = crate::functional::functional_declaration();
    let entry = declaration
        .entries
        .iter()
        .find(|e| e.intent == taskmanager_ui_contract::ProductIntent::AlertRuleAuthoring)
        .expect("AlertRuleAuthoring declared");
    assert_eq!(
        entry.decision,
        taskmanager_ui_contract::SurfaceDecision::Local {
            route: "alerts.page.authoring",
        }
    );
}
