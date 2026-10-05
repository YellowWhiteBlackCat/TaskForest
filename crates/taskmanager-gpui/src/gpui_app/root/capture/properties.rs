//! Positive frozen-target data required before certifying a presented properties frame.
use super::super::{ProcessDetailsSection, RootView, TopPage};
use super::CaptureScenario;
use crate::gpui_app::process_insights::ProcessInsightsRenderState;
use taskmanager_shell::fixture::process_insights::process_insights_capture_data_ready;
impl RootView {
    pub(crate) fn process_properties_capture_ready(&mut self) -> bool {
        let Some(scenario) = self
            .capture_evidence
            .scenario
            .filter(|scenario| scenario.is_process_properties())
        else {
            return false;
        };
        let Some(identity) = self.capture_evidence.scenario_process_identity else {
            return false;
        };
        if self.page != TopPage::Apps || self.process_properties_identity() != Some(identity) {
            return false;
        }
        let Some((item, histories)) = self.process_details_target(identity) else {
            return false;
        };
        if item.current_start_token() != Some(identity.start_token()) {
            return false;
        }
        match scenario {
            CaptureScenario::ProcessPropertiesPerformance => {
                self.details_section == ProcessDetailsSection::Performance
                    && item.current_cpu_percentage().is_some()
                    && item.current_memory_bytes().is_some()
                    && item.current_disk_read_bytes_per_sec().is_some()
                    && item.current_disk_write_bytes_per_sec().is_some()
                    && [
                        &histories.cpu,
                        &histories.memory,
                        &histories.disk_read,
                        &histories.disk_write,
                    ]
                    .into_iter()
                    .all(|samples| samples.iter().filter(|value| value.is_finite()).count() >= 2)
            }
            CaptureScenario::ProcessMemoryPssSwap => {
                self.details_section == ProcessDetailsSection::Overview
                    && item.current_memory_pss_bytes().is_some()
                    && item.current_memory_uss_bytes().is_some()
                    && item.current_swap_bytes().is_some()
            }
            _ => {
                let expected_facet = self.capture_evidence.properties_insight_facet();
                self.details_facet == expected_facet
                    && self.details_section == ProcessDetailsSection::Insights
                    && self.process_insights.is_ready_for(identity)
                    && matches!(self.process_insights.render_state(), ProcessInsightsRenderState::Projection(projection) if projection.raw_identity().is_some_and(|raw| raw.pid == identity.pid() && raw.start_token == identity.start_token()) && process_insights_capture_data_ready(projection, scenario.token()))
            }
        }
    }
}
