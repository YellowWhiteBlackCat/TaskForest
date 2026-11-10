//! Small-window policy shared by the production window and headless tests.

use crate::gpui_app::elements;
use crate::gpui_app::sidebar::{NetworkVisibility, SelectedDevice};
use crate::gpui_app::theme::mono_font_with_fallback;
use gpui::{
    Context, InteractiveElement, IntoElement, ParentElement, Pixels, Size,
    StatefulInteractiveElement, Styled, div, px, rems, size,
};
use taskforest_core::core::config::SidebarDeviceOverrideConfig;
use taskforest_core::core::metrics::SystemSnapshot;
use taskforest_core::core::{PowerSupplySnapshot, SensorCenterSnapshot, SensorQuantity};
use taskmanager_application::i18n;
use taskmanager_theme::{GlassTier, Theme, tokens};
use taskmanager_ui::icons_binding::icon;
use taskmanager_ui::presentation_binding::control_measurement;
use taskmanager_ui::theme_binding::definite_length;
use taskmanager_ui::theme_binding::font_size;
use taskmanager_ui::theme_binding::font_weight;
use taskmanager_ui::theme_binding::glass_fill;
use taskmanager_ui::theme_binding::glass_shadow;
use taskmanager_ui::theme_binding::hsla;
use taskmanager_ui::theme_binding::rem_length;
use taskmanager_ui::theme_binding::rem_value;
use taskmanager_ui_contract::FrontendShape;
use taskmanager_ui_contract::IconId;
use taskmanager_ui_contract::navigation::NavOrientation;
use taskmanager_ui_contract::presentation::budget::{
    ContentSlot, PerformanceSlotRequest, vertical_space_for_height,
};
use taskmanager_ui_contract::presentation::budget::{
    PageLayoutBudget, PerformancePageBudget, SystemPageBudget,
};
use taskmanager_ui_contract::presentation::thresholds::{LayoutAxis, LayoutBands, bands, number};
use taskmanager_ui_contract::presentation::vocabulary::{
    LayoutProfile, MAX_WINDOW_HEIGHT, MAX_WINDOW_WIDTH, NAV_RAIL_WIDTH, NavigationPresentation,
    VerticalSpace,
};

use super::RootView;

mod dashboard;
pub(crate) mod device_strip;
pub(crate) use dashboard::{DashboardBudget, DashboardSummaries};

// The window floor is a per-shape band (the terminal and Bevy spend none), so
// it has one address: the threshold registry this shape declares against.
pub const MIN_WIDTH: f32 = number(FrontendShape::Gpui, LayoutAxis::MinimumWindowWidth);
pub const MIN_HEIGHT: f32 = number(FrontendShape::Gpui, LayoutAxis::MinimumWindowHeight);
// The registered layout bands resolve through the threshold registry — this
// shape never restates a number it has already declared there (ADR-064).
pub const PERFORMANCE_STATS_MIN_WIDTH: f32 =
    number(FrontendShape::Gpui, LayoutAxis::PerformanceStatsMinWidth);
pub const PERFORMANCE_STATS_MAX_WIDTH: f32 =
    number(FrontendShape::Gpui, LayoutAxis::PerformanceStatsMaxWidth);
pub const NAVIGATION_VERTICAL_INSET: f32 = 14.0;
pub const ACTIVE_ALERT_HEIGHT: f32 = 32.0;
pub const NAVIGATION_HORIZONTAL_INSET: f32 =
    number(FrontendShape::Gpui, LayoutAxis::NavigationHorizontalInset);

fn shape_bands() -> LayoutBands {
    bands(FrontendShape::Gpui)
}

#[must_use]
pub fn performance_budget_from_frame(
    frame: FrameBudget,
    sidebar_visible: bool,
    persisted_sidebar_width: f32,
) -> PerformancePageBudget {
    performance_frame_from_frame(frame, sidebar_visible, persisted_sidebar_width).budget
}

