//! Behavior tests for the open-files boundary's pure walk/classification
//! core. Everything here runs off Windows against fixtures laid out with the
//! same serialized NT shapes the FFI section parses.

use super::*;

/// Serialize one `ObjectAllTypes` entry: UNICODE_STRING header, the fixed
/// statistics tail, then the inline NUL-terminated name. The walker reads
/// only the two header lengths and the inline name position, so the pointer
/// and tail bytes are zero filler here.
fn push_type_entry(buffer: &mut Vec<u8>, name: &str) {
    let units: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let struct_bytes = object_type_information_bytes();
    let header_start = buffer.len();
    let name_bytes = (name.encode_utf16().count() * 2) as u16;
    buffer.extend_from_slice(&name_bytes.to_le_bytes());
    buffer.extend_from_slice(&((units.len() * 2) as u16).to_le_bytes());
    buffer.resize(header_start + struct_bytes, 0);
    for unit in units {
        buffer.extend_from_slice(&unit.to_le_bytes());
    }
    buffer.resize(align_to_pointer(buffer.len()), 0);
}

/// Build a complete ObjectAllTypes buffer for the given type names.
fn types_buffer(names: &[&str]) -> Vec<u8> {
    let mut buffer = Vec::new();
    buffer.extend_from_slice(&(names.len() as u32).to_le_bytes());
    buffer.resize(align_to_pointer(4), 0);
    for name in names {
        push_type_entry(&mut buffer, name);
    }
    buffer
}

/// Serialize one `SYSTEM_HANDLE_TABLE_ENTRY_INFO`.
fn push_handle_entry(buffer: &mut Vec<u8>, pid: u16, object_type: u8, handle: u16) {
    let entry_start = buffer.len();
    buffer.extend_from_slice(&pid.to_le_bytes());
    buffer.extend_from_slice(&0_u16.to_le_bytes()); // CreatorBackTraceIndex
    buffer.push(object_type);
    buffer.push(0); // HandleAttributes
    buffer.extend_from_slice(&handle.to_le_bytes());
    buffer.extend(std::iter::repeat_n(0, POINTER_BYTES)); // Object pointer
    buffer.extend_from_slice(&0_u32.to_le_bytes()); // GrantedAccess
    // Trailing alignment padding completes the serialized stride.
    buffer.resize(entry_start + handle_entry_bytes(), 0);
}

fn handle_table(entries: &[(u16, u8, u16)]) -> Vec<u8> {
    let mut buffer = Vec::new();
    buffer.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    buffer.resize(align_to_pointer(4), 0);
    for &(pid, object_type, handle) in entries {
        push_handle_entry(&mut buffer, pid, object_type, handle);
    }
    buffer
}

fn slot_with_name(status: i32, name: &str) -> NameQuerySlot {
    let mut slot = NameQuerySlot {
        status,
        name_units: name.encode_utf16().count(),
        name: [0; MAX_NAME_UTF16_UNITS],
    };
    for (index, unit) in name.encode_utf16().enumerate() {
        slot.name[index] = unit;
    }
    slot
}

#[test]
fn utf16_ascii_comparison_ignores_case_and_rejects_mismatched_shapes() {
    let file_utf16: Vec<u8> = "File".encode_utf16().flat_map(u16::to_le_bytes).collect();
    assert!(utf16_bytes_eq_ascii_ignore_case(&file_utf16, b"File"));
    let mixed: Vec<u8> = "fILE".encode_utf16().flat_map(u16::to_le_bytes).collect();
    assert!(utf16_bytes_eq_ascii_ignore_case(&mixed, b"File"));
    // A high byte set means the unit is not the ASCII letter.
    let non_ascii: Vec<u8> = "Fïle".encode_utf16().flat_map(u16::to_le_bytes).collect();
    assert!(!utf16_bytes_eq_ascii_ignore_case(&non_ascii, b"File"));
    assert!(!utf16_bytes_eq_ascii_ignore_case(b"FI", b"FILE"));
    assert!(!utf16_bytes_eq_ascii_ignore_case(&[], b"F"));
}

#[test]
fn file_type_index_walks_the_serialized_type_list() {
    let buffer = types_buffer(&["Object", "File", "Event", "Directory"]);
    assert_eq!(find_file_type_index(&buffer), Ok(1));
    // Case-insensitive match, as the object namespace is.
    let buffer = types_buffer(&["object", "evEnt", "fIlE"]);
    assert_eq!(find_file_type_index(&buffer), Ok(2));
}

#[test]
fn file_type_index_reports_typed_failures_for_malformed_buffers() {
    // No File type at all: an honest typed failure, never index zero.
    let buffer = types_buffer(&["Object", "Event"]);
    assert_eq!(
        find_file_type_index(&buffer),
        Err(WindowsApiError::QueryFailed)
    );
    // Truncated mid-entry (cutting into the inline name, not just tail
    // padding) and empty buffers fail the same typed way.
    let mut truncated = types_buffer(&["Object", "File"]);
    truncated.truncate(truncated.len() - 12);
    assert_eq!(
        find_file_type_index(&truncated),
        Err(WindowsApiError::QueryFailed)
    );
    assert_eq!(find_file_type_index(&[]), Err(WindowsApiError::QueryFailed));
    // An absurd entry count is refused before any walk.
    let mut bogus = Vec::new();
    bogus.extend_from_slice(&10_000_u32.to_le_bytes());
    assert_eq!(
        find_file_type_index(&bogus),
        Err(WindowsApiError::QueryFailed)
    );
}

