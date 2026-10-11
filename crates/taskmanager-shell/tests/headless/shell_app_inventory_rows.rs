use super::*;

use crate::fixture::{ProjectionSeedDomain, ProjectionSeedFact, seed_projection_fact};
use taskmanager_core::core::services::{ServiceItem, ServiceStatus};
use taskmanager_core::core::session::SessionItem;
use taskmanager_core::core::startup::{
    StartupControlPolicy, StartupEntry, StartupEntryLocator, StartupImpact, StartupImpactEvidence,
    StartupImpactUnknownReason, StartupScope, StartupSource,
};
use taskmanager_core::core::target::{ServiceId, SessionId};

fn service(id: &str, name: &str) -> ServiceItem {
    ServiceItem::from_inventory(id, name, ServiceStatus::Active, "", "", "", "")
}

fn startup(id: &str, name: &str) -> StartupEntry {
    StartupEntry {
        id: id.into(),
        name: name.into(),
        exec: String::from("app"),
        enabled: true,
        source: StartupSource::DesktopEntry,
        scope: StartupScope::User,
        control_policy: StartupControlPolicy::Direct,
        locator: StartupEntryLocator::new("provider-locator"),
        impact: StartupImpact::None,
        impact_evidence: StartupImpactEvidence::Unknown {
            reason: StartupImpactUnknownReason::NotInstrumented,
        },
    }
}

fn session(id: &str, user: &str) -> SessionItem {
    SessionItem {
        id: SessionId::new(id),
        user: user.to_owned(),
        ..SessionItem::default()
    }
}

enum InventoryFixtureRows {
    Services(Vec<ServiceItem>),
    Startup(Vec<StartupEntry>),
    Sessions(Vec<SessionItem>),
}

#[derive(Clone, Copy)]
enum FixtureOrder {
    ZetaThenAlpha,
    AlphaThenZeta,
    AlphaOnly,
    DuplicateIds,
}

fn inventory_fixture_rows(domain: InventoryRowDomain, order: FixtureOrder) -> InventoryFixtureRows {
    let ids = match order {
        FixtureOrder::ZetaThenAlpha => [Some(("zeta", "Zeta")), Some(("alpha", "Alpha"))],
        FixtureOrder::AlphaThenZeta => [Some(("alpha", "Alpha")), Some(("zeta", "Zeta"))],
        FixtureOrder::AlphaOnly => [Some(("alpha", "Alpha")), None],
        FixtureOrder::DuplicateIds => [Some(("duplicate", "First")), Some(("duplicate", "Second"))],
    };
    match domain {
        InventoryRowDomain::Services => InventoryFixtureRows::Services(
            ids.into_iter()
                .flatten()
                .map(|(id, name)| service(&format!("service:{id}"), name))
                .collect(),
        ),
        InventoryRowDomain::Startup => InventoryFixtureRows::Startup(
            ids.into_iter()
                .flatten()
                .map(|(id, name)| startup(&format!("startup:{id}"), name))
                .collect(),
        ),
        InventoryRowDomain::Sessions => InventoryFixtureRows::Sessions(
            ids.into_iter()
                .flatten()
                .map(|(id, user)| session(&format!("session:{id}"), user))
                .collect(),
        ),
    }
}

fn seed_inventory_fixture(app: &mut ShellApp, rows: InventoryFixtureRows) {
    let fact = match rows {
        InventoryFixtureRows::Services(rows) => ProjectionSeedFact::Services(Some(rows)),
        InventoryFixtureRows::Startup(rows) => ProjectionSeedFact::StartupEntries(Some(rows)),
        InventoryFixtureRows::Sessions(rows) => ProjectionSeedFact::Sessions(Some(rows)),
    };
    seed_projection_fact(app, fact);
}

#[test]
fn row_ids_use_owner_facts_and_reject_missing_ids() {
    let first = InventoryRowId::from_service(&service("provider:one", "Same label"))
        .expect("provider service target");
    let second = InventoryRowId::from_service(&service("provider:two", "Same label"))
        .expect("second provider service target");
    assert_ne!(first, second, "labels do not collapse distinct service IDs");
    assert_eq!(
        InventoryRowId::from_service(&service("", "Visible legacy service")),
        None,
        "an empty legacy target stays display-only"
    );
    assert_eq!(
        InventoryRowId::from_startup(&startup("", "Visible startup row")),
        None,
        "startup labels and locators do not replace a missing owner ID"
    );
    assert_eq!(
        InventoryRowId::from_session(&session("", "visible-user")),
        None,
        "a user label does not replace a missing session target"
    );

    let service_id = InventoryRowId::Service(ServiceId::new("same-token"));
    let session_id = InventoryRowId::Session(SessionId::new("same-token"));
    assert_ne!(service_id, session_id, "owner domains remain distinct");
}

