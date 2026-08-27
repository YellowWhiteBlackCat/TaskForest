//! Window geometry and independent virtual-scroll lifetimes for Iced.

use super::VirtualScrollState;

#[derive(Clone, Copy)]
pub(super) enum ViewportRegion {
    Applications,
    AppHistory,
    Services,
    Startup,
    Users,
    PerformanceRail,
}

pub(super) struct IcedViewportState {
    size: iced::Size,
    applications: VirtualScrollState,
    app_history: VirtualScrollState,
    services: VirtualScrollState,
    startup: VirtualScrollState,
    users: VirtualScrollState,
    performance_rail: VirtualScrollState,
}

impl IcedViewportState {
    pub(super) fn new(size: iced::Size) -> Self {
        Self {
            size,
            applications: VirtualScrollState::new(),
            app_history: VirtualScrollState::new(),
            services: VirtualScrollState::new(),
            startup: VirtualScrollState::new(),
            users: VirtualScrollState::new(),
            performance_rail: VirtualScrollState::new(),
        }
    }

    pub(super) const fn size(&self) -> iced::Size {
        self.size
    }

    pub(super) fn resize(&mut self, size: iced::Size) -> bool {
        if !size.width.is_finite()
            || !size.height.is_finite()
            || size.width <= 0.0
            || size.height <= 0.0
        {
            return false;
        }
        self.size = size;
        for region in [
            ViewportRegion::Applications,
            ViewportRegion::AppHistory,
            ViewportRegion::Services,
            ViewportRegion::Startup,
            ViewportRegion::Users,
            ViewportRegion::PerformanceRail,
        ] {
            self.scroll_mut(region).invalidate_viewport();
        }
        true
    }

    pub(super) fn update(
        &mut self,
        region: ViewportRegion,
        viewport: iced::widget::scrollable::Viewport,
    ) {
        self.scroll_mut(region).update_from_viewport(viewport);
    }

    pub(super) const fn scroll(&self, region: ViewportRegion) -> &VirtualScrollState {
        match region {
            ViewportRegion::Applications => &self.applications,
            ViewportRegion::AppHistory => &self.app_history,
            ViewportRegion::Services => &self.services,
            ViewportRegion::Startup => &self.startup,
            ViewportRegion::Users => &self.users,
            ViewportRegion::PerformanceRail => &self.performance_rail,
        }
    }

    pub(super) fn scroll_mut(&mut self, region: ViewportRegion) -> &mut VirtualScrollState {
        match region {
            ViewportRegion::Applications => &mut self.applications,
            ViewportRegion::AppHistory => &mut self.app_history,
            ViewportRegion::Services => &mut self.services,
            ViewportRegion::Startup => &mut self.startup,
            ViewportRegion::Users => &mut self.users,
            ViewportRegion::PerformanceRail => &mut self.performance_rail,
        }
    }
}
