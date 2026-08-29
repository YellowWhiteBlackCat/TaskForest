use std::rc::Rc;
use std::sync::Arc;

use gpui::AppContext;
use taskmanager_application::{
    ApplicationHistoryMetricSeries, ApplicationHistoryProjection, ApplicationHistoryRow,
    ApplicationHistoryStatus,
};
use taskmanager_core::core::history::{ApplicationHistoryIdentity, HistoryWindow};

use super::*;

fn metric(samples: &[f32], times: &[u64], peak: f64) -> ApplicationHistoryMetricSeries {
    ApplicationHistoryMetricSeries {
        samples: Arc::from(samples),
        sample_times_ms: Arc::from(times),
        peak_value: Some(peak),
        peak_measured_at_ms: times.last().copied(),
        observed: samples.iter().filter(|sample| sample.is_finite()).count(),
        gaps: samples.iter().filter(|sample| !sample.is_finite()).count(),
        clock_jumps: 0,
    }
}

fn durable_row(name: &str, verified: bool) -> ApplicationHistoryRow {
    let identity = if verified {
        ApplicationHistoryIdentity::verified_launcher(name)
    } else {
        ApplicationHistoryIdentity::unverified_process_name(name)
    }
    .expect("non-empty fixture identity");
    ApplicationHistoryRow {
        identity,
        cpu_usage: Some(metric(&[2.0, 3.0, 4.0], &[1_000, 2_000, 10_000], 4.0)),
        memory: Some(metric(&[128.0, 256.0], &[1_000, 2_000], 256.0)),
        process_count: Some(metric(&[1.0, 3.0], &[1_000, 2_000], 3.0)),
    }
}

fn projection(rows: Arc<[ApplicationHistoryRow]>) -> ApplicationHistoryProjection {
    ApplicationHistoryProjection {
        status: if rows.is_empty() {
            ApplicationHistoryStatus::Collecting
        } else {
            ApplicationHistoryStatus::Ready
        },
        selected_window: HistoryWindow::OneHour,
        rows_window: (!rows.is_empty()).then_some(HistoryWindow::OneHour),
        rows,
        source_request: None,
        refreshing: false,
        failure: None,
        unavailable_reason: None,
        loaded_at_ms: Some(10_000),
    }
}

#[test]
fn renderer_projection_preserves_durable_identity_peaks_and_downtime_gap() {
    let rows = [
        durable_row("org.example.Browser", true),
        durable_row("render-worker", false),
    ];

    let projected = projected_app_history_rows(&rows);

    assert_eq!(projected.len(), 2);
    assert_eq!(projected[0].name, "org.example.Browser");
    assert!(projected[0].verified);
    assert_eq!(projected[0].peak_cpu_usage, Some(4.0));
    assert_eq!(projected[0].peak_memory_bytes, Some(256.0));
    assert_eq!(projected[0].peak_process_count, Some(3.0));
    assert_eq!(projected[0].cpu_samples.len(), 4);
    assert_eq!(&projected[0].cpu_samples[..2], &[2.0, 3.0]);
    assert!(projected[0].cpu_samples[2].is_nan());
    assert_eq!(projected[0].cpu_samples[3], 4.0);
    assert!(!projected[1].verified);
}

#[gpui::test]
fn renderer_projection_cache_is_keyed_by_durable_rows_publication(cx: &mut gpui::TestAppContext) {
    let root =
        cx.new(|cx| crate::gpui_app::root::RootView::new(taskmanager_theme::Theme::dark(), cx));
    root.update(cx, |view, _cx| {
        let first_projection = projection(Arc::from([durable_row("org.example.Editor", true)]));
        let first = view.app_history_rows(&first_projection);
        let reused = view.app_history_rows(&first_projection);
        assert!(Rc::ptr_eq(&first, &reused));
        assert!(Rc::ptr_eq(&first[0].cpu_samples, &reused[0].cpu_samples));

        let republished = projection(Arc::from([durable_row("org.example.Editor", true)]));
        let rebuilt = view.app_history_rows(&republished);
        assert!(!Rc::ptr_eq(&first, &rebuilt));
        assert_eq!(first[0].name, rebuilt[0].name);
        assert_eq!(first[0].peak_cpu_usage, rebuilt[0].peak_cpu_usage);
        assert_eq!(first[0].cpu_samples.len(), rebuilt[0].cpu_samples.len());
        for (left, right) in first[0]
            .cpu_samples
            .iter()
            .zip(rebuilt[0].cpu_samples.iter())
        {
            assert!(left == right || (left.is_nan() && right.is_nan()));
        }
    });
}
