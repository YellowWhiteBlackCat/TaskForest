// test-intent: behavior
//! The device-pixel scale the chart aligns against (ICED-032 / ICED-054).
//!
//! iced multiplies the window's DPI scale factor with the application's
//! render scale at the toolkit boundary. The chart's sub-pixel alignment is
//! exact only against that PRODUCT: aligning against the app render scale
//! alone is off by a factor on every HiDPI display.

use crate::app::Message;

#[test]
fn the_device_pixel_scale_is_the_product_of_the_app_and_window_scales() {
    let mut app = crate::IcedApp::demo();
    let app_scale = app.ui_size().renderer_scale();

    // No window event yet: the window scale is the neutral 1.0, so the
    // product is the app's own render scale.
    assert_eq!(app.device_pixel_scale(), app_scale);

    let _ = app.update(Message::WindowScaleFactorChanged(2.0));

    assert_eq!(
        app.device_pixel_scale(),
        app_scale * 2.0,
        "the window's DPI scale multiplies the app's render scale"
    );
}

#[test]
fn a_window_scale_event_reaches_the_app_state() {
    let mut app = crate::IcedApp::demo();

    let _ = app.update(Message::WindowScaleFactorChanged(1.5));

    assert_eq!(app.window_scale_factor, 1.5);
}

#[test]
fn the_window_scale_event_is_mapped_from_the_window_event_stream() {
    // The subscription maps iced's window event into the typed message.
    let mapped =
        crate::app::subscription::message_for_window_event(iced::window::Event::Rescaled(1.25));
    assert!(
        matches!(mapped, Some(Message::WindowScaleFactorChanged(1.25))),
        "the window's scale-factor event becomes the typed message: {mapped:?}"
    );
}
