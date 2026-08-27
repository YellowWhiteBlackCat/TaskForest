//! Audited Windows Event Log (winevt) access.
//!
//! Service-log snapshots, incremental log streams, and boot evidence all read
//! the same documented `wevtapi` path (`EvtQuery`/`EvtNext`/`EvtRender`/
//! `EvtFormatMessage`). No command interpreter is involved. Every
//! `EVT_HANDLE` is owned by an RAII guard, every buffer is bounded, and only
//! typed entries cross this module — no handle, pointer, or UTF-16 buffer
//! escapes.
//!
//! Rendering strategy: each event is rendered as XML and parsed by the pure
//! helpers below (which also run on non-Windows hosts, so the parser is
//! contract-testable on Linux CI). The human-readable message is formatted via
//! `EvtFormatMessage` with one publisher-metadata handle per distinct provider
//! per query (bounded); a message that cannot be formatted stays empty — the
//! event data is returned separately so callers can fall back honestly
//! instead of receiving fabricated text.

use crate::WindowsApiError;

/// Upper bound on entries returned by one query call. A follow poll can never
/// exceed the contract's feed capacity no matter how noisy the channel is.
pub const MAX_EVENT_LOG_ENTRIES_PER_QUERY: usize = 256;
/// Upper bound on one rendered event XML document, in bytes.
const MAX_EVENT_XML_BYTES: usize = 1024 * 1024;
/// Upper bound on one formatted event message, in UTF-16 units.
const MAX_EVENT_MESSAGE_UNITS: usize = 8_192;
/// Upper bound on parsed event-data items per event.
const MAX_EVENT_PROPERTIES_PER_EVENT: usize = 64;
/// Upper bound on publisher-metadata handles opened per query call.
const MAX_PUBLISHER_METADATA_PER_QUERY: usize = 8;
/// Upper bound on channel and provider names, in bytes.
const MAX_EVENT_LOG_NAME_BYTES: usize = 512;
/// `EvtNext` wait bound per batch in milliseconds.
const EVENT_NEXT_TIMEOUT_MS: u32 = 1_000;
/// `EvtNext` batch bound; the handle array never grows past this.
const EVENT_NEXT_BATCH: usize = 64;

/// A bounded Windows Event Log query. `after_record_id` selects the lane:
/// `None` returns the newest `limit` events in chronological order (snapshot
/// semantics), `Some(id)` returns up to `limit` events with
/// `EventRecordID > id`, oldest first (incremental stream semantics — the
/// Windows equivalent of journalctl's `--after-cursor`, using the channel's
/// monotonically increasing record id as the cursor).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct WindowsEventLogQuery {
    pub channel: String,
    pub provider: Option<String>,
    pub event_id: Option<u32>,
    pub after_record_id: Option<u64>,
}

/// One typed event-log entry. `level` is the raw Windows level (1 Critical,
/// 2 Error, 3 Warning, 4 Informational, 5 Verbose; `None` when the publisher
/// omitted it). `message` is the formatted publisher message and is empty —
/// never fabricated — when it could not be formatted. `properties` are the
/// rendered event-data name/value pairs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowsEventLogEntry {
    pub record_id: u64,
    pub timestamp_ms: Option<u64>,
    pub provider: Option<String>,
    pub event_id: u32,
    pub level: Option<u8>,
    pub message: String,
    pub properties: Vec<(String, String)>,
}

/// Query a bounded batch of events from one channel. See
/// [`WindowsEventLogQuery`] for the snapshot/stream lane semantics.
#[must_use = "inspect the native event log query result"]
pub fn query_event_log(
    query: &WindowsEventLogQuery,
    limit: usize,
) -> Result<Vec<WindowsEventLogEntry>, WindowsApiError> {
    validate_query(query)?;
    let limit = limit.min(MAX_EVENT_LOG_ENTRIES_PER_QUERY);
    if limit == 0 {
        return Err(WindowsApiError::InvalidInput);
    }
    #[cfg(windows)]
    {
        query_event_log_windows(query, limit)
    }
    #[cfg(not(windows))]
    {
        let _ = (query, limit);
        Err(WindowsApiError::Unsupported)
    }
}

