//! GPUI graph allocation adapter over the shared application timeline projection.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use taskmanager_application::system_timeline::{
    SystemHistoryWindow, TimelineMetric, TimelineReadout, TimelineSelection, TimelineSeries,
    TimelineState,
};
use taskmanager_shell::system_timeline::project_timeline;
use taskmanager_telemetry_store::CorrelatedSystemTelemetryHistory;

#[derive(Clone, Debug)]
pub struct GraphTimelineSeries {
    projection: TimelineSeries,
    samples: [Rc<[f32]>; 4],
    pub covered_ms: u64,
}
impl GraphTimelineSeries {
    pub fn samples(&self, metric: TimelineMetric) -> Rc<[f32]> {
        Rc::clone(
            &self.samples[match metric {
                TimelineMetric::Cpu => 0,
                TimelineMetric::Memory => 1,
                TimelineMetric::Disk => 2,
                TimelineMetric::Network => 3,
            }],
        )
    }
    pub fn readout(&self, selection: TimelineSelection) -> Option<TimelineReadout> {
        self.projection.readout(selection)
    }
}
#[derive(Clone, Debug, Default)]
pub struct TimelineGraphCache {
    projection: TimelineState,
    cache: RefCell<Option<GraphTimelineSeries>>,
}
impl TimelineGraphCache {
    pub fn series(
        &self,
        history: &CorrelatedSystemTelemetryHistory,
        window: SystemHistoryWindow,
    ) -> GraphTimelineSeries {
        let projection = project_timeline(&self.projection, history, window);
        if let Some(previous) = self.cache.borrow().as_ref()
            && previous.projection.anchor_ms == projection.anchor_ms
            && previous.projection.window_ms == projection.window_ms
            && Arc::ptr_eq(&previous.projection.cpu_percent, &projection.cpu_percent)
            && Arc::ptr_eq(
                &previous.projection.memory_percent,
                &projection.memory_percent,
            )
            && Arc::ptr_eq(
                &previous.projection.disk_mib_per_sec,
                &projection.disk_mib_per_sec,
            )
            && Arc::ptr_eq(
                &previous.projection.network_mib_per_sec,
                &projection.network_mib_per_sec,
            )
        {
            return previous.clone();
        }
        let graph = GraphTimelineSeries {
            samples: [
                Rc::from(projection.cpu_percent.as_ref()),
                Rc::from(projection.memory_percent.as_ref()),
                Rc::from(projection.disk_mib_per_sec.as_ref()),
                Rc::from(projection.network_mib_per_sec.as_ref()),
            ],
            covered_ms: projection.covered_ms,
            projection,
        };
        *self.cache.borrow_mut() = Some(graph.clone());
        graph
    }
}
