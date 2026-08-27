//! Fixture-driven proof of the kernel process-snapshot walk behind the
//! compressed-memory fact. The fixtures reproduce the documented
//! `SYSTEM_PROCESS_INFORMATION` entry layout — including the in-snapshot
//! image-name pointers — so the walk's matching, chaining, and corruption
//! handling are all exercised off-Windows.

use super::*;

/// Fixed stride per fixture entry: one full header plus room for the
/// entry's UTF-16 image name and its terminating NUL.
const FIXTURE_ENTRY_STRIDE: usize = 512;

/// Arbitrary base address the fixture "snapshot" pretends to occupy; the
/// entry name pointers are recorded relative to it, exactly like the kernel
/// records them relative to the real buffer.
const FIXTURE_BUFFER_BASE: usize = 0x0100_0000;

fn put_u16(buffer: &mut [u8], at: usize, value: u16) {
    buffer[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(buffer: &mut [u8], at: usize, value: u32) {
    buffer[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_usize(buffer: &mut [u8], at: usize, value: usize) {
    buffer[at..at + POINTER_BYTES].copy_from_slice(&value.to_le_bytes());
}

/// Append one entry with a NUL-terminated UTF-16 name stored after the
/// entry header, `NextEntryOffset` chaining, and the given working set.
fn push_fixture_entry(
    snapshot: &mut Vec<u8>,
    image_name: &str,
    working_set: usize,
    next_offset_from_entry_start: usize,
) {
    let entry_start = snapshot.len();
    snapshot.resize(entry_start + FIXTURE_ENTRY_STRIDE, 0);
    let name_offset = entry_start + MIN_ENTRY_BYTES;
    for (index, unit) in image_name.encode_utf16().enumerate() {
        put_u16(snapshot, name_offset + index * 2, unit);
    }
    put_u16(
        snapshot,
        entry_start + IMAGE_NAME_LENGTH_FIELD,
        (image_name.len() * 2) as u16,
    );
    put_usize(
        snapshot,
        entry_start + IMAGE_NAME_BUFFER_FIELD,
        FIXTURE_BUFFER_BASE + name_offset,
    );
    put_usize(snapshot, entry_start + WORKING_SET_SIZE_FIELD, working_set);
    put_u32(
        snapshot,
        entry_start + NEXT_ENTRY_OFFSET_FIELD,
        next_offset_from_entry_start as u32,
    );
}

fn three_process_snapshot(first: &str, second: &str, third: &str) -> Vec<u8> {
    let mut snapshot = Vec::new();
    push_fixture_entry(&mut snapshot, first, 4096, FIXTURE_ENTRY_STRIDE);
    push_fixture_entry(&mut snapshot, second, 12_288, FIXTURE_ENTRY_STRIDE);
    // The final entry closes the chain.
    push_fixture_entry(&mut snapshot, third, 8_192, 0);
    snapshot
}

#[test]
fn compression_store_working_set_is_found_mid_snapshot() {
    let snapshot = three_process_snapshot("System", "Memory Compression", "explorer.exe");
    assert_eq!(
        find_memory_compression_working_set(&snapshot, FIXTURE_BUFFER_BASE),
        Ok(Some(12_288))
    );
}

#[test]
fn snapshot_without_the_store_reports_honest_absence() {
    let snapshot = three_process_snapshot("System", "explorer.exe", "MsMpEng.exe");
    assert_eq!(
        find_memory_compression_working_set(&snapshot, FIXTURE_BUFFER_BASE),
        Ok(None)
    );
}

#[test]
fn the_store_match_is_exact_not_case_or_substring_tolerant() {
    let lowercased = three_process_snapshot("System", "memory compression", "explorer.exe");
    assert_eq!(
        find_memory_compression_working_set(&lowercased, FIXTURE_BUFFER_BASE),
        Ok(None),
        "a case-mismatched store name must not match"
    );
    let suffixed = three_process_snapshot("System", "Memory Compression Store", "explorer.exe");
    assert_eq!(
        find_memory_compression_working_set(&suffixed, FIXTURE_BUFFER_BASE),
        Ok(None),
        "a longer name with the same prefix must not match"
    );
}

#[test]
fn an_entry_with_no_name_never_dereferences_its_null_buffer() {
    // The idle pseudo-process reports an empty name: the length guard must
    // reject it before the null `Buffer` pointer is ever trusted.
    let mut snapshot = Vec::new();
    push_fixture_entry(&mut snapshot, "", 4096, FIXTURE_ENTRY_STRIDE);
    push_fixture_entry(&mut snapshot, "explorer.exe", 8_192, 0);
    put_usize(&mut snapshot, IMAGE_NAME_BUFFER_FIELD, 0);
    assert_eq!(
        find_memory_compression_working_set(&snapshot, FIXTURE_BUFFER_BASE),
        Ok(None)
    );
}

#[test]
fn a_chain_that_leaves_the_buffer_is_a_typed_query_failure() {
    let mut snapshot = Vec::new();
    push_fixture_entry(&mut snapshot, "System", 4096, FIXTURE_ENTRY_STRIDE * 16);
    assert_eq!(
        find_memory_compression_working_set(&snapshot, FIXTURE_BUFFER_BASE),
        Err(WindowsApiError::QueryFailed)
    );
}

#[test]
fn a_non_final_link_that_does_not_advance_is_a_typed_query_failure() {
    let mut snapshot = Vec::new();
    // A non-zero hop smaller than one entry header could loop forever.
    push_fixture_entry(&mut snapshot, "System", 4096, 4);
    assert_eq!(
        find_memory_compression_working_set(&snapshot, FIXTURE_BUFFER_BASE),
        Err(WindowsApiError::QueryFailed)
    );
}

#[test]
fn a_truncated_or_empty_snapshot_is_a_typed_query_failure() {
    let truncated = vec![0_u8; MIN_ENTRY_BYTES - 1];
    assert_eq!(
        find_memory_compression_working_set(&truncated, FIXTURE_BUFFER_BASE),
        Err(WindowsApiError::QueryFailed)
    );
    assert_eq!(
        find_memory_compression_working_set(&[], FIXTURE_BUFFER_BASE),
        Err(WindowsApiError::QueryFailed)
    );
}
