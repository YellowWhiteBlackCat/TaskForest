// test-intent: behavior
//! The process-insights semantic capsules: the connection state and the
//! measured round-trip time wear rounded pills, and the seccomp mode plus
//! the Linux capability bits wear monospace tag backgrounds.

use super::*;
use iced::advanced::layout::{Layout, Limits};
use iced::advanced::renderer::Headless;
use iced::advanced::widget::{Id, Operation, Tree};
use iced::{Element, Rectangle, Size};
use taskforest_core::core::device_state::DeviceState;
use taskforest_core::core::process_telemetry::{
    CapabilityRiskLevel, ConnectionAddressFamily, ConnectionEndpoint, ConnectionState,
    ConnectionTransport, ProcessCapabilities, ProcessConnection,
};
use taskmanager_assets::embedded_fonts;
use taskmanager_test_support::pin_english;
use taskmanager_theme::Theme;

#[derive(Default)]
struct RenderedTextFragments(Vec<(String, Rectangle)>);

impl Operation for RenderedTextFragments {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation)) {
        visit(self);
    }

    fn text(&mut self, _id: Option<&Id>, bounds: Rectangle, fragment: &str) {
        self.0.push((fragment.to_owned(), bounds));
    }
}

fn software_renderer() -> iced::Renderer {
    {
        let mut fonts = iced::advanced::graphics::text::font_system()
            .write()
            .expect("font system");
        for font in embedded_fonts() {
            fonts.load_font(font);
        }
    }
    iced::futures::executor::block_on(iced::Renderer::new(
        crate::theme_binding::BUNDLED_UI_FONT,
        iced::Pixels(16.0),
        Some("tiny-skia"),
    ))
    .expect("software renderer")
}

fn rendered_text(
    element: &mut Element<'_, Message, iced::Theme, iced::Renderer>,
    renderer: &iced::Renderer,
) -> Vec<(String, Rectangle)> {
    let mut tree = Tree::new(&*element);
    let node = element.as_widget_mut().layout(
        &mut tree,
        renderer,
        &Limits::new(Size::ZERO, Size::new(720.0, 480.0)),
    );
    let mut fragments = RenderedTextFragments::default();
    element
        .as_widget_mut()
        .operate(&mut tree, Layout::new(&node), renderer, &mut fragments);
    fragments.0
}

fn has_visible_text(fragments: &[(String, Rectangle)], expected: &str) -> bool {
    fragments
        .iter()
        .any(|(fragment, bounds)| fragment == expected && bounds.width > 0.0 && bounds.height > 0.0)
}

fn connection(state: ConnectionState, rtt_ms: Option<f32>) -> ProcessConnection {
    ProcessConnection {
        transport: ConnectionTransport::Tcp,
        family: ConnectionAddressFamily::Ipv4,
        local: ConnectionEndpoint::Ip("127.0.0.1:80".parse().expect("endpoint")),
        remote: ConnectionEndpoint::Ip("10.0.0.2:443".parse().expect("endpoint")),
        state,
        provider_key: None,
        rtt_ms,
    }
}

/// The typed state maps onto the three semantic roles, and each role wears
/// the theme's own semantic token.
#[test]
fn the_connection_state_roles_use_the_theme_semantic_tokens() {
    let theme = Theme::dark();
    let palette = theme.palette();

    assert_eq!(
        pills::ConnectionStateRole::of(ConnectionState::Established),
        pills::ConnectionStateRole::Active
    );
    assert_eq!(
        pills::ConnectionStateRole::of(ConnectionState::TimeWait),
        pills::ConnectionStateRole::Transition
    );
    assert_eq!(
        pills::ConnectionStateRole::of(ConnectionState::Closed),
        pills::ConnectionStateRole::Inactive
    );

    assert_eq!(
        pills::ConnectionStateRole::Active.fill(&theme),
        palette.success,
        "an established session wears the theme's success token"
    );
    assert_eq!(
        pills::ConnectionStateRole::Transition.fill(&theme),
        palette.warning,
        "a closing session wears the theme's warning token"
    );
}

/// The state is rendered as a capsule carrying the typed state name, next to
/// the endpoints, and the measured RTT rides its own capsule.
#[test]
fn the_connection_row_renders_the_state_and_rtt_as_capsules() {
    pin_english();
    let theme = Theme::dark();
    let renderer = software_renderer();

    let mut row = pills::connection_row(
        &theme,
        &connection(ConnectionState::Established, Some(16.5)),
        "RTT",
    );
    let fragments = rendered_text(&mut row, &renderer);
    assert!(
        has_visible_text(&fragments, "ESTABLISHED"),
        "the typed state must be visible: {fragments:?}"
    );
    assert!(
        has_visible_text(&fragments, "TCP · EXTERNAL"),
        "the transport and scope stay visible: {fragments:?}"
    );
    assert!(
        has_visible_text(&fragments, "127.0.0.1:80 → 10.0.0.2:443"),
        "the endpoints stay visible: {fragments:?}"
    );
    assert!(
        has_visible_text(&fragments, "RTT 16.5 ms"),
        "the measured round-trip time rides its own capsule: {fragments:?}"
    );
}

/// An absent round-trip time stays absent: the capsule is never fabricated
/// from a zero.
#[test]
fn an_unmeasured_round_trip_time_produces_no_capsule() {
    let theme = Theme::dark();
    assert!(
        pills::rtt_pill(&theme, None, "RTT").is_none(),
        "an unmeasured round-trip time must not render a capsule"
    );
    assert!(
        pills::rtt_pill(&theme, Some(f32::NAN), "RTT").is_none(),
        "a non-finite round-trip time must not render a capsule"
    );
    assert!(
        pills::rtt_pill(&theme, Some(-1.0), "RTT").is_none(),
        "a negative round-trip time must not render a capsule"
    );
    assert!(
        pills::rtt_pill(&theme, Some(0.0), "RTT").is_some(),
        "a measured zero round-trip time is a real observation"
    );
}

/// The capability row renders one monospace chip per effective capability
/// bit, with the measured risk level beside them.
#[test]
fn the_capability_row_renders_one_chip_per_effective_bit() {
    pin_english();
    let theme = Theme::dark();
    let renderer = software_renderer();
    let capabilities = ProcessCapabilities::from_masks(
        DeviceState::healthy(1),
        Some(0),
        Some(1 << 21),
        Some(1 << 21),
        Some(0),
        Some(0),
    );
    assert_eq!(
        capabilities.risk_level(),
        CapabilityRiskLevel::Critical,
        "the fixture's effective bit is a critical capability"
    );

    let mut row = chips::capability_chips(&theme, &capabilities);
    let fragments = rendered_text(&mut row, &renderer);
    assert!(
        has_visible_text(&fragments, "CAP_SYS_ADMIN"),
        "the effective capability bit must be visible: {fragments:?}"
    );
    assert!(
        fragments
            .iter()
            .any(|(fragment, _)| fragment.contains("Critical")),
        "the measured risk level stays visible: {fragments:?}"
    );
}

/// The seccomp mode wears the same monospace tag background.
#[test]
fn the_seccomp_mode_renders_as_a_monospace_chip() {
    pin_english();
    let theme = Theme::dark();
    let renderer = software_renderer();

    let mut chip = chips::mono_chip(&theme, "2");
    let fragments = rendered_text(&mut chip, &renderer);
    assert!(
        has_visible_text(&fragments, "2"),
        "the seccomp mode must be visible as a chip: {fragments:?}"
    );
}
