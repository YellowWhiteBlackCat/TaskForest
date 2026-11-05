//! The resolution record: what a frame ACTUALLY resolved, and the shared
//! verifier that re-derives it from the contract.
//!
//! This is the evidence half of ADR-064. A capture that paints a frame also
//! records the presentation facts it resolved (page bucket, vertical capacity,
//! slot extents, the Performance arbitration, the control measurement). The
//! verifier re-runs the shared folds on those recorded inputs and reports any
//! disagreement, so "the picture looks right" becomes "the recorded resolution
//! is the one the contract mandates".
//!
//! The record is a TEXTUAL artifact contract: the fields are transported as
//! marker text, and the numbers are the shared ones. A shape that cannot spend
//! a field (the terminal shape has no pixel page slot) declares that absence
//! instead of fabricating a value.
//!
//! The design-ladder numbers ride in as values: ADR-063 keeps their owner in
//! `taskmanager-theme`, and this record stays a contract-owned artifact that
//! the verifier re-runs without importing a token codec.

use crate::keybindings::FrontendShape;
use crate::presentation::budget::{
    ContentSlot, PerformancePageBudget, PerformanceSlotRequest, vertical_space_for_height,
};
use crate::presentation::sizes::{
    ControlRole, ControlState, DensityPoint, UiSizePoint, resolve_size,
};
use crate::presentation::thresholds::LayoutBands;
use crate::presentation::vocabulary::{
    DeviceNavigationPresentation, LayoutProfile, PerformanceChartInventory,
    PerformanceDetailsPresentation, PerformanceVerticalRunway, VerticalSpace,
    chart_inventory_from_axes,
};

/// The control role whose resolved measurement a frame reports. The primary
/// activation control is the one every shape spends, so it is the one a
/// capture can compare across frontends.
pub const REPORTED_CONTROL_ROLE: ControlRole = ControlRole::Button;

/// The presentation facts one frame resolved. Every field is a shared fact:
/// a shape records what it resolved, and the verifier proves the record
/// follows the contract.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolvedPresentation {
    pub profile: LayoutProfile,
    pub vertical_space: VerticalSpace,
    pub page_padding: f32,
    /// The width the shape classified the horizontal axis from (its own
    /// chrome already deducted).
    pub width_input: f32,
    /// The height the shape classified the vertical capacity axis from.
    pub page_content_height: f32,
    /// The design-ladder values the frame resolved through. The theme owns
    /// the numbers (ADR-063); the record carries what the fold consumed so
    /// the verifier re-runs it without importing a token codec.
    pub control_height: f32,
    pub body_font_size: f32,
    pub renderer_scale: f32,
    pub row_padding_y: f32,
    pub line_height: f32,
    /// The control extent the frame resolved, in the shared logical px.
    pub control_extent: f32,
    /// The hit target the frame resolved, in the shared logical px.
    pub hit_target: Option<f32>,
    /// The Performance arbitration, when the frame rendered that page.
    pub performance: Option<ResolvedPerformance>,
}

/// The Performance-page facts one frame resolved.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolvedPerformance {
    pub runway: PerformanceVerticalRunway,
    pub chart_inventory: PerformanceChartInventory,
    pub details: PerformanceDetailsPresentation,
    pub device_navigation: DeviceNavigationPresentation,
    /// The workspace width the slot arbitration started from.
    pub workspace_width: f32,
    /// The content height the rung ladder classified from.
    pub content_height: f32,
    pub sidebar_visible: bool,
    pub persisted_sidebar_width: Option<f32>,
    pub sidebar_width: f32,
    pub stats_width: f32,
    pub main_width: f32,
}

/// One disagreement between a recorded resolution and the shared contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolutionFindingKind {
    ProfileMismatch,
    VerticalSpaceMismatch,
    PagePaddingMismatch,
    RunwayMismatch,
    ChartInventoryMismatch,
    DetailsMismatch,
    DeviceNavigationMismatch,
    SidebarWidthMismatch,
    StatsWidthMismatch,
    MainWidthMismatch,
    ControlExtentMismatch,
    HitTargetMismatch,
}

/// One finding about a recorded resolution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolutionFinding {
    pub frontend: FrontendShape,
    pub kind: ResolutionFindingKind,
}

/// The stable marker field order of a recorded resolution. Every shape
/// transports the SAME fields, so one parser reads all four frontends and a
/// missing field is a failure rather than a silently shorter line.
pub const RESOLUTION_MARKER_FIELDS: &[&str] = &[
    "profile",
    "vertical",
    "padding",
    "width_input",
    "page_content_height",
    "control_extent",
    "hit_target",
    "control_height",
    "body_font_size",
    "renderer_scale",
    "row_padding_y",
    "line_height",
    "rung",
    "inventory",
    "details",
    "device_nav",
    "sidebar_visible",
    "persisted_sidebar_width",
    "workspace_width",
    "content_height",
    "sidebar_width",
    "stats_width",
    "main_width",
];