/// The GPUI Performance frame: the shared verdict plus the inputs it was
/// derived from, so a capture can report what was actually resolved.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GpuiPerformanceFrame {
    pub budget: PerformancePageBudget,
    /// The width the horizontal bucket was classified from.
    pub width_input: f32,
    /// The content height the rung ladder classified from.
    pub content_height: f32,
    /// The page padding the frame earned.
    pub page_padding: f32,
    /// The horizontal bucket the frame earned.
    pub profile: LayoutProfile,
    /// The vertical capacity the frame classified.
    pub vertical_space: VerticalSpace,
}

#[must_use]
pub fn performance_frame_from_frame(
    frame: FrameBudget,
    sidebar_visible: bool,
    persisted_sidebar_width: f32,
) -> GpuiPerformanceFrame {
    let workspace_width =
        (f32::from(frame.content.size.width) + frame.content.page_padding).max(0.0);
    let content_height = f32::from(frame.content.size.height).max(0.0);
    let budget = PerformancePageBudget::allocate(
        PerformanceSlotRequest {
            profile: frame.content.profile,
            workspace_width,
            content_height,
            page_padding: frame.content.page_padding,
            sidebar_visible,
            persisted_sidebar_width: persisted_sidebar_width
                .is_finite()
                .then_some(persisted_sidebar_width),
        },
        &shape_bands(),
    );
    GpuiPerformanceFrame {
        budget,
        width_input: f32::from(frame.content.size.width),
        content_height,
        page_padding: frame.content.page_padding,
        profile: frame.content.profile,
        vertical_space: frame.content.vertical_space,
    }
}

#[must_use]
pub fn system_budget_from_frame(frame: FrameBudget) -> SystemPageBudget {
    SystemPageBudget::from_page_layout(frame.page_layout())
}

/// Build the capture marker text for one resolved GPUI frame.
///
/// The values are the ones the frame ACTUALLY resolved; the shared contract
/// verifies them against the same folds it mandates (ADR-064).
#[must_use]
pub fn presentation_resolution_marker(
    frame: &GpuiPerformanceFrame,
    ui_size: tokens::UiSize,
    density: tokens::RowDensity,
    sidebar_visible: bool,
    persisted_sidebar_width: Option<f32>,
) -> String {
    use taskmanager_ui_contract::presentation::resolution::{
        ResolvedPerformance, ResolvedPresentation, resolution_marker_text,
    };
    use taskmanager_ui_contract::presentation::sizes::{ControlRole, ControlState};

    let budget = frame.budget;
    let measured = control_measurement(ControlRole::Button, ControlState::Normal, ui_size, density);
    let resolved = ResolvedPresentation {
        profile: frame.profile,
        vertical_space: frame.vertical_space,
        page_padding: frame.page_padding,
        width_input: frame.width_input,
        page_content_height: frame.content_height,
        control_height: ui_size.control_height().0,
        body_font_size: ui_size.body_font_size().0,
        renderer_scale: ui_size.renderer_scale(),
        row_padding_y: density.row_padding_y().0,
        line_height: density.line_height().0,
        control_extent: measured.extent,
        hit_target: measured.hit_target,
        performance: Some(ResolvedPerformance {
            runway: budget.vertical,
            chart_inventory: budget.chart_inventory,
            details: budget.details,
            device_navigation: budget.device_navigation,
            workspace_width: frame.width_input + frame.page_padding,
            content_height: frame.content_height,
            sidebar_visible,
            persisted_sidebar_width,
            sidebar_width: budget.sidebar_width,
            stats_width: budget.stats_width,
            main_width: budget.main_width,
        }),
    };
    resolution_marker_text(&resolved)
}

/// The page's actual content slot after navigation and shell chrome have been
/// removed. This is the only horizontal/vertical capacity input that page
/// policy should consume.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContentBudget {
    pub size: Size<Pixels>,
    pub profile: LayoutProfile,
    pub vertical_space: VerticalSpace,
    pub page_padding: f32,
}

impl ContentBudget {
    fn from_body(body: Size<Pixels>) -> Self {
        let slot = ContentSlot::for_body(
            f32::from(body.width),
            f32::from(body.height),
            &shape_bands(),
        );
        Self {
            size: size(px(slot.width), px(slot.height)),
            profile: slot.profile,
            vertical_space: slot.vertical_space,
            page_padding: slot.page_padding,
        }
    }
}