fn validate_query(query: &WindowsEventLogQuery) -> Result<(), WindowsApiError> {
    if query.channel.is_empty()
        || query.channel.contains('\0')
        || query.channel.len() > MAX_EVENT_LOG_NAME_BYTES
    {
        return Err(WindowsApiError::InvalidInput);
    }
    if let Some(provider) = &query.provider
        && (provider.is_empty()
            || provider.contains('\0')
            || provider.len() > MAX_EVENT_LOG_NAME_BYTES)
    {
        return Err(WindowsApiError::InvalidInput);
    }
    Ok(())
}

/// Compose the XPath filter from typed parts so no caller-supplied XPath
/// string is ever spliced into the native query.
fn build_event_log_xpath(
    provider: Option<&str>,
    event_id: Option<u32>,
    after_record_id: Option<u64>,
) -> String {
    let mut predicates = Vec::new();
    if let Some(provider) = provider {
        predicates.push(format!("Provider[@Name='{}']", escape_xpath_text(provider)));
    }
    if let Some(event_id) = event_id {
        predicates.push(format!("EventID={event_id}"));
    }
    if let Some(record_id) = after_record_id {
        predicates.push(format!("EventRecordID>{record_id}"));
    }
    if predicates.is_empty() {
        "*".to_string()
    } else {
        format!("*[System[{}]]", predicates.join(" and "))
    }
}

/// The XPath string is parsed as XML by winevt, so provider text must be
/// entity-escaped; `&` first so no escaped entity is escaped twice.
fn escape_xpath_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('\'', "&apos;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Decoded fields of one rendered event XML document.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ParsedEventXml {
    record_id: Option<u64>,
    event_id: Option<u32>,
    level: Option<u8>,
    provider: Option<String>,
    timestamp_ms: Option<u64>,
    rendering_message: Option<String>,
    properties: Vec<(String, String)>,
}

/// Parse one `EvtRender(EvtRenderEventXml)` document. Tolerant by design:
/// missing elements become `None`, and unknown structures never fail the
/// whole query — an event without a record id is skipped by the caller, never
/// guessed.
fn parse_event_xml(xml: &str) -> ParsedEventXml {
    let mut parsed = ParsedEventXml::default();
    if let Some(system) = extract_element(xml, "System") {
        parsed.provider = extract_attribute_value(&system, "Provider", "Name");
        parsed.event_id = extract_element_text(&system, "EventID")
            .and_then(|text| text.split('.').next().unwrap_or(&text).parse().ok());
        parsed.level = extract_element_text(&system, "Level").and_then(|text| text.parse().ok());
        // Rendered XML has used both spellings across SDK versions.
        parsed.record_id = extract_element_text(&system, "EventRecordID")
            .or_else(|| extract_element_text(&system, "RecordId"))
            .and_then(|text| text.parse().ok());
        parsed.timestamp_ms = extract_attribute_value(&system, "TimeCreated", "SystemTime")
            .and_then(|stamp| parse_event_timestamp_ms(&stamp));
    }
    if let Some(rendering) = extract_element(xml, "RenderingInfo") {
        parsed.rendering_message = extract_element_text(&rendering, "Message");
    }
    let container = extract_element(xml, "EventData").or_else(|| extract_element(xml, "UserData"));
    if let Some(container) = container {
        let mut cursor = 0usize;
        while parsed.properties.len() < MAX_EVENT_PROPERTIES_PER_EVENT {
            let Some((start, end)) = locate_element(&container[cursor..], "Data") else {
                break;
            };
            let element = &container[cursor + start..cursor + end];
            let value = unescape_xml_text(element_text(element));
            // Classic providers emit unnamed `<Data>` values; name them by
            // position so classic and manifest events stay distinguishable.
            let positional = format!("param{}", parsed.properties.len() + 1);
            let name = extract_attribute(element, "Name")
                .as_deref()
                .map(unescape_xml_text)
                .unwrap_or(positional);
            parsed.properties.push((name, value));
            cursor += end;
        }
    }
    parsed
}