/// Render one recorded resolution as the marker field text a capture
/// transports. The shape supplies its own values; this function only formats
/// them, so no frontend can spell a different field set.
#[must_use]
pub fn resolution_marker_text(resolved: &ResolvedPresentation) -> String {
    let performance = resolved.performance;
    format!(
        "profile={} vertical={} padding={:.2} width_input={:.2} page_content_height={:.2} \
         control_extent={:.2} hit_target={:.2} control_height={:.2} body_font_size={:.2} \
         renderer_scale={:.4} row_padding_y={:.2} line_height={:.2} rung={} inventory={} \
         details={} device_nav={} sidebar_visible={} persisted_sidebar_width={} \
         workspace_width={:.2} content_height={:.2} sidebar_width={:.2} stats_width={:.2} \
         main_width={:.2}",
        resolved.profile.token(),
        resolved.vertical_space.token(),
        resolved.page_padding,
        resolved.width_input,
        resolved.page_content_height,
        resolved.control_extent,
        resolved.hit_target.unwrap_or_default(),
        resolved.control_height,
        resolved.body_font_size,
        resolved.renderer_scale,
        resolved.row_padding_y,
        resolved.line_height,
        performance
            .map(|value| value.runway.token())
            .unwrap_or("none"),
        performance
            .map(|value| value.chart_inventory.token())
            .unwrap_or("none"),
        performance
            .map(|value| value.details.token())
            .unwrap_or("none"),
        performance
            .map(|value| value.device_navigation.token())
            .unwrap_or("none"),
        performance
            .map(|value| value.sidebar_visible.to_string())
            .unwrap_or_else(|| "false".to_owned()),
        performance
            .map(|value| match value.persisted_sidebar_width {
                Some(width) => format!("{width:.2}"),
                None => "none".to_owned(),
            })
            .unwrap_or_else(|| "none".to_owned()),
        performance
            .map(|value| value.workspace_width)
            .unwrap_or_default(),
        performance
            .map(|value| value.content_height)
            .unwrap_or_default(),
        performance
            .map(|value| value.sidebar_width)
            .unwrap_or_default(),
        performance
            .map(|value| value.stats_width)
            .unwrap_or_default(),
        performance
            .map(|value| value.main_width)
            .unwrap_or_default(),
    )
}

/// Parse a recorded resolution back from its marker text.
///
/// The parse is strict: an unknown token, a missing field or a malformed
/// number is a failure, because a capture that cannot be read back proves
/// nothing.
#[must_use]
pub fn parse_resolution_marker(text: &str) -> Option<ResolvedPresentation> {
    let mut fields = std::collections::HashMap::new();
    for token in text.split_whitespace() {
        // A record embedded in a capture marker line carries that line's own
        // prefix tokens (`ICED_CAPTURE_MARKER`, `event=viewport_ready`, …).
        // They are not part of the resolution vocabulary, so a token without
        // an `=` is skipped instead of failing the whole record.
        let Some((key, value)) = token.split_once('=') else {
            continue;
        };
        fields.insert(key, value);
    }
    let profile = LayoutProfile::from_token(fields.get("profile")?)?;
    let vertical_space = VerticalSpace::from_token(fields.get("vertical")?)?;
    let page_padding = fields.get("padding")?.parse::<f32>().ok()?;
    let width_input = fields.get("width_input")?.parse::<f32>().ok()?;
    let page_content_height = fields.get("page_content_height")?.parse::<f32>().ok()?;
    let control_extent = fields.get("control_extent")?.parse::<f32>().ok()?;
    let hit_target = fields.get("hit_target")?.parse::<f32>().ok();
    let control_height = fields.get("control_height")?.parse::<f32>().ok()?;
    let body_font_size = fields.get("body_font_size")?.parse::<f32>().ok()?;
    let renderer_scale = fields.get("renderer_scale")?.parse::<f32>().ok()?;
    let row_padding_y = fields.get("row_padding_y")?.parse::<f32>().ok()?;
    let line_height = fields.get("line_height")?.parse::<f32>().ok()?;
    let performance = match fields.get("rung") {
        Some(&"none") => None,
        Some(rung) => Some(ResolvedPerformance {
            runway: PerformanceVerticalRunway::from_token(rung)?,
            chart_inventory: PerformanceChartInventory::from_token(fields.get("inventory")?)?,
            details: PerformanceDetailsPresentation::from_token(fields.get("details")?)?,
            device_navigation: DeviceNavigationPresentation::from_token(fields.get("device_nav")?)?,
            workspace_width: fields.get("workspace_width")?.parse::<f32>().ok()?,
            content_height: fields.get("content_height")?.parse::<f32>().ok()?,
            sidebar_visible: fields.get("sidebar_visible")?.parse::<bool>().ok()?,
            persisted_sidebar_width: match fields.get("persisted_sidebar_width") {
                Some(&"none") => None,
                Some(value) => Some(value.parse::<f32>().ok()?),
                None => None,
            },
            sidebar_width: fields.get("sidebar_width")?.parse::<f32>().ok()?,
            stats_width: fields.get("stats_width")?.parse::<f32>().ok()?,
            main_width: fields.get("main_width")?.parse::<f32>().ok()?,
        }),
        None => None,
    };
    Some(ResolvedPresentation {
        profile,
        vertical_space,
        page_padding,
        width_input,
        page_content_height,
        control_height,
        body_font_size,
        renderer_scale,
        row_padding_y,
        line_height,
        control_extent,
        hit_target,
        performance,
    })
}

