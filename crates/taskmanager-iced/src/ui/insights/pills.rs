//! The semantic pills of the process-insights network facet: the connection
//! state and the measured round-trip time wear rounded capsules whose fill
//! comes from the theme's semantic tokens.
//!
//! The state vocabulary is the core's typed [`ConnectionState`]; the pill
//! maps it onto three semantic roles (live / transitioning / quiescent) so
//! the reader sees the lifecycle at a glance instead of parsing RFC state
//! names. No color literal enters this module: the fills are the theme's
//! success / warning / muted tokens and the foregrounds are picked from the
//! fill's luminance.

use iced::widget::{container, row, text};
use iced::{Element, Length};
use taskmanager_core::core::process_telemetry::{
    ConnectionAddressFamily, ConnectionState, ConnectionTransport, ProcessConnection,
};
use taskmanager_theme::color::on_accent;
use taskmanager_theme::{Color, Theme, tokens};
use zhuge_ui_kit_iced::surface::{self, BadgeGeometry};

use crate::app::Message;

/// The pill's box height — the shared badge pill height.
const PILL_HEIGHT: f32 = 20.0;

/// The semantic role of one typed connection state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ConnectionStateRole {
    /// A live data-transfer session (`ESTABLISHED`).
    Active,
    /// A session that is opening, listening or closing.
    Transition,
    /// A quiescent, connectionless or unrecognized session.
    Inactive,
}

impl ConnectionStateRole {
    /// The role of one typed connection state.
    #[must_use]
    pub(crate) fn of(state: ConnectionState) -> Self {
        match state {
            ConnectionState::Established => Self::Active,
            ConnectionState::SynSent
            | ConnectionState::SynReceived
            | ConnectionState::Listen
            | ConnectionState::FinWait1
            | ConnectionState::FinWait2
            | ConnectionState::TimeWait
            | ConnectionState::CloseWait
            | ConnectionState::LastAck
            | ConnectionState::Closing => Self::Transition,
            ConnectionState::Closed | ConnectionState::Unconnected | ConnectionState::Unknown => {
                Self::Inactive
            }
        }
    }

    /// The pill's fill: the theme's semantic token for the role.
    #[must_use]
    pub(crate) fn fill(self, theme: &Theme) -> Color {
        let palette = theme.palette();
        match self {
            Self::Active => palette.success,
            Self::Transition => palette.warning,
            Self::Inactive => palette.fg_muted,
        }
    }

    /// The pill's foreground on its own fill: luminance-picked for the
    /// semantic fills, the panel surface for the neutral one.
    #[must_use]
    pub(crate) fn foreground(self, theme: &Theme) -> Color {
        let palette = theme.palette();
        match self {
            Self::Active | Self::Transition => on_accent(self.fill(theme)),
            Self::Inactive => palette.surface,
        }
    }
}

/// The parts of one connection readout: the family-aware transport name, the
/// loopback/external scope, and the two endpoints.
pub(crate) struct ConnectionReadout {
    /// The transport name, family-aware (`TCP`, `TCP6`, `UDP`, `UDP6`).
    pub(crate) transport: String,
    /// Whether the endpoints are on the loopback interface.
    pub(crate) scope: &'static str,
    /// The `local → remote` endpoint pair.
    pub(crate) endpoints: String,
}

/// Split one connection into its readout parts. The transport naming is
/// family-aware so the local/remote endpoints are never dropped.
pub(crate) fn connection_readout(connection: &ProcessConnection) -> ConnectionReadout {
    let transport = match (&connection.transport, &connection.family) {
        (ConnectionTransport::Tcp, ConnectionAddressFamily::Ipv6) => "TCP6".to_string(),
        (ConnectionTransport::Udp, ConnectionAddressFamily::Ipv6) => "UDP6".to_string(),
        _ => connection.transport.to_string(),
    };
    let scope = if connection.is_loopback() {
        "LOOPBACK"
    } else {
        "EXTERNAL"
    };
    ConnectionReadout {
        transport,
        scope,
        endpoints: format!("{} → {}", connection.local, connection.remote),
    }
}

/// One connection row: the typed state as a semantic capsule, the transport
/// and scope, the two endpoints, and the measured round-trip time as a second
/// capsule. The state is no longer parsed out of a bracketed string — it is
/// the state the provider reported.
pub(crate) fn connection_row<'a>(
    theme_snapshot: &'a Theme,
    connection: &ProcessConnection,
    rtt_label: &str,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let readout = connection_readout(connection);
    let mut line = row![
        state_pill(theme_snapshot, connection.state),
        text(format!("{} · {}", readout.transport, readout.scope)).size(f32::from(tokens::FONT_12)),
        // The endpoints own the row's remaining width and stay inside it: a
        // long endpoint pair can never push a capsule past the panel's
        // right edge.
        container(text(readout.endpoints.clone()).size(f32::from(tokens::FONT_12)))
            .width(Length::Fill)
            .clip(true),
    ]
    .spacing(6)
    .align_y(iced::Alignment::Center);
    if let Some(rtt) = rtt_pill(theme_snapshot, connection.rtt_ms, rtt_label) {
        line = line.push(rtt);
    }
    line.into()
}

/// The connection-state pill: a fully rounded capsule carrying the typed
/// state name, filled with the state's semantic token.
pub(crate) fn state_pill<'a>(
    theme_snapshot: &'a Theme,
    state: ConnectionState,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let role = ConnectionStateRole::of(state);
    let fill = crate::theme_binding::color(role.fill(theme_snapshot));
    let foreground = crate::theme_binding::color(role.foreground(theme_snapshot));
    surface::badge(
        state.as_str(),
        BadgeGeometry {
            size: f32::from(tokens::FONT_CAPTION),
            font: None,
            padding: [0.0, f32::from(tokens::SPACE_8)].into(),
            height: Length::Fixed(PILL_HEIGHT),
            centered: true,
        },
        move |_| container::Style {
            background: Some(fill.into()),
            text_color: Some(foreground),
            border: iced::Border {
                color: iced::Color::TRANSPARENT,
                width: 0.0,
                radius: (PILL_HEIGHT / 2.0).into(),
            },
            ..container::Style::default()
        },
    )
}

/// The round-trip-time pill: a neutral capsule carrying the measured RTT.
///
/// `None` when the provider exposed no round-trip time (UDP, UNIX sockets,
/// permission failures) — the pill is never fabricated from a zero.
pub(crate) fn rtt_pill<'a>(
    theme_snapshot: &'a Theme,
    rtt_ms: Option<f32>,
    label: &str,
) -> Option<Element<'a, Message, iced::Theme, iced::Renderer>> {
    let rtt = rtt_ms.filter(|value| value.is_finite() && *value >= 0.0)?;
    let palette = theme_snapshot.palette();
    let fill = crate::theme_binding::color(palette.fg_muted);
    let foreground = crate::theme_binding::color(palette.surface);
    Some(surface::badge(
        format!("{label} {rtt:.1} ms"),
        BadgeGeometry {
            size: f32::from(tokens::FONT_CAPTION),
            font: None,
            padding: [0.0, f32::from(tokens::SPACE_8)].into(),
            height: Length::Fixed(PILL_HEIGHT),
            centered: true,
        },
        move |_| container::Style {
            background: Some(fill.into()),
            text_color: Some(foreground),
            border: iced::Border {
                color: iced::Color::TRANSPARENT,
                width: 0.0,
                radius: (PILL_HEIGHT / 2.0).into(),
            },
            ..container::Style::default()
        },
    ))
}
