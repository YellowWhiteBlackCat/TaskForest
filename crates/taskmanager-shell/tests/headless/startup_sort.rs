//! test-intent: behavior
//!
//! The Startup table's visual order and the provider order are two
//! different things. A control that acts on the SELECTED row must resolve
//! that row through the same canonical sort the table renders, or a sorted
//! table acts on a different unit than the one the user sees.

use crate::app::PlatformEffect;

/// The selected Startup row is a VISUAL index: a sorted table must act on
/// the entry the user actually sees, not on the provider-order entry that
/// happens to sit at the same position.
#[test]
fn a_sorted_startup_table_acts_on_the_visible_entry() {
    use crate::app::sorting::{InfoSortCol, InfoTable};

    let mut app = crate::demo_app();
    // Sort the Startup table by name, so the visual order differs from the
    // provider order.
    app.set_info_sort(InfoTable::Startup, InfoSortCol::Name);
    app.selected = 0;

    let visible = app
        .sorted_startup_entry_at(0)
        .expect("the sorted table has a first row")
        .id
        .clone();
    let provider_first = app
        .data
        .startup_entries
        .as_deref()
        .and_then(|entries| entries.first())
        .expect("the provider list has a first entry")
        .id
        .clone();

    let _ = app.request_startup_control(false);
    assert!(
        app.pending_startup().is_some(),
        "the request arms the shared confirmation gate"
    );
    let effect = app
        .confirm_startup_control()
        .expect("confirm emits the gated request");
    let PlatformEffect::StartupControl(request) = effect else {
        panic!("startup action must cross the typed platform-effect boundary");
    };

    assert_eq!(
        request.entry.id, visible,
        "the request must target the row the user sees"
    );
    assert_ne!(
        visible, provider_first,
        "the sorted row must differ from the provider's first entry, or this test proves nothing"
    );
}