/// Root-level geometry projection. The root computes this once per render and
/// passes the immutable value to every page. It intentionally contains no
/// page state, persistence, or toolkit widgets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameBudget {
    pub viewport: Size<Pixels>,
    pub body: Size<Pixels>,
    pub content: ContentBudget,
    pub navigation: NavigationPresentation,
    pub navigation_width: f32,
}

/// Shell dimensions supplied by the root renderer. The native titlebar is not
/// part of the GPUI viewport, so `has_csd_titlebar` controls whether it is
/// deducted; the horizontal app navigation strip is always deducted when it
/// is present.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameChromeBudget {
    pub titlebar_height: f32,
    pub has_csd_titlebar: bool,
    pub alert_height: f32,
}

impl FrameChromeBudget {
    #[must_use]
    pub fn new(titlebar_height: f32, has_csd_titlebar: bool, alert_visible: bool) -> Self {
        Self {
            titlebar_height: titlebar_height.max(0.0),
            has_csd_titlebar,
            alert_height: if alert_visible {
                ACTIVE_ALERT_HEIGHT
            } else {
                0.0
            },
        }
    }
}

impl FrameBudget {
    /// Build the production root allocation after shell chrome is known.
    #[must_use]
    pub fn for_root(
        viewport: Size<Pixels>,
        orientation: NavOrientation,
        chrome: FrameChromeBudget,
    ) -> Self {
        let navigation = navigation_for_width(viewport, orientation);
        let navigation_width = if matches!(orientation, NavOrientation::Vertical) {
            nav_rail_width(navigation) + NAVIGATION_HORIZONTAL_INSET
        } else {
            0.0
        };
        let horizontal_navigation_height = if matches!(orientation, NavOrientation::Horizontal) {
            chrome.titlebar_height + NAVIGATION_VERTICAL_INSET
        } else {
            0.0
        };
        let shell_height = if chrome.has_csd_titlebar {
            chrome.titlebar_height
        } else {
            0.0
        } + chrome.alert_height
            + horizontal_navigation_height;
        let body = size(
            px((f32::from(viewport.width) - navigation_width).max(0.0)),
            px((f32::from(viewport.height) - shell_height).max(0.0)),
        );
        Self {
            viewport,
            body,
            content: ContentBudget::from_body(body),
            navigation,
            navigation_width,
        }
    }

    /// Build the content-only projection used by legacy/headless callers that
    /// do not model the root chrome. Production rendering uses [`Self::for_root`].
    #[must_use]
    pub fn for_content(viewport: Size<Pixels>, orientation: NavOrientation) -> Self {
        let navigation = navigation_for_width(viewport, orientation);
        let navigation_width = if matches!(orientation, NavOrientation::Vertical) {
            nav_rail_width(navigation) + NAVIGATION_HORIZONTAL_INSET
        } else {
            0.0
        };
        let body = size(
            px((f32::from(viewport.width) - navigation_width).max(0.0)),
            viewport.height,
        );
        Self {
            viewport,
            body,
            content: ContentBudget::from_body(body),
            navigation,
            navigation_width,
        }
    }

    #[must_use]
    pub fn page_layout(self) -> PageLayoutBudget {
        PageLayoutBudget {
            profile: self.content.profile,
            vertical_space: self.content.vertical_space,
            page_padding: self.content.page_padding,
            navigation: self.navigation,
            content_width: f32::from(self.content.size.width),
            content_height: f32::from(self.content.size.height),
            chrome: None,
        }
    }
}

fn navigation_for_width(
    viewport: Size<Pixels>,
    orientation: NavOrientation,
) -> NavigationPresentation {
    let width = f32::from(viewport.width).max(0.0);
    match orientation {
        NavOrientation::Horizontal => {
            if ContentBudget::from_body(size(px(width), px(0.0))).profile
                == LayoutProfile::UltraCompact
            {
                NavigationPresentation::IconOnly
            } else {
                NavigationPresentation::Labeled
            }
        }
        NavOrientation::Vertical => {
            let labeled_body = size(
                px((width - NAV_RAIL_WIDTH - NAVIGATION_HORIZONTAL_INSET).max(0.0)),
                px(0.0),
            );
            if ContentBudget::from_body(labeled_body).profile == LayoutProfile::UltraCompact {
                NavigationPresentation::IconOnly
            } else {
                NavigationPresentation::Labeled
            }
        }
    }
}

