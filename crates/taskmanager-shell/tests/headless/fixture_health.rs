//! test-intent: behavior
//!
//! The shell fixture seam must use the same complete-batch revision gate as
//! live SMART observations, so stale test data cannot restore an old report.

use super::*;

use crate::fixture::demo_app;
use taskmanager_application::SmartStateRevision;

fn smart_batch(revision: SmartStateRevision, phase: SmartSelfTestPhase) -> SmartObservationBatch {
    let fixture = health_fixture();
    let mut report = fixture.smart_report;
    report.phase = phase;
    SmartObservationBatch {
        revision,
        subject: None,
        observations: vec![SmartSelfTestObservation {
            device_id: fixture.selected_disk.device_id.into(),
            device_generation: fixture.selected_disk.device_generation,
            device_key: fixture.selected_disk.name.into(),
            display_name: fixture.selected_disk.model,
            report,
        }],
        issues: Vec::new(),
        ended: Vec::new(),
    }
}

#[test]
fn shell_smart_fixture_uses_revision_and_target_validation() {
    let mut app = demo_app();
    let (projection, _) = app.projection().smart_projection();
    let prior_revision = projection.revision();
    let first_revision = prior_revision
        .checked_next()
        .and_then(SmartStateRevision::checked_next)
        .expect("test revisions fit");
    let stale_revision = prior_revision.checked_next().expect("test revision fits");
    let duplicate_target_revision = first_revision.checked_next().expect("test revision fits");

    seed_smart_observation_batch(
        &mut app,
        smart_batch(first_revision, SmartSelfTestPhase::Completed),
    );
    let (projection, _) = app.projection().smart_projection();
    assert_eq!(projection.revision(), first_revision);
    assert_eq!(projection.observations().len(), 1);
    assert_eq!(
        projection.observations()[0].report.phase,
        SmartSelfTestPhase::Completed
    );

    seed_smart_observation_batch(
        &mut app,
        smart_batch(stale_revision, SmartSelfTestPhase::Failed),
    );
    seed_smart_observation_batch(
        &mut app,
        smart_batch(first_revision, SmartSelfTestPhase::Failed),
    );

    let mut duplicate_target = smart_batch(duplicate_target_revision, SmartSelfTestPhase::Failed);
    let duplicate = duplicate_target.observations[0].clone();
    duplicate_target.observations.push(duplicate);
    seed_smart_observation_batch(&mut app, duplicate_target);

    let (projection, _) = app.projection().smart_projection();
    assert_eq!(projection.revision(), first_revision);
    assert_eq!(projection.observations().len(), 1);
    assert_eq!(
        projection.observations()[0].report.phase,
        SmartSelfTestPhase::Completed,
        "stale, duplicate-revision, and duplicate-target batches must not replace the accepted report"
    );
}
