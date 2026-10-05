//! Headless visual/projection tests for the process insights isolation card.

use super::*;
use gpui::{AppContext, Context, IntoElement, Render, TestAppContext, Window};
use taskmanager_core::core::device_state::DeviceState;
use taskmanager_core::core::process_telemetry::{
    IsolationKind, LinuxNamespaceAudit, LinuxNamespaceKind, NamespaceAuditEntry,
    NamespaceAuditStatus, ProcessCapabilities, ProcessIsolation, ProcessTelemetrySnapshot,
};
use taskmanager_ui::init;

fn labels() -> ProcessInsightsLabels {
    ProcessInsightsLabels::capture_fixture()
}

struct IsolationCardView {
    card: Div,
}

impl Render for IsolationCardView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        std::mem::replace(&mut self.card, div())
    }
}

fn draw_isolation_frame(cx: &mut TestAppContext, isolation: ProcessIsolation) {
    cx.update(init);
    let theme = Theme::dark();
    let snapshot = ProcessTelemetrySnapshot {
        isolation,
        ..ProcessTelemetrySnapshot::default()
    };
    let card = isolation_card(&theme, &snapshot.isolation, &labels(), 480.0);
    let window = cx.add_window(|_w, _cx| IsolationCardView { card });
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
}

#[gpui::test]
fn isolation_card_renders_sandbox_detection(cx: &mut TestAppContext) {
    draw_isolation_frame(
        cx,
        ProcessIsolation {
            state: DeviceState::healthy(1),
            kind: Some(IsolationKind::Docker),
            container_id: Some("c-docker-12345".into()),
            sandboxed: Some(true),
            ..ProcessIsolation::default()
        },
    );
}

#[gpui::test]
fn isolation_card_renders_posix_capabilities(cx: &mut TestAppContext) {
    draw_isolation_frame(
        cx,
        ProcessIsolation {
            state: DeviceState::healthy(1),
            capabilities: Some(ProcessCapabilities::from_masks(
                DeviceState::healthy(1),
                Some(0),
                Some(1 << 21),
                Some(1 << 21),
                Some(0),
                Some(0),
            )),
            ..ProcessIsolation::default()
        },
    );
}

#[gpui::test]
fn isolation_card_renders_namespace_audit(cx: &mut TestAppContext) {
    let audit = LinuxNamespaceAudit::from_entries(
        DeviceState::healthy(1),
        vec![
            NamespaceAuditEntry {
                kind: LinuxNamespaceKind::Pid,
                status: NamespaceAuditStatus::Isolated {
                    inode: 4026533000,
                    host_inode: 4026531836,
                },
            },
            NamespaceAuditEntry {
                kind: LinuxNamespaceKind::Mount,
                status: NamespaceAuditStatus::Host { inode: 4026531840 },
            },
        ],
    );
    draw_isolation_frame(
        cx,
        ProcessIsolation {
            state: DeviceState::healthy(1),
            namespaces: Some(audit),
            ..ProcessIsolation::default()
        },
    );
}

#[gpui::test]
fn isolation_card_renders_seccomp_filter(cx: &mut TestAppContext) {
    draw_isolation_frame(
        cx,
        ProcessIsolation {
            state: DeviceState::healthy(1),
            seccomp_mode: Some(2),
            ..ProcessIsolation::default()
        },
    );
}

#[test]
fn format_connection_renders_socket_inventory_and_rtt() {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use taskmanager_core::core::process_telemetry::{
        ConnectionAddressFamily, ConnectionEndpoint, ConnectionState, ConnectionTransport,
        ProcessConnection,
    };

    let connection = ProcessConnection {
        transport: ConnectionTransport::Tcp,
        family: ConnectionAddressFamily::Ipv4,
        local: ConnectionEndpoint::Ip(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            8080,
        )),
        remote: ConnectionEndpoint::Ip(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            45678,
        )),
        state: ConnectionState::Established,
        provider_key: None,
        rtt_ms: Some(14.8),
    };
    let line = formatting::format_connection(&connection);
    assert!(line.contains("TCP"), "must render TCP transport");
    assert!(line.contains("14.8 ms"), "must render observed RTT");
}