#[test]
fn handle_table_parse_filters_by_pid_and_type_and_honors_the_cap() {
    // (pid, object_type, handle): File type index is 28 here.
    let buffer = handle_table(&[
        (12, 28, 0x40),
        (13, 28, 0x50), // wrong pid
        (12, 3, 0x60),  // wrong type (not a File object)
        (12, 28, 0x70),
        (12, 28, 0x80),
    ]);
    let (handles, truncated) =
        parse_target_file_handles(&buffer, 12, 28, 1024).expect("walk succeeds");
    assert_eq!(handles, vec![0x40, 0x70, 0x80]);
    assert!(!truncated);

    // More matching entries than the cap: truncation is reported, never done
    // silently.
    let (handles, truncated) =
        parse_target_file_handles(&buffer, 12, 28, 2).expect("walk succeeds");
    assert_eq!(handles, vec![0x40, 0x70]);
    assert!(truncated);

    // A count larger than the buffer's complete entries consumes only the
    // complete ones (the documented imprecision of this class).
    let mut overrun = handle_table(&[(12, 28, 0x40)]);
    overrun[..4].copy_from_slice(&5_u32.to_le_bytes());
    let (handles, truncated) =
        parse_target_file_handles(&overrun, 12, 28, 1024).expect("walk succeeds");
    assert_eq!(handles, vec![0x40]);
    assert!(!truncated);
}

#[test]
fn named_targets_classify_by_pipe_namespace() {
    use WindowsOpenHandleKind::{File, Other, Pipe};
    assert_eq!(
        classify_named_target(Some(r"\Device\HarddiskVolume3\win.ini")),
        File
    );
    assert_eq!(classify_named_target(Some(r"\Device\NamedPipe\foo")), Pipe);
    assert_eq!(classify_named_target(Some(r"\\.\pipe\foo")), Pipe);
    // The namespace is case-insensitive.
    assert_eq!(classify_named_target(Some(r"\device\namedpipe\foo")), Pipe);
    // A bare pipe root (no instance) is not a pipe instance.
    assert_eq!(classify_named_target(Some(r"\Device\NamedPipe")), File);
    // Unresolvable or unnamed targets stay honestly unclassified.
    assert_eq!(classify_named_target(None), Other);
}

#[test]
fn inline_name_span_trusts_neither_length_nor_pointer() {
    let mut buffer = [0_u16; MAX_NAME_UTF16_UNITS];
    let address = buffer.as_ptr() as usize;
    let name: Vec<u16> = "\\\\.\\pipe\\x".encode_utf16().collect();
    buffer[0] = (name.len() * 2) as u16;
    let inline = address + 2 * POINTER_BYTES;
    for (shift, unit) in (POINTER_BYTES / 2..POINTER_BYTES).enumerate() {
        buffer[unit] = ((inline >> (16 * shift)) & 0xFFFF) as u16;
    }
    buffer[POINTER_BYTES..POINTER_BYTES + name.len()].copy_from_slice(&name);
    assert_eq!(inline_name_units(address, &buffer), Some(name.len()));
    // A pointer aimed anywhere else must invalidate the span.
    buffer[POINTER_BYTES / 2] += 1;
    assert_eq!(inline_name_units(address, &buffer), None);

    // Zero length is a valid unnamed result.
    let mut empty = [0_u16; MAX_NAME_UTF16_UNITS];
    empty[0] = 0;
    assert_eq!(inline_name_units(empty.as_ptr() as usize, &empty), Some(0));
}

#[test]
fn slot_decoding_distinguishes_pending_failed_and_named_results() {
    // Pending worker: sentinel status means no name regardless of units.
    let pending = slot_with_name(NAME_STATUS_PENDING, "abc");
    assert_eq!(decode_slot_name(&pending), None);
    // Failed query: any nonzero NTSTATUS means no name.
    let failed = slot_with_name(STATUS_INFO_LENGTH_MISMATCH, "abc");
    assert_eq!(decode_slot_name(&failed), None);
    // Published success with units decodes lossily.
    let named = slot_with_name(0, "path");
    assert_eq!(decode_slot_name(&named), Some("path".to_string()));
    // Success with zero units is an unnamed object, not an empty name.
    let unnamed = slot_with_name(0, "");
    assert_eq!(decode_slot_name(&unnamed), None);
}

#[cfg(not(windows))]
#[test]
fn open_files_query_is_typed_unsupported_off_windows() {
    assert_eq!(
        query_process_open_files(1234),
        Err(WindowsApiError::Unsupported)
    );
}