/// Byte offsets (start of open tag, end of close tag) of the first
/// `<name ...>...</name>` element inside `haystack`, ignoring namespaces.
/// The element name must end at a tag boundary so `Data` never matches
/// `Database` and `System` never matches `SystemTime`.
fn locate_element(haystack: &str, name: &str) -> Option<(usize, usize)> {
    let open = format!("<{name}");
    let close = format!("</{name}>");
    let mut search_from = 0usize;
    let open_start = loop {
        let at = haystack[search_from..].find(&open)? + search_from;
        let after = at + open.len();
        match haystack[after..].chars().next() {
            Some(boundary) if boundary == '>' || boundary == '/' || boundary.is_whitespace() => {
                break at;
            }
            Some(_) => search_from = after,
            None => return None,
        }
    };
    let after_open = open_start + open.len();
    let tag_end = haystack[after_open..].find('>')? + after_open;
    if haystack.as_bytes().get(tag_end.wrapping_sub(1)) == Some(&b'/') {
        // Self-closing `<name ... />` carries no text.
        return Some((open_start, tag_end + 1));
    }
    let close_start = haystack[tag_end..].find(&close)? + tag_end;
    Some((open_start, close_start + close.len()))
}

fn extract_element(xml: &str, name: &str) -> Option<String> {
    let (start, end) = locate_element(xml, name)?;
    Some(xml[start..end].to_string())
}

/// Inner text of the first `<name ...>text</name>` element, if non-empty.
fn extract_element_text(xml: &str, name: &str) -> Option<String> {
    let (start, end) = locate_element(xml, name)?;
    let element = &xml[start..end];
    let text = element_text(element);
    (!text.is_empty()).then(|| unescape_xml_text(text))
}

/// Value of `attribute='...'`/`attribute="..."` on the first `<name ...>`
/// element.
fn extract_attribute_value(xml: &str, name: &str, attribute: &str) -> Option<String> {
    let (start, _) = locate_element(xml, name)?;
    let element = &xml[start..];
    extract_attribute(element, attribute).map(|value| unescape_xml_text(&value))
}

/// Raw attribute value from an element string's opening tag, no unescaping.
fn extract_attribute(element: &str, attribute: &str) -> Option<String> {
    let open_end = element.find('>')?;
    let tag = &element[..open_end];
    for quote in ['\'', '"'] {
        let needle = format!("{attribute}={quote}");
        if let Some(at) = tag.find(&needle) {
            let value_start = at + needle.len();
            let value_end = tag[value_start..].find(quote)? + value_start;
            let value = tag[value_start..value_end].to_string();
            return (!value.is_empty()).then_some(value);
        }
    }
    None
}

fn element_text(element: &str) -> &str {
    match (element.find('>'), element.rfind("</")) {
        (Some(open_end), Some(close_start)) if close_start > open_end => {
            &element[open_end + 1..close_start]
        }
        _ => "",
    }
}

