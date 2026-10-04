//! Complete dashboard groups inside an allocated Iced body slot.

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DashboardBudget {
    pub(crate) compact_summary: bool,
    pub(crate) columns: usize,
    pub(crate) count: usize,
    pub(crate) chart_height: f32,
}
impl DashboardBudget {
    pub(crate) fn resolve(width: f32, height: f32) -> Self {
        let compact_summary = height < 440.0 || width < 600.0;
        let columns = if width >= 600.0 { 2 } else { 1 };
        let fixed = if compact_summary { 68.0 } else { 148.0 };
        let available = (height - fixed - 8.0).max(0.0);
        if available < 132.0 {
            return Self {
                compact_summary,
                columns,
                count: 0,
                chart_height: 0.0,
            };
        }
        let rows = (available / 132.0).floor().clamp(1.0, 4.0) as usize;
        let count = (rows * columns).min(4);
        let rows = count.div_ceil(columns);
        let each = (available - (rows.saturating_sub(1) as f32 * 8.0)) / rows as f32;
        Self {
            compact_summary,
            columns,
            count,
            chart_height: (each - 100.0).clamp(32.0, 100.0),
        }
    }
}