/// Verify one recorded resolution against the shared contract.
///
/// The verifier re-runs the same folds the frontend ran, on the frontend's own
/// measured inputs. It never re-derives the shape's chrome: the recorded
/// body rectangle is the input, exactly as the shape computed it.
#[must_use]
pub fn verify_resolved(
    frontend: FrontendShape,
    resolved: &ResolvedPresentation,
    bands: &LayoutBands,
) -> Vec<ResolutionFinding> {
    let mut findings = Vec::new();
    let mut push = |kind| {
        findings.push(ResolutionFinding { frontend, kind });
    };

    let expected_profile = ContentSlot::for_body(resolved.width_input, 0.0, bands).profile;
    if expected_profile != resolved.profile {
        push(ResolutionFindingKind::ProfileMismatch);
    }
    let expected_vertical = vertical_space_for_height(resolved.page_content_height, bands);
    if expected_vertical != resolved.vertical_space {
        push(ResolutionFindingKind::VerticalSpaceMismatch);
    }
    if (expected_profile.page_padding() - resolved.page_padding).abs() > 0.01 {
        push(ResolutionFindingKind::PagePaddingMismatch);
    }

    if let Some(performance) = resolved.performance {
        let expected_runway = PerformanceVerticalRunway::for_content_height(
            performance.content_height,
            bands.charts_rung_content_height,
        );
        if expected_runway != performance.runway {
            push(ResolutionFindingKind::RunwayMismatch);
        }
        if chart_inventory_from_axes(resolved.profile, expected_runway)
            != performance.chart_inventory
        {
            push(ResolutionFindingKind::ChartInventoryMismatch);
        }
        let allocated = PerformancePageBudget::allocate(
            PerformanceSlotRequest {
                profile: resolved.profile,
                workspace_width: performance.workspace_width,
                content_height: performance.content_height,
                page_padding: resolved.page_padding,
                sidebar_visible: performance.sidebar_visible,
                persisted_sidebar_width: performance.persisted_sidebar_width,
            },
            bands,
        );
        if allocated.details != performance.details {
            push(ResolutionFindingKind::DetailsMismatch);
        }
        if allocated.device_navigation != performance.device_navigation {
            push(ResolutionFindingKind::DeviceNavigationMismatch);
        }
        if (allocated.sidebar_width - performance.sidebar_width).abs() > 0.01 {
            push(ResolutionFindingKind::SidebarWidthMismatch);
        }
        if (allocated.stats_width - performance.stats_width).abs() > 0.01 {
            push(ResolutionFindingKind::StatsWidthMismatch);
        }
        if (allocated.main_width - performance.main_width).abs() > 0.01 {
            push(ResolutionFindingKind::MainWidthMismatch);
        }
    }

    let measured = resolve_size(
        frontend,
        REPORTED_CONTROL_ROLE,
        ControlState::Normal,
        UiSizePoint {
            control_height: resolved.control_height,
            body_font_size: resolved.body_font_size,
            renderer_scale: resolved.renderer_scale,
        },
        DensityPoint {
            row_padding_y: resolved.row_padding_y,
            line_height: resolved.line_height,
        },
    );
    if (measured.extent - resolved.control_extent).abs() > 0.01 {
        push(ResolutionFindingKind::ControlExtentMismatch);
    }
    if let Some(target) = measured.hit_target {
        if (target - resolved.hit_target.unwrap_or_default()).abs() > 0.01 {
            push(ResolutionFindingKind::HitTargetMismatch);
        }
    }
    findings
}