fn unescape_xml_text(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// Parse an event `SystemTime` stamp (`2015-01-01T03:04:05.123456789Z` or a
/// `±HH:MM` offset) into epoch milliseconds. Sub-millisecond precision is
/// truncated; a malformed stamp is `None` rather than zero.
fn parse_event_timestamp_ms(stamp: &str) -> Option<u64> {
    let (date, rest) = stamp.split_once('T')?;
    // The offset marker (if any) starts after the minimal HH:MM:SS time.
    let (time, offset) = match rest.rfind(['Z', 'z', '+', '-']) {
        Some(at) if at >= 8 => (&rest[..at], Some(&rest[at..])),
        _ => (rest, None),
    };
    let mut date_fields = date.split('-');
    let year: i64 = date_fields.next()?.parse().ok()?;
    let month: i64 = date_fields.next()?.parse().ok()?;
    let day: i64 = date_fields.next()?.parse().ok()?;
    let mut time_fields = time.split(':');
    let hour: i64 = time_fields.next()?.parse().ok()?;
    let minute: i64 = time_fields.next()?.parse().ok()?;
    let seconds_field = time_fields.next()?;
    let (second, millis): (i64, u64) = match seconds_field.split_once('.') {
        Some((seconds, fraction)) => {
            let mut millis = fraction.to_string();
            millis.truncate(3);
            while millis.len() < 3 {
                millis.push('0');
            }
            (seconds.parse().ok()?, millis.parse().ok()?)
        }
        None => (seconds_field.parse().ok()?, 0),
    };
    let offset_seconds = match offset {
        None | Some("Z") | Some("z") => 0_i64,
        Some(text) => {
            let sign = if text.starts_with('-') { -1_i64 } else { 1_i64 };
            let body = text.trim_start_matches(['+', '-']);
            let (offset_hour, offset_minute) = body.split_once(':')?;
            sign * (offset_hour.parse::<i64>().ok()? * 3_600
                + offset_minute.parse::<i64>().ok()? * 60)
        }
    };
    let days = days_from_civil(year, month, day)?;
    let seconds = days * 86_400 + hour * 3_600 + minute * 60 + second - offset_seconds;
    (seconds >= 0)
        .then(|| u64::try_from(seconds).ok())
        .flatten()
        .map(|seconds| seconds * 1_000 + millis)
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's
/// `days_from_civil`), inverted by the pure timestamp tests below.
fn days_from_civil(year: i64, month: i64, day: i64) -> Option<i64> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year.rem_euclid(400);
    let month_shifted = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * month_shifted + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    Some(era * 146_097 + day_of_era - 719_468)
}

#[cfg(windows)]
struct EventHandleGuard(windows::Win32::System::EventLog::EVT_HANDLE);

#[cfg(windows)]
impl EventHandleGuard {
    fn new(handle: windows::Win32::System::EventLog::EVT_HANDLE) -> Option<Self> {
        (handle.0 != 0).then_some(Self(handle))
    }
}

#[cfg(windows)]
impl Drop for EventHandleGuard {
    fn drop(&mut self) {
        // SAFETY: the handle was returned by EvtQuery/EvtNext/
        // EvtOpenPublisherMetadata and is owned exclusively by this guard;
        // Drop runs at most once.
        let _ = unsafe { windows::Win32::System::EventLog::EvtClose(self.0) };
    }
}

/// Bounded per-query cache of publisher-metadata handles used as formatting
/// contexts by `EvtFormatMessage`. All handles close when the query ends.
#[cfg(windows)]
struct PublisherMetadataCache {
    entries: Vec<(Vec<u16>, windows::Win32::System::EventLog::EVT_HANDLE)>,
}

#[cfg(windows)]
impl PublisherMetadataCache {
    fn open(&mut self, provider: &str) -> Option<windows::Win32::System::EventLog::EVT_HANDLE> {
        let encoded = encode_utf16(provider)?;
        if let Some((_, handle)) = self.entries.iter().find(|(name, _)| *name == encoded) {
            return Some(*handle);
        }
        if self.entries.len() >= MAX_PUBLISHER_METADATA_PER_QUERY {
            return None;
        }
        use windows::Win32::System::EventLog::EvtOpenPublisherMetadata;
        use windows::core::PCWSTR;
        let handle = {
            // SAFETY: `encoded` is a bounded NUL-terminated UTF-16 buffer
            // alive for this synchronous call; null session/file path and
            // flags select the local machine's current metadata.
            unsafe {
                EvtOpenPublisherMetadata(None, PCWSTR(encoded.as_ptr()), PCWSTR::null(), 0, 0)
            }
        }
        .ok()
        .filter(|handle| handle.0 != 0)?;
        self.entries.push((encoded, handle));
        Some(handle)
    }
}

#[cfg(windows)]
impl Drop for PublisherMetadataCache {
    fn drop(&mut self) {
        for (_, handle) in &self.entries {
            // SAFETY: every stored handle came from EvtOpenPublisherMetadata
            // and is owned exclusively by this cache; Drop runs at most once
            // each.
            let _ = unsafe { windows::Win32::System::EventLog::EvtClose(*handle) };
        }
    }
}

#[cfg(windows)]
fn encode_utf16(value: &str) -> Option<Vec<u16>> {
    if value.is_empty() || value.contains('\0') || value.len() > MAX_EVENT_LOG_NAME_BYTES {
        return None;
    }
    Some(value.encode_utf16().chain(std::iter::once(0)).collect())
}

#[cfg(windows)]
fn query_event_log_windows(
    query: &WindowsEventLogQuery,
    limit: usize,
) -> Result<Vec<WindowsEventLogEntry>, WindowsApiError> {
    use windows::Win32::System::EventLog::{
        EvtNext, EvtQuery, EvtQueryChannelPath, EvtQueryForwardDirection, EvtQueryReverseDirection,
    };
    use windows::core::PCWSTR;

    let channel = encode_utf16(&query.channel).ok_or(WindowsApiError::InvalidInput)?;
    let xpath = build_event_log_xpath(
        query.provider.as_deref(),
        query.event_id,
        query.after_record_id,
    );
    let xpath = encode_utf16(&xpath).ok_or(WindowsApiError::InvalidInput)?;
    // Snapshot lane: newest first, then chronological on return. Stream lane:
    // forward from the cursor record id, already chronological.
    let direction = if query.after_record_id.is_some() {
        EvtQueryForwardDirection
    } else {
        EvtQueryReverseDirection
    };
    let result_set = {
        // SAFETY: both wide buffers are bounded and NUL-terminated and stay
        // alive for this synchronous call; the null session selects the local
        // machine; the returned handle is owned by the RAII guard below.
        unsafe {
            EvtQuery(
                None,
                PCWSTR(channel.as_ptr()),
                PCWSTR(xpath.as_ptr()),
                EvtQueryChannelPath.0 | direction.0,
            )
        }
    }
    .map_err(map_event_log_error)?;
    let result_set = EventHandleGuard::new(result_set).ok_or(WindowsApiError::QueryFailed)?;
    let mut metadata = PublisherMetadataCache {
        entries: Vec::new(),
    };
    let mut collected: Vec<WindowsEventLogEntry> = Vec::new();
    while collected.len() < limit {
        let wanted = limit - collected.len();
        let mut batch = vec![0_isize; wanted.min(EVENT_NEXT_BATCH)];
        let mut returned = 0_u32;
        let outcome = {
            // SAFETY: `batch` is a writable array of handle slots whose
            // length matches the requested size; `returned` is a writable
            // local; every returned handle is closed by an RAII guard.
            unsafe {
                EvtNext(
                    result_set.0,
                    &mut batch,
                    EVENT_NEXT_TIMEOUT_MS,
                    0,
                    &mut returned,
                )
            }
        };
        match outcome {
            Ok(()) => {}
            Err(error) if is_no_more_items_or_timeout(&error) => break,
            Err(error) => return Err(map_event_log_error(error)),
        }
        if returned == 0 {
            break;
        }
        let returned = usize::try_from(returned).map_err(|_| WindowsApiError::ResourceLimit)?;
        if returned > batch.len() {
            return Err(WindowsApiError::ResourceLimit);
        }
        for raw in batch[..returned].iter().copied() {
            if collected.len() == limit {
                break;
            }
            let Some(event) =
                EventHandleGuard::new(windows::Win32::System::EventLog::EVT_HANDLE(raw))
            else {
                continue;
            };
            let xml = render_event_xml(event.0)?;
            let parsed = parse_event_xml(&xml);
            let message = if let Some(rendered) = parsed.rendering_message {
                rendered
            } else {
                parsed
                    .provider
                    .as_deref()
                    .and_then(|provider| metadata.open(provider))
                    .and_then(|context| format_event_message(context, event.0))
                    .unwrap_or_default()
            };
            let Some(record_id) = parsed.record_id else {
                // Without a record id the entry cannot be attributed or
                // resumed; skip it instead of inventing a cursor.
                continue;
            };
            collected.push(WindowsEventLogEntry {
                record_id,
                timestamp_ms: parsed.timestamp_ms,
                provider: parsed.provider,
                event_id: parsed.event_id.unwrap_or_default(),
                level: parsed.level,
                message,
                properties: parsed.properties,
            });
        }
    }
    if query.after_record_id.is_none() {
        collected.reverse();
    }
    Ok(collected)
}

#[cfg(windows)]
fn render_event_xml(
    event: windows::Win32::System::EventLog::EVT_HANDLE,
) -> Result<String, WindowsApiError> {
    use windows::Win32::Foundation::ERROR_INSUFFICIENT_BUFFER;
    use windows::Win32::System::EventLog::{EvtRender, EvtRenderEventXml};

    let mut used = 0_u32;
    let mut property_count = 0_u32;
    let sizing = {
        // SAFETY: the sizing call passes a null buffer with size zero; both
        // output pointers refer to writable locals; the event handle is
        // owned by the caller's guard.
        unsafe {
            EvtRender(
                None,
                event,
                EvtRenderEventXml.0,
                0,
                None,
                &mut used,
                &mut property_count,
            )
        }
    };
    if let Err(error) = sizing {
        if error.code() != ERROR_INSUFFICIENT_BUFFER.to_hresult() {
            return Err(map_event_log_error(error));
        }
    }
    if used == 0 {
        return Ok(String::new());
    }
    let bytes = usize::try_from(used).map_err(|_| WindowsApiError::ResourceLimit)?;
    if bytes > MAX_EVENT_XML_BYTES || bytes % 2 != 0 {
        return Err(WindowsApiError::ResourceLimit);
    }
    let mut buffer = vec![0_u16; bytes / 2];
    let mut used_after = 0_u32;
    {
        // SAFETY: `buffer` has exactly `used` bytes of capacity and stays
        // alive for this synchronous call; the event handle is guarded.
        unsafe {
            EvtRender(
                None,
                event,
                EvtRenderEventXml.0,
                used,
                Some(buffer.as_mut_ptr().cast::<core::ffi::c_void>()),
                &mut used_after,
                &mut property_count,
            )
        }
    }
    .map_err(map_event_log_error)?;
    let units = usize::try_from(used_after).map_err(|_| WindowsApiError::ResourceLimit)?;
    if units > buffer.len() {
        return Err(WindowsApiError::ResourceLimit);
    }
    let end = buffer
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(buffer.len());
    String::from_utf16(&buffer[..end]).map_err(|_| WindowsApiError::InvalidText)
}

#[cfg(windows)]
fn format_event_message(
    metadata: windows::Win32::System::EventLog::EVT_HANDLE,
    event: windows::Win32::System::EventLog::EVT_HANDLE,
) -> Option<String> {
    use windows::Win32::Foundation::ERROR_INSUFFICIENT_BUFFER;
    use windows::Win32::System::EventLog::{EvtFormatMessage, EvtFormatMessageEvent};

    let mut used = 0_u32;
    let sizing = {
        // SAFETY: the sizing call passes a null buffer with size zero; the
        // metadata and event handles are valid and owned by the caller.
        unsafe {
            EvtFormatMessage(
                Some(metadata),
                Some(event),
                0,
                None,
                EvtFormatMessageEvent.0,
                None,
                &mut used,
            )
        }
    };
    if let Err(error) = sizing {
        // ERROR_INSUFFICIENT_BUFFER is the expected sizing result; any other
        // failure (missing message table, unreadable provider) means no
        // formatted message exists — an honest empty string, not an error.
        if error.code() != ERROR_INSUFFICIENT_BUFFER.to_hresult() {
            return None;
        }
    }
    // `used` counts the terminating NUL.
    let units = usize::try_from(used).ok()?.checked_sub(1)?;
    if units == 0 || units > MAX_EVENT_MESSAGE_UNITS {
        return None;
    }
    let mut buffer = vec![0_u16; units];
    let mut used_after = 0_u32;
    {
        // SAFETY: `buffer` matches the reported capacity and stays alive for
        // this synchronous call; both handles are valid and guarded.
        unsafe {
            EvtFormatMessage(
                Some(metadata),
                Some(event),
                0,
                None,
                EvtFormatMessageEvent.0,
                Some(&mut buffer),
                &mut used_after,
            )
        }
    }
    .ok()?;
    let end = buffer
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(buffer.len());
    let message = String::from_utf16(&buffer[..end]).ok()?;
    (!message.is_empty()).then_some(message)
}

#[cfg(windows)]
fn is_no_more_items_or_timeout(error: &windows::core::Error) -> bool {
    use windows::Win32::Foundation::{ERROR_NO_MORE_ITEMS, ERROR_TIMEOUT};

    let code = error.code();
    code == ERROR_NO_MORE_ITEMS.to_hresult() || code == ERROR_TIMEOUT.to_hresult()
}

#[cfg(windows)]
fn map_event_log_error(error: windows::core::Error) -> WindowsApiError {
    use windows::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_INVALID_PARAMETER};

    let code = error.code();
    if code == ERROR_ACCESS_DENIED.to_hresult() {
        WindowsApiError::PermissionDenied
    } else if code == ERROR_INVALID_PARAMETER.to_hresult() {
        WindowsApiError::InvalidInput
    } else {
        WindowsApiError::QueryFailed
    }
}

#[cfg(test)]
#[path = "../tests/headless/windows_api_event_log.rs"]
mod tests;
