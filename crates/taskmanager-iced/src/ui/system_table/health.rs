//! Health cards consume cached canonical filesystem, sensor and SMART groups.
use crate::app::{FocusTarget, Message};
use crate::ui::components::{key_value_rows, titled_card};
use crate::ui::system_dashboard::SystemDashboardMessage;
use crate::{IcedApp, focus};
use iced::widget::{column, row, scrollable};
use iced::{Element, Length};
use taskmanager_application::i18n::t;
use taskmanager_core::core::smart::SmartSelfTestKind;
use taskmanager_shell::presentation::health_review::{
    HealthReviewSection, sensor_groups, storage_groups,
};
use taskmanager_shell::presentation::smart::self_test_intent;
pub(super) fn render(app: &IcedApp) -> Element<'_, Message, iced::Theme, iced::Renderer> {
    let projection = app.shell.projection();
    let disks = projection
        .snapshot
        .as_ref()
        .map(|snapshot| snapshot.disks.as_slice())
        .unwrap_or_default();
    let (reports, _) = projection.smart_projection();
    let storage = storage_groups(
        projection
            .storage_health_projection()
            .map(|(snapshot, _)| snapshot),
        disks,
        reports.observations(),
    );
    let sensors = sensor_groups(projection.sensors.as_ref());
    let groups = storage
        .into_iter()
        .filter(|_| app.system_health_section != HealthReviewSection::Sensors)
        .chain(
            sensors
                .into_iter()
                .filter(|_| app.system_health_section != HealthReviewSection::Storage),
        );
    let cards = groups.map(|group| {
        let rows = group
            .rows
            .into_iter()
            .map(|row| (row.label, row.value))
            .collect();
        titled_card(app.theme(), group.title, key_value_rows(rows))
    });
    let mut body = column(cards).spacing(8).width(Length::Fill);
    for (index, disk) in disks.iter().enumerate() {
        body = body.push(
            row![
                focus::button(
                    app.theme(),
                    FocusTarget::SmartSelfTestShort { index },
                    t("health.short_test"),
                    Message::RequestSmartSelfTest(self_test_intent(disk, SmartSelfTestKind::Short)),
                    false
                ),
                focus::button(
                    app.theme(),
                    FocusTarget::SmartSelfTestExtended { index },
                    t("health.extended_test"),
                    Message::RequestSmartSelfTest(self_test_intent(
                        disk,
                        SmartSelfTestKind::Extended
                    )),
                    false
                ),
            ]
            .spacing(8)
            .wrap(),
        );
    }
    let tabs = row(HealthReviewSection::ALL.into_iter().map(|section| {
        let label = match section {
            HealthReviewSection::All => t("health.system_health_alerts"),
            HealthReviewSection::Storage => t("health.storage"),
            HealthReviewSection::Sensors => t("health.sensors"),
        };
        focus::choice_pill(
            app.theme(),
            FocusTarget::SystemHealthSection(section),
            label.into(),
            app.system_health_section == section,
            Message::SystemDashboard(SystemDashboardMessage::SelectHealthSection(section)),
        )
    }))
    .spacing(8)
    .wrap();
    column![
        tabs,
        scrollable(body)
            .id("system-health-scroll")
            .height(Length::Fill)
    ]
    .height(Length::Fill)
    .spacing(8)
    .into()
}
