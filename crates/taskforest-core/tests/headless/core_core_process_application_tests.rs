use super::super::metadata::ProcessMetadataObservation;
use super::*;

fn identity() -> ProcessApplicationIdentity {
    ProcessApplicationIdentity::new("org.example.Editor", "Editor", None).expect("identity fixture")
}

fn item_with(observation: ProcessMetadataObservation<ProcessApplicationIdentity>) -> ProcessItem {
    ProcessItem::default().with_application_identity_observation(observation)
}

/// Every availability state lands in its honest bucket. Enumerating the
/// distinct states through the constructors (not a duplicated variant
/// list) keeps this table honest as the vocabulary evolves.
#[test]
fn every_availability_state_maps_to_its_honest_bucket() {
    let cases = [
        (
            ProcessMetadataObservation::available(identity(), 10),
            ProcessCategory::Application,
        ),
        (
            ProcessMetadataObservation::partial(
                identity(),
                10,
                ProcessMetadataFailure::Unsupported,
            ),
            ProcessCategory::Application,
        ),
        (
            ProcessMetadataObservation::<ProcessApplicationIdentity>::absent(10),
            ProcessCategory::Background,
        ),
        (
            ProcessMetadataObservation::<ProcessApplicationIdentity>::unavailable(
                ProcessMetadataFailure::PermissionDenied,
            ),
            ProcessCategory::Uncategorized,
        ),
        (
            ProcessMetadataObservation::<ProcessApplicationIdentity>::default(),
            ProcessCategory::Uncategorized,
        ),
    ];
    for (observation, want) in cases {
        let availability = observation.availability();
        assert_eq!(
            process_category(&item_with(observation)),
            want,
            "availability {availability:?} must classify as {want:?}",
        );
    }
}

/// A partial identity (icon resolution failed, identity itself current)
/// still groups as an application — the icon failure is separate from
/// application identity.
#[test]
fn partial_identity_with_icon_failure_still_groups_as_application() {
    let observation =
        ProcessMetadataObservation::partial(identity(), 10, ProcessMetadataFailure::NotFound);
    let item = item_with(observation);
    assert_eq!(item.application_identity_observation().availability(), {
        ProcessMetadataAvailability::Partial(ProcessMetadataFailure::NotFound)
    });
    assert_eq!(process_category(&item), ProcessCategory::Application);
}

/// Stale history — a previously verified identity OR a previously
/// confirmed absence — is no longer current, so neither bucket is
/// provable and both must fall to Uncategorized.
#[test]
fn stale_history_never_masquerades_as_a_current_bucket() {
    let stale_identity = ProcessMetadataObservation::available(identity(), 10)
        .transition_failure(ProcessMetadataFailure::PidRace);
    assert_eq!(
        process_category(&item_with(stale_identity)),
        ProcessCategory::Uncategorized,
        "a stale identity must not stay in Application"
    );

    let stale_absence = ProcessMetadataObservation::<ProcessApplicationIdentity>::absent(10)
        .transition_failure(ProcessMetadataFailure::PidRace);
    assert_eq!(
        process_category(&item_with(stale_absence)),
        ProcessCategory::Uncategorized,
        "a stale confirmed absence must not stay in Background"
    );
}

/// The honesty invariant: an Unknown identity (legacy payload, provider
/// never reported) must never be fabricated into Background.
#[test]
fn unknown_identity_never_falls_into_background() {
    let unknown = item_with(ProcessMetadataObservation::default());
    assert_ne!(process_category(&unknown), ProcessCategory::Background);
    assert_eq!(process_category(&unknown), ProcessCategory::Uncategorized);
}

/// `ALL` carries every bucket exactly once, in evaluation order, with
/// distinct stable keys (the frontend expansion-set discriminator).
#[test]
fn all_lists_every_bucket_once_with_distinct_stable_keys() {
    assert_eq!(ProcessCategory::ALL.len(), 3);
    assert_eq!(
        ProcessCategory::ALL,
        [
            ProcessCategory::Application,
            ProcessCategory::Background,
            ProcessCategory::Uncategorized,
        ]
    );
    let key_list: Vec<&str> = ProcessCategory::ALL
        .iter()
        .map(|c| c.stable_key())
        .collect();
    let distinct: std::collections::HashSet<&str> = key_list.iter().copied().collect();
    assert_eq!(
        key_list.len(),
        distinct.len(),
        "stable keys must be distinct"
    );
}

