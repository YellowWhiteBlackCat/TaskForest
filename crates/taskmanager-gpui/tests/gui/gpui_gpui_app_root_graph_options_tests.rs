use super::normalize_graph_data_points;

#[test]
fn graph_data_points_migration_normalizes_out_of_range_config_values() {
    assert_eq!(normalize_graph_data_points(0), 10);
    assert_eq!(normalize_graph_data_points(60), 60);
    assert_eq!(normalize_graph_data_points(u32::MAX), 600);
}
