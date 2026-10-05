//! Four identity-bound process curves and current scalar captions.
use crate::app::Message;
use crate::ui::device_chart;
use iced::{Element, widget::column};
use taskmanager_core::core::process::ProcessLiveKey;

pub(super) fn performance_tab<'a>(
    app: &'a crate::IcedApp,
    identity: ProcessLiveKey,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let theme_snapshot = app.theme();
    let smooth = true;
    let history = app.process_perf_series();
    let cpu = history.as_ref().map_or_else(
        || std::rc::Rc::from([].as_slice()),
        |snapshot| std::rc::Rc::clone(&snapshot.cpu),
    );
    let memory = history.as_ref().map_or_else(
        || std::rc::Rc::from([].as_slice()),
        |snapshot| std::rc::Rc::clone(&snapshot.memory),
    );
    let read = history.as_ref().map_or_else(
        || std::rc::Rc::from([].as_slice()),
        |snapshot| std::rc::Rc::clone(&snapshot.disk_read),
    );
    let write = history.as_ref().map_or_else(
        || std::rc::Rc::from([].as_slice()),
        |snapshot| std::rc::Rc::clone(&snapshot.disk_write),
    );

    let [cpu_caption, memory_caption, read_caption, write_caption] =
        app.process_performance_captions(identity, [&cpu, &memory, &read, &write]);

    column![
        device_chart::device_mini_graph(
            cpu,
            device_chart::DeviceMetricScale::Percent,
            crate::theme_binding::color(theme_snapshot.cpu),
            cpu_caption,
            theme_snapshot,
            device_chart::GraphPrefs {
                smooth,
                max_override: None,
                hover: false,
            },
        ),
        device_chart::device_mini_graph(
            memory,
            device_chart::DeviceMetricScale::Bytes { use_base2: true },
            crate::theme_binding::color(theme_snapshot.memory),
            memory_caption,
            theme_snapshot,
            device_chart::GraphPrefs {
                smooth,
                max_override: None,
                hover: false,
            },
        ),
        device_chart::device_mini_graph(
            read,
            device_chart::DeviceMetricScale::BytesPerSecond {
                use_bytes: true,
                use_base2: true
            },
            crate::theme_binding::color(theme_snapshot.disk),
            read_caption,
            theme_snapshot,
            device_chart::GraphPrefs {
                smooth,
                max_override: None,
                hover: false,
            },
        ),
        device_chart::device_mini_graph(
            write,
            device_chart::DeviceMetricScale::BytesPerSecond {
                use_bytes: true,
                use_base2: true
            },
            crate::theme_binding::color(theme_snapshot.disk),
            write_caption,
            theme_snapshot,
            device_chart::GraphPrefs {
                smooth,
                max_override: None,
                hover: false,
            },
        ),
    ]
    .spacing(8)
    .into()
}
