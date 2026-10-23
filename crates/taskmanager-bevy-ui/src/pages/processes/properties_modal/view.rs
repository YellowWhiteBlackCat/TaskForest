//! Declarative properties chrome around one bounded complete inspection body.

use super::{
    ProcessPropertiesAction, ProcessPropertiesBody, ProcessPropertiesDismissButton,
    ProcessPropertiesFacet, ProcessPropertiesFooter, ProcessPropertiesOverlay,
    ProcessPropertiesPanel, ProcessPropertiesScrollHint, ProcessPropertiesSection,
    ProcessPropertiesTab, ProcessPropertiesView, on_authorize, on_facet,
    on_properties_dismiss_activated, on_refresh,
};
use crate::palette::{UiPalette, space_4, space_8, space_12, space_16};
use crate::widgets::chart::{CurveMeasurement, CurvePaintAccess, paint_curve_at_size};
use crate::widgets::controls::{ControlTone, ControlVisual, detail_row_scene};
use crate::window::{Role, TextRole};
use bevy::color::Color;
use bevy::ecs::system::Query;
use bevy::ecs::{component::Component, entity::Entity, hierarchy::Children};
use bevy::scene::{Scene, bsn, on};
use bevy::text::{LineBreak, TextLayout};
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, ComputedNode, FlexDirection, FlexWrap,
    JustifyContent, Node, Overflow, PositionType, ScrollPosition, UiRect, percent, px, rem,
};
use bevy::ui::widget::Text;
use bevy::ui_widgets::{Button, ScrollArea, Tab, TabList};
use std::sync::Arc;
use taskmanager_application::{
    ProcessInsightFacet, i18n::t, process_details_vm::ProcessDetailsField,
};
use taskmanager_shell::presentation::process_insight_facet_label;

use super::backdrop::backdrop_scene;
use super::gpu::gpu_panel_scene;
use super::memory::memory_panel_scene;
use super::quota::quota_panel_scene;
use super::tabs::{IndicatorPlacement, TAB_INDICATOR_HEIGHT_PX, TabIndicator};

/// The properties panel's maximum width, in rem: the widest the inspection
/// body may grow before it starts scrolling its own columns. The bound rides
/// the rem axis with the surface around it.
const PROPERTIES_PANEL_MAX_WIDTH_REM: f32 = 47.5;

/// The properties title's maximum height, in rem: the two-line budget the
/// panel title may take before it is clipped.
const PROPERTIES_TITLE_MAX_HEIGHT_REM: f32 = 3.0;

/// The scroll hint's minimum height, in rem: one caption line, so the hint
/// never collapses into the body it introduces.
const PROPERTIES_SCROLL_HINT_MIN_HEIGHT_REM: f32 = 0.875;

/// The properties curve's slot height, in rem: the budget one inspected
/// metric's curve may take inside the panel.
const PROPERTIES_CURVE_SLOT_HEIGHT_REM: f32 = 3.5;

