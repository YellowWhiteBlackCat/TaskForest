use super::*;

const SAMPLE_EVENT_XML: &str = "\
<Event xmlns='http://schemas.microsoft.com/win/2004/08/events/event'>
  <System>
    <Provider Name='Service Control Manager' Guid='{555908d1-a6d7-4695-8e1e-26931d2012f4}' EventSourceName='Service Control Manager'/>
    <EventID Qualifiers='16384'>7036</EventID>
    <Version>0</Version>
    <Level>4</Level>
    <Task>0</Task>
    <Opcode>0</Opcode>
    <Keywords>0x8080000000000000</Keywords>
    <TimeCreated SystemTime='2026-01-01T03:04:05.123456789Z'/>
    <EventRecordID>4242</EventRecordID>
    <Correlation ActivityID='{00000000-0000-0000-0000-000000000000}'/>
    <Execution ProcessID='1024' ThreadID='2048'/>
    <Channel>System</Channel>
    <Computer>WORKSTATION</Computer>
    <Security/>
  </System>
  <EventData>
    <Data Name='param1'>W32Time</Data>
    <Data>entered the running state</Data>
  </EventData>
  <RenderingInfo Culture='en-US'>
    <Message>The service entered the running state.</Message>
    <Level>Information</Level>
  </RenderingInfo>
</Event>";

#[test]
fn xpath_is_composed_from_typed_parts_with_escaped_literals() {
    assert_eq!(build_event_log_xpath(None, None, None), "*");
    assert_eq!(
        build_event_log_xpath(Some("W32Time"), None, None),
        "*[System[Provider[@Name='W32Time']]]"
    );
    assert_eq!(
        build_event_log_xpath(None, Some(100), None),
        "*[System[EventID=100]]"
    );
    assert_eq!(
        build_event_log_xpath(None, None, Some(41)),
        "*[System[EventRecordID>41]]"
    );
    assert_eq!(
        build_event_log_xpath(Some("W32Time"), Some(7036), Some(41)),
        "*[System[Provider[@Name='W32Time'] and EventID=7036 and EventRecordID>41]]"
    );
    // Apostrophes and ampersands must not break out of the XPath literal.
    assert_eq!(
        build_event_log_xpath(Some("A&B's <svc>"), None, None),
        "*[System[Provider[@Name='A&amp;B&apos;s &lt;svc&gt;']]]"
    );
}

#[test]
fn rendered_event_xml_decodes_to_typed_fields() {
    let parsed = parse_event_xml(SAMPLE_EVENT_XML);
    assert_eq!(parsed.record_id, Some(4242));
    assert_eq!(parsed.event_id, Some(7036));
    assert_eq!(parsed.level, Some(4));
    assert_eq!(parsed.provider.as_deref(), Some("Service Control Manager"));
    assert_eq!(parsed.timestamp_ms, Some(1_767_236_645_123));
    assert_eq!(
        parsed.rendering_message.as_deref(),
        Some("The service entered the running state.")
    );
    assert_eq!(
        parsed.properties,
        vec![
            ("param1".to_string(), "W32Time".to_string()),
            (
                "param2".to_string(),
                "entered the running state".to_string()
            ),
        ]
    );
}

#[test]
fn record_id_falls_back_to_legacy_spelling_and_data_stays_bounded() {
    let legacy = "\
<System>
  <Provider Name='P'/>
  <EventID>5</EventID>
  <RecordId>7</RecordId>
  <TimeCreated SystemTime='1970-01-01T00:00:00Z'/>
</System>";
    let mut overflow = String::new();
    for index in 0..MAX_EVENT_PROPERTIES_PER_EVENT + 8 {
        overflow.push_str(&format!("<Data>v{index}</Data>"));
    }
    let xml = format!("<Event>{legacy}<EventData>{overflow}</EventData></Event>");
    let parsed = parse_event_xml(&xml);
    assert_eq!(parsed.record_id, Some(7));
    assert_eq!(parsed.timestamp_ms, Some(0));
    assert_eq!(parsed.properties.len(), MAX_EVENT_PROPERTIES_PER_EVENT);
    assert_eq!(
        parsed.properties.first().map(|(name, _)| name.as_str()),
        Some("param1")
    );
}

#[test]
fn timestamp_parsing_truncates_fraction_and_applies_offsets() {
    assert_eq!(parse_event_timestamp_ms("1970-01-01T00:00:00Z"), Some(0));
    assert_eq!(
        parse_event_timestamp_ms("2026-01-01T00:00:00Z"),
        Some(1_767_225_600_000)
    );
    // Nine rendered fraction digits truncate to milliseconds.
    assert_eq!(
        parse_event_timestamp_ms("1970-01-01T00:00:00.123456789Z"),
        Some(123)
    );
    assert_eq!(
        parse_event_timestamp_ms("1970-01-01T00:00:00.1Z"),
        Some(100)
    );
    assert_eq!(
        parse_event_timestamp_ms("1970-01-01T02:00:00+02:00"),
        Some(0)
    );
    assert_eq!(
        parse_event_timestamp_ms("1970-01-01T23:00:00-01:00"),
        Some(86_400_000)
    );
    // Malformed stamps are typed absence, never zero by accident.
    assert_eq!(parse_event_timestamp_ms("not-a-stamp"), None);
    assert_eq!(parse_event_timestamp_ms("2026-13-40T00:00:00Z"), None);
}

#[test]
fn query_validation_and_dormant_lane_are_typed_on_every_host() {
    let mut query = WindowsEventLogQuery {
        channel: "System".to_string(),
        provider: Some("W32Time".to_string()),
        event_id: None,
        after_record_id: None,
    };
    // Structural validation runs everywhere and never fabricates success.
    query.channel = String::new();
    assert_eq!(
        query_event_log(&query, 10),
        Err(WindowsApiError::InvalidInput)
    );
    query.channel = "System".to_string();
    query.provider = Some("a\0b".to_string());
    assert_eq!(
        query_event_log(&query, 10),
        Err(WindowsApiError::InvalidInput)
    );
    query.provider = Some("W32Time".to_string());
    assert_eq!(
        query_event_log(&query, 0),
        Err(WindowsApiError::InvalidInput)
    );
    #[cfg(not(windows))]
    {
        // Off-Windows the native lane is dormant, not empty.
        assert_eq!(
            query_event_log(&query, 10),
            Err(WindowsApiError::Unsupported)
        );
    }
}