#[test]
fn identity_requires_real_launcher_and_display_values() {
    assert!(ProcessApplicationIdentity::new(" ", "Editor", None).is_none());
    assert!(ProcessApplicationIdentity::new("org.example.Editor", " ", None).is_none());
}

#[test]
fn identity_normalizes_outer_whitespace_without_fabricating_an_icon() {
    let identity = ProcessApplicationIdentity::new(
        " org.example.Editor ",
        " Example Editor ",
        Some("  ".to_owned()),
    )
    .expect("non-empty identity fixture");

    assert_eq!(identity.launcher_id, "org.example.Editor");
    assert_eq!(identity.display_name, "Example Editor");
    assert_eq!(identity.icon_token, None);
    assert_eq!(identity.icon_asset, None);
    assert!(!identity.has_icon_token());
}

#[test]
fn identity_wire_keeps_an_explicit_icon_token_opaque() {
    let identity = ProcessApplicationIdentity::new(
        "org.example.Editor",
        "Editor",
        Some("/usr/share/icons/hicolor/scalable/apps/editor.svg".to_owned()),
    )
    .expect("icon-bearing identity fixture");
    let decoded: ProcessApplicationIdentity =
        serde_json::from_value(serde_json::to_value(&identity).expect("identity serialization"))
            .expect("identity deserialization");

    assert_eq!(decoded, identity);
    assert!(decoded.has_icon_token());
}

#[test]
fn icon_asset_is_bounded_reference_counted_and_round_trips() {
    let asset =
        ApplicationIconAsset::from_bytes(ApplicationIconFormat::Png, b"\x89PNG\r\n\x1a\n".to_vec())
            .expect("small icon fixture should be accepted");
    let identity = ProcessApplicationIdentity::new("org.example.Editor", "Editor", None)
        .expect("identity fixture")
        .with_icon_resolution(Some(asset.clone()), None);
    let decoded: ProcessApplicationIdentity =
        serde_json::from_value(serde_json::to_value(&identity).expect("identity serialization"))
            .expect("identity deserialization");

    assert_eq!(decoded, identity);
    assert_eq!(
        decoded.icon_asset.as_ref().map(|item| item.content_hash),
        Some(asset.content_hash)
    );
    assert!(decoded.has_icon_asset());
}

#[test]
fn icon_asset_rejects_empty_and_oversized_payloads() {
    assert!(ApplicationIconAsset::from_bytes(ApplicationIconFormat::Svg, Vec::new()).is_none());
    assert!(
        ApplicationIconAsset::from_bytes(
            ApplicationIconFormat::Svg,
            vec![0; MAX_APPLICATION_ICON_BYTES + 1],
        )
        .is_none()
    );
}

#[test]
fn icon_asset_wire_rejects_a_fabricated_content_hash() {
    let asset = ApplicationIconAsset::from_bytes(ApplicationIconFormat::Svg, b"<svg/>".to_vec())
        .expect("small SVG fixture");
    let mut wire = serde_json::to_value(&asset).expect("asset serialization");
    wire["content_hash"] = serde_json::json!(0);

    let decoded = serde_json::from_value::<ApplicationIconAsset>(wire);
    assert!(decoded.is_err(), "icon bytes and cache key must be coupled");
}

#[test]
fn icon_asset_rejects_bytes_that_do_not_match_the_declared_format() {
    assert!(
        ApplicationIconAsset::from_bytes(ApplicationIconFormat::Png, b"<svg/>".to_vec()).is_none()
    );
    assert!(
        ApplicationIconAsset::from_bytes(ApplicationIconFormat::Svg, b"not-an-image".to_vec())
            .is_none()
    );
    assert!(
        ApplicationIconAsset::from_bytes(
            ApplicationIconFormat::Svg,
            b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>".to_vec()
        )
        .is_some()
    );
}