#[test]
fn inventory_selection_reconciles_by_identity_while_old_geometry_fails_closed() {
    let mut app = ShellApp::new();
    seed_projection_fact(
        &mut app,
        ProjectionSeedFact::Services(Some(vec![
            service("service:zeta", "Zeta"),
            service("service:alpha", "Alpha"),
        ])),
    );
    seed_projection_fact(
        &mut app,
        ProjectionSeedFact::StartupEntries(Some(vec![
            startup("startup:zeta", "Zeta"),
            startup("startup:alpha", "Alpha"),
        ])),
    );
    seed_projection_fact(
        &mut app,
        ProjectionSeedFact::Sessions(Some(vec![
            session("session:zeta", "Zeta"),
            session("session:alpha", "Alpha"),
        ])),
    );

    for domain in [
        InventoryRowDomain::Services,
        InventoryRowDomain::Startup,
        InventoryRowDomain::Sessions,
    ] {
        let selected = app
            .inventory_row_identity_at(domain, 0)
            .expect("provider row identity");
        let old_anchor = app
            .inventory_row_anchor_at(domain, 0)
            .expect("identity and projection generation");

        app.set_info_sort(domain.info_table(), InfoSortCol::Name);

        assert_eq!(
            app.reconcile_inventory_row_identity(&selected),
            Some(1),
            "stable selection follows the row when the canonical order changes: {domain:?}"
        );
        assert_eq!(
            app.resolve_inventory_row_anchor(&old_anchor),
            None,
            "old geometry is rejected after the sort changes: {domain:?}"
        );
        assert_eq!(
            InventoryRowAnchor::new(
                selected,
                app.inventory_projection_generation(match domain {
                    InventoryRowDomain::Services => InventoryRowDomain::Startup,
                    InventoryRowDomain::Startup => InventoryRowDomain::Sessions,
                    InventoryRowDomain::Sessions => InventoryRowDomain::Services,
                }),
            ),
            None,
            "row identity cannot be bound to another inventory's generation"
        );
    }
}

#[test]
fn inventory_refresh_reconciles_identity_and_rejects_stale_rows_for_all_domains() {
    for domain in [
        InventoryRowDomain::Services,
        InventoryRowDomain::Startup,
        InventoryRowDomain::Sessions,
    ] {
        let mut app = ShellApp::new();
        seed_inventory_fixture(
            &mut app,
            inventory_fixture_rows(domain, FixtureOrder::ZetaThenAlpha),
        );
        let selected = app
            .inventory_row_identity_at(domain, 0)
            .expect("provider row identity");
        let old_anchor = app
            .inventory_row_anchor_at(domain, 0)
            .expect("identity and projection generation");
        let old_revision = old_anchor.generation().revision();

        seed_inventory_fixture(
            &mut app,
            inventory_fixture_rows(domain, FixtureOrder::AlphaThenZeta),
        );

        assert_eq!(
            app.inventory_projection_generation(domain).revision(),
            old_revision.saturating_add(1),
            "refresh advances this domain's generation: {domain:?}"
        );
        assert_eq!(
            app.reconcile_inventory_row_identity(&selected),
            Some(1),
            "the same owner identity follows a refresh reorder: {domain:?}"
        );
        assert_eq!(
            app.resolve_inventory_row_anchor(&old_anchor),
            None,
            "old geometry is rejected after refresh: {domain:?}"
        );
        let current_anchor = app
            .inventory_row_anchor_at(domain, 1)
            .expect("reconciled identity has a current anchor");
        assert_eq!(
            current_anchor.id(),
            &selected,
            "the new row at the reconciled position has the selected ID: {domain:?}"
        );
        assert_eq!(
            app.resolve_inventory_row_anchor(&current_anchor),
            Some(1),
            "the current generation resolves to the current row: {domain:?}"
        );

        seed_inventory_fixture(
            &mut app,
            inventory_fixture_rows(domain, FixtureOrder::AlphaOnly),
        );
        assert_eq!(
            app.reconcile_inventory_row_identity(&selected),
            None,
            "a disappeared ID cannot resolve to its remaining neighbor: {domain:?}"
        );

        seed_inventory_fixture(
            &mut app,
            inventory_fixture_rows(domain, FixtureOrder::DuplicateIds),
        );
        let duplicate = app
            .inventory_row_identity_at(domain, 0)
            .expect("duplicate rows still carry their owner ID");
        let duplicate_anchor = app
            .inventory_row_anchor_at(domain, 0)
            .expect("the row carries the owner ID");
        assert_eq!(
            app.reconcile_inventory_row_identity(&duplicate),
            None,
            "duplicate owner IDs are ambiguous and fail closed: {domain:?}"
        );
        assert_eq!(
            app.resolve_inventory_row_anchor(&duplicate_anchor),
            None,
            "ambiguous owner IDs cannot resolve pointer geometry: {domain:?}"
        );
    }
}

#[test]
fn inventory_revisions_are_domain_scoped() {
    let mut app = ShellApp::new();
    let services = app.inventory_projection_generation(InventoryRowDomain::Services);
    let startup = app.inventory_projection_generation(InventoryRowDomain::Startup);
    let sessions = app.inventory_projection_generation(InventoryRowDomain::Sessions);

    seed_projection_fact(
        &mut app,
        ProjectionSeedFact::AdvanceRevision(ProjectionSeedDomain::Services),
    );

    assert_ne!(
        app.inventory_projection_generation(InventoryRowDomain::Services),
        services
    );
    assert_eq!(
        app.inventory_projection_generation(InventoryRowDomain::Startup),
        startup
    );
    assert_eq!(
        app.inventory_projection_generation(InventoryRowDomain::Sessions),
        sessions
    );
}