#[must_use]
pub fn layout_profile(viewport: Size<Pixels>) -> LayoutProfile {
    LayoutProfile::for_content_width(f32::from(viewport.width))
}

#[must_use]
pub fn vertical_space(viewport: Size<Pixels>) -> VerticalSpace {
    vertical_space_for_height(f32::from(viewport.height), &shape_bands())
}

#[must_use]
pub fn page_layout_for_viewport(viewport: Size<Pixels>) -> PageLayoutBudget {
    FrameBudget::for_content(viewport, NavOrientation::Horizontal).page_layout()
}

#[must_use]
pub fn page_layout_for_frame(
    viewport: Size<Pixels>,
    orientation: NavOrientation,
) -> PageLayoutBudget {
    FrameBudget::for_content(viewport, orientation).page_layout()
}

#[must_use]
pub const fn nav_rail_width(presentation: NavigationPresentation) -> f32 {
    presentation.rail_width()
}

pub fn settings_content_max_height(viewport: Size<Pixels>) -> f32 {
    (f32::from(viewport.height) - 200.0).max(220.0)
}

/// Parse `WIDTHxHEIGHT` for deterministic capture runs. Invalid values retain
/// the normal 1180x780 launch size; valid values are clamped to the UI contract.
pub fn parse_window_size(value: &str) -> Option<Size<Pixels>> {
    let (width, height) = value.trim().split_once(['x', 'X'])?;
    let width = width.trim().parse::<f32>().ok()?;
    let height = height.trim().parse::<f32>().ok()?;
    if !width.is_finite() || !height.is_finite() {
        return None;
    }
    Some(size(
        px(width.clamp(MIN_WIDTH, MAX_WINDOW_WIDTH)),
        px(height.clamp(MIN_HEIGHT, MAX_WINDOW_HEIGHT)),
    ))
}

pub fn initial_window_size() -> Size<Pixels> {
    std::env::var("TM_WINDOW_SIZE")
        .ok()
        .and_then(|value| parse_window_size(&value))
        .unwrap_or_else(|| size(px(1180.0), px(780.0)))
}

pub fn disconnected_device(theme: &Theme, stable_id: Option<&str>) -> impl IntoElement {
    let layer = theme.glass_card();
    let mut card = div()
        .max_w(px(460.0))
        .p(px(f32::from(tokens::SPACE_18)))
        .rounded(rem_length(tokens::card_radius(theme)))
        .border(rems(GlassTier::Card.edge_width_rem()))
        .border_color(hsla(layer.edge))
        .bg(glass_fill(theme, GlassTier::Card))
        .shadow(glass_shadow(theme, GlassTier::Card))
        .flex()
        .flex_col()
        .gap(definite_length(tokens::SPACE_8))
        .child(
            div()
                .flex()
                .items_center()
                .gap(definite_length(tokens::SPACE_6))
                .font_weight(font_weight(tokens::FONT_WEIGHT_SEMIBOLD))
                .text_color(hsla(theme.fg))
                .child(
                    icon(IconId::Gpu)
                        .size(rem_value(16.0))
                        .text_color(hsla(theme.gpu)),
                )
                .child(i18n::t("device.disconnected")),
        )
        .child(
            div()
                .text_size(font_size(tokens::FONT_12))
                .text_color(hsla(theme.fg_dim))
                .child(i18n::t("device.reconnect_hint")),
        )
        .child(
            div()
                .text_size(font_size(tokens::FONT_11))
                .font(mono_font_with_fallback(theme))
                .text_color(hsla(theme.fg_dim))
                .child(
                    stable_id.map_or_else(crate::gpui_app::formatting::missing_value, String::from),
                ),
        );
    card = card.debug_selector(|| "tm-device-disconnected".to_string());
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .child(card)
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_root_responsive_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_root_layout_profile_parity_tests.rs"]
mod profile_parity_tests;
