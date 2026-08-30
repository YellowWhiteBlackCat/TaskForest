//! Headless sparkline geometry and canonical-tree row integration.

use super::super::process_projection::{ProcessProjection, ProjectedRow};
use super::super::process_sparkline::{ProcessCpuSparkline, process_sparkline_points};
use iced::Size;
use std::collections::HashSet;
use std::rc::Rc;
use taskmanager_core::core::process::ProcessLiveKey;
use taskmanager_shell::{SortCol, SortDir};

const SPARK_SIZE: Size = Size::new(48.0, 16.0);

#[test]
fn empty_history_is_a_two_point_midpoint_baseline() {
    let points = process_sparkline_points(&[], SPARK_SIZE);
    assert_eq!(points.len(), 2);
    assert!(
        points
            .iter()
            .all(|point| point.y == SPARK_SIZE.height * 0.5)
    );
}

#[test]
fn samples_auto_range_and_spread_across_the_available_width() {
    let points = process_sparkline_points(&[10.0, 50.0, 90.0], SPARK_SIZE);
    assert_eq!(points.len(), 3);
    assert_eq!([points[0].x, points[1].x, points[2].x], [0.0, 24.0, 48.0]);
    assert!(points[0].y > points[1].y && points[1].y > points[2].y);
}

#[test]
fn fingerprint_changes_only_when_process_history_identity_changes() {
    let color = iced::Color::WHITE;
    let samples: Rc<[f32]> = Rc::from([1.0, 2.0, 3.0].as_slice());
    let identity = ProcessLiveKey::from_parts(11, 111).expect("fixture identity");
    let replacement = ProcessLiveKey::from_parts(12, 121).expect("fixture identity");
    let base = ProcessCpuSparkline::new(Rc::clone(&samples), color, identity);
    assert_eq!(
        base.fingerprint(),
        ProcessCpuSparkline::new(Rc::clone(&samples), iced::Color::BLACK, identity).fingerprint()
    );
    assert_ne!(
        base.fingerprint(),
        ProcessCpuSparkline::new(Rc::from([1.0, 2.0, 4.0].as_slice()), color, identity,)
            .fingerprint()
    );
    assert_ne!(
        base.fingerprint(),
        ProcessCpuSparkline::new(samples, color, replacement).fingerprint()
    );
}

#[test]
fn canonical_process_nodes_carry_hierarchy_depth() {
    let items = [
        taskmanager_test_support::ProcessItemFixtureBuilder::new()
            .pid(1)
            .name("root".into())
            .build(),
        taskmanager_test_support::ProcessItemFixtureBuilder::new()
            .pid(2)
            .name("child".into())
            .parent_pid(Some(1))
            .build(),
    ];
    let refs: Vec<_> = items.iter().collect();
    let projection = ProcessProjection::project_with_local_time(
        &refs,
        (SortCol::Cpu, SortDir::Desc),
        &HashSet::from(["category:uncategorized".to_string()]),
        &HashSet::new(),
        &taskmanager_core::core::time::LocalTimeRulesObservation::unsupported(0),
        0,
    );
    let depths: Vec<_> = projection
        .rows()
        .iter()
        .filter_map(|row| match row {
            ProjectedRow::Tree { depth, .. } => Some(*depth),
            ProjectedRow::GroupHeader { .. } => None,
        })
        .collect();
    assert_eq!(depths, [1, 2]);
}