#[derive(Component, Clone)]
pub(crate) struct ProcessPropertiesCurve {
    pub(crate) metric: ProcessDetailsField,
    pub(crate) samples: Arc<[f32]>,
    ceiling: f32,
    color: Color,
}
impl Default for ProcessPropertiesCurve {
    fn default() -> Self {
        Self {
            metric: ProcessDetailsField::Cpu,
            samples: Arc::from([]),
            ceiling: 100.0,
            color: Color::WHITE,
        }
    }
}
fn text_scene(value: String) -> Box<dyn Scene> {
    Box::new(
        bsn! { Text(value) TextRole(Role::Body) TextLayout { linebreak: LineBreak::WordBoundary }
        Node { width: percent(100), min_width: px(0.0) } },
    )
}
pub(super) fn properties_modal_scene(
    view: &ProcessPropertiesView,
    palette: &UiPalette,
    scroll: ScrollPosition,
    indicator: IndicatorPlacement,
) -> impl Scene + use<> {
    let title = format!("{} · {}", t("dialog.properties"), view.target.name);
    let subtitle = format!("PID: {}", view.target.pid);
    let tabs: Vec<Box<dyn Scene>> = ProcessPropertiesSection::ALL.into_iter().map(|section| {
        Box::new(bsn! { Node { min_height: rem(palette.control_height_rem), padding: UiRect::horizontal(px(space_8())) }
            Tab ProcessPropertiesTab(section) ControlVisual(ControlTone::Surface, {section == view.section})
            Children [ Text({section.label().to_owned()}) TextRole(Role::Body) ] }) as Box<dyn Scene>
    }).collect();
    let facets: Vec<Box<dyn Scene>> = if view.section == ProcessPropertiesSection::Insights {
        ProcessInsightFacet::ALL.into_iter().map(|facet| Box::new(bsn! { Node { min_height: rem(palette.control_height_rem), padding: UiRect::horizontal(px(space_8())) }
            Button ProcessPropertiesFacet(facet) ControlVisual(ControlTone::Surface, {facet == view.facet}) on(on_facet)
            Children [ Text({process_insight_facet_label(facet).to_owned()}) TextRole(Role::Caption) ] }) as Box<dyn Scene>).collect()
    } else {
        Vec::new()
    };
    // The resource quota surface owns the memory, CPU and process-count facts
    // of the resources facet: it renders the same observed values plus the
    // gauge and the limit alert, so the shared projection's plain rows for
    // those three facts are not repeated underneath it.
    let covered_labels = if view.quotas.is_empty() {
        Vec::new()
    } else {
        vec![
            t("common.memory").to_owned(),
            t("proc_insights.cpu_quota").to_owned(),
            t("proc_insights.pids").to_owned(),
        ]
    };
    let mut content: Vec<Box<dyn Scene>> = view
        .rows
        .iter()
        .filter(|(label, _)| !covered_labels.iter().any(|covered| covered == label))
        .map(|(label, value)| {
            Box::new(detail_row_scene(
                label.clone(),
                text_scene(value.clone()),
                palette,
            )) as Box<dyn Scene>
        })
        .collect();
    content.insert(
        0,
        Box::new(
            bsn! { Text({t("proc_insights.scroll_hint").to_owned()}) TextRole(Role::Caption)
            ProcessPropertiesScrollHint TextLayout { linebreak: LineBreak::WordBoundary }
            Node { width: percent(100), min_height: rem(PROPERTIES_SCROLL_HINT_MIN_HEIGHT_REM), flex_shrink: 0.0 } },
        ),
    );
    if let Some(memory) = view.memory {
        content.insert(0, Box::new(memory_panel_scene(memory, palette)));
    }
    if !view.gpu_bars.is_empty() {
        content.insert(0, Box::new(gpu_panel_scene(&view.gpu_bars, palette)));
    }
    if !view.quotas.is_empty() {
        content.insert(0, Box::new(quota_panel_scene(&view.quotas, palette)));
    }
    if !view.curves.is_empty() {
        content.push(Box::new(
            bsn! { Text({t("prop.recent_samples").to_owned()}) TextRole(Role::Caption) },
        ));
    }
    for curve in &view.curves {
        let color = match curve.metric {
            ProcessDetailsField::Cpu => palette.cpu,
            ProcessDetailsField::Memory => palette.memory,
            _ => palette.disk,
        };
        let caption = format!(
            "{} {} · {} {}",
            t("dashboard.latest"),
            curve.current,
            t("dashboard.peak"),
            curve.peak
        );
        let mut body: Vec<Box<dyn Scene>> = vec![
            Box::new(bsn! { Text({curve.label.clone()}) TextRole(Role::Body) }),
            Box::new(
                bsn! { Text(caption) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::WordBoundary } Node { width: percent(100), min_width: px(0.0) } },
            ),
        ];
        if curve
            .samples
            .iter()
            .filter(|value| value.is_finite())
            .count()
            >= 2
        {
            body.push(Box::new(bsn! { Node { width: percent(100), height: rem(PROPERTIES_CURVE_SLOT_HEIGHT_REM), flex_shrink: 0.0, overflow: Overflow::clip() }
                ProcessPropertiesCurve { metric: {curve.metric}, samples: {curve.samples.clone()}, ceiling: {curve.ceiling}, color }
                CurveMeasurement::default() }));
        } else {
            body.push(text_scene(t("proc_insights.collecting").to_owned()));
        }
        content.push(Box::new(bsn! { Node { width: percent(100), min_width: px(0.0), flex_direction: FlexDirection::Column,
            row_gap: px(space_4()), padding: UiRect::all(px(space_8())), flex_shrink: 0.0 }
            BackgroundColor({palette.content_bg}) Children [{body}] }));
    }
    let mut actions: Vec<Box<dyn Scene>> = Vec::new();
    let rail = crate::widgets::scrollbar::scrollbar_rail_scene(palette);
    if view.section == ProcessPropertiesSection::Insights && view.live {
        actions.push(Box::new(bsn! { Node { min_height: rem(palette.control_height_rem), padding: UiRect::horizontal(px(space_8())) }
            Button ProcessPropertiesAction::Refresh ControlVisual(ControlTone::Surface, false) on(on_refresh)
            Children [ Text({t("common.refresh").to_owned()}) TextRole(Role::Body) ] }));
    }
    if view.authorize_network {
        actions.push(Box::new(bsn! { Node { min_height: rem(palette.control_height_rem), padding: UiRect::horizontal(px(space_8())) }
            Button ProcessPropertiesAction::AuthorizeNetwork ControlVisual(ControlTone::Surface, true) on(on_authorize)
            Children [ Text({t("proc_insights.enable_network_capture").to_owned()}) TextRole(Role::Body)
                TextLayout { linebreak: LineBreak::WordBoundary } Node { min_width: px(0.0) } ] }));
    }
    actions.push(Box::new(bsn! { Node { min_height: rem(palette.control_height_rem), padding: UiRect::horizontal(px(space_8())) }
        Button ProcessPropertiesDismissButton ControlVisual(ControlTone::Surface, true) on(on_properties_dismiss_activated)
        Children [ Text({t("common.close").to_owned()}) TextRole(Role::Body) ] }));
    let lane = crate::widgets::scrollbar::rail_lane(UiRect::bottom(px(space_8())));
    bsn! {
        Node { width: percent(100), height: percent(100), position_type: PositionType::Absolute,
            justify_content: JustifyContent::Center, align_items: AlignItems::Center }
        ProcessPropertiesOverlay
        Children [
            @{ backdrop_scene(palette) } --
            Node { width: percent(92), max_width: rem(PROPERTIES_PANEL_MAX_WIDTH_REM), height: percent(92), min_height: px(0.0), min_width: px(0.0),
                flex_direction: FlexDirection::Column, row_gap: px(space_8()),
                padding: UiRect::px(space_12(), space_12(), space_12(), space_16()),
                border_radius: BorderRadius::all(px(palette.panel_radius_px)) }
            BackgroundColor({palette.panel_fill}) ProcessPropertiesPanel
            Children [
                Text(title) TextRole(Role::Heading) TextLayout { linebreak: LineBreak::WordBoundary }
                    Node { width: percent(100), min_width: px(0.0), max_height: rem(PROPERTIES_TITLE_MAX_HEIGHT_REM), flex_shrink: 0.0, overflow: Overflow::clip() } --
                Text(subtitle) TextRole(Role::Caption) Node { flex_shrink: 0.0 } --
                // The headless tab-list widget: shared tablist/tab roles,
                // roving arrow-key focus and typed ValueChange selection; the
                // presentation state stays the sole section authority. The
                // sliding indicator is declared beside the strip and driven
                // by the measured tab geometry.
                Node { width: percent(100), flex_direction: FlexDirection::Column, row_gap: px(space_4()),
                    flex_shrink: 0.0, position_type: PositionType::Relative }
                Children [
                    Node { width: percent(100), flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap,
                        row_gap: px(space_4()), column_gap: px(space_4()), flex_shrink: 0.0 }
                    TabList on(super::sync_selected_tab) Children [{tabs}] --
                    Node { position_type: PositionType::Absolute, left: px(indicator.x), bottom: px(0.0),
                        width: px(indicator.width), height: px(TAB_INDICATOR_HEIGHT_PX) }
                    TabIndicator BackgroundColor({palette.accent})
                ] --
                Node { width: percent(100), flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap,
                    row_gap: px(space_4()), column_gap: px(space_4()), flex_shrink: 0.0 } Children [{facets}] --
                // The viewport wrapper owns the scroll area AND the floating
                // rail: the rail is a sibling of the scroll area, so it never
                // scrolls with the content and never takes layout space away
                // from it.
                Node { width: percent(100), min_width: px(0.0), flex_grow: 1.0, flex_basis: px(0.0), min_height: px(0.0),
                    flex_direction: FlexDirection::Column }
                Children [
                    Node { width: percent(100), min_width: px(0.0), flex_grow: 1.0, flex_basis: px(0.0), min_height: px(0.0),
                        flex_direction: FlexDirection::Column, row_gap: px(space_4()), overflow: Overflow::scroll_y(),
                        padding: { lane } }
                    ScrollArea ScrollPosition({scroll.0}) ProcessPropertiesBody Children [{content}] --
                    @{ rail }
                ] --
                Node { width: percent(100), flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap,
                    row_gap: px(space_4()), column_gap: px(space_8()), flex_shrink: 0.0 }
                ProcessPropertiesFooter Children [{actions}]
            ]
        ]
    }
}
pub(super) fn paint_curves(
    mut curves: Query<(
        Entity,
        &ComputedNode,
        &ProcessPropertiesCurve,
        &mut CurveMeasurement,
    )>,
    mut access: CurvePaintAccess,
) {
    for (entity, node, curve, mut last) in &mut curves {
        let size = node.size() * node.inverse_scale_factor();
        if size.x > 0.0
            && size.y > 0.0
            && last.0 != Some((size.x, size.y))
            && paint_curve_at_size(
                &mut access,
                entity,
                size,
                &curve.samples,
                curve.ceiling,
                curve.color,
            )
        {
            last.0 = Some((size.x, size.y));
        }
    }
}
