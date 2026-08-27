//! Fixture-driven proof of the PEB environment/cwd walk's pure core. The
//! fixtures reproduce the documented `RTL_USER_PROCESS_PARAMETERS` layout and
//! UTF-16 environment-block shapes, so the width-derived offsets, bounds
//! checks, truncation counting, and noise handling all run off-Windows.

use super::*;

fn put_u16(buffer: &mut [u8], at: usize, value: u16) {
    buffer[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_usize(buffer: &mut [u8], at: usize, value: usize) {
    buffer[at..at + POINTER_BYTES].copy_from_slice(&value.to_le_bytes());
}

/// Serialize `KEY=VALUE` strings as a NUL-separated, double-NUL-terminated
/// UTF-16 environment block.
fn environment_block(entries: &[&str]) -> Vec<u8> {
    let mut block = Vec::new();
    for entry in entries {
        for unit in entry.encode_utf16().chain(std::iter::once(0)) {
            block.extend_from_slice(&unit.to_le_bytes());
        }
    }
    // The block's own terminating empty string.
    block.extend_from_slice(&0_u16.to_le_bytes());
    block
}

#[test]
fn layout_arithmetic_matches_the_documented_offsets() {
    // The offsets this walk derives from the pointer width are exactly the
    // documented structure offsets for each width, so a layout change in a
    // future `windows` crate update cannot silently drift in.
    #[cfg(target_pointer_width = "64")]
    {
        assert_eq!(basic_information_peb_field(), 0x08);
        assert_eq!(basic_information_bytes(), 0x30);
        assert_eq!(peb_process_parameters_field(), 0x20);
        assert_eq!(params_current_directory_field(), 0x38);
        assert_eq!(params_environment_field(), 0x80);
    }
    #[cfg(target_pointer_width = "32")]
    {
        assert_eq!(basic_information_peb_field(), 0x04);
        assert_eq!(basic_information_bytes(), 0x18);
        assert_eq!(peb_process_parameters_field(), 0x10);
        assert_eq!(params_current_directory_field(), 0x24);
        assert_eq!(params_environment_field(), 0x48);
    }
    assert_eq!(unicode_string_bytes(), 2 * POINTER_BYTES);
}

#[test]
fn process_parameters_fields_are_lifted_from_a_laid_out_block() {
    let mut buffer = vec![0_u8; params_environment_field() + POINTER_BYTES];
    let dos_path: Vec<u16> = "C:\\work".encode_utf16().collect();
    put_u16(
        &mut buffer,
        params_current_directory_field(),
        (dos_path.len() * 2) as u16,
    );
    put_usize(
        &mut buffer,
        params_current_directory_field() + unicode_string_buffer_field(),
        0x0000_7fff_0000_1000 & usize::MAX,
    );
    put_usize(
        &mut buffer,
        params_environment_field(),
        0x0000_7fff_0000_2000 & usize::MAX,
    );

    let fields = parse_process_parameters(&buffer).expect("laid-out block parses");

    assert_eq!(fields.dos_path_bytes as usize, dos_path.len() * 2);
    assert_eq!(fields.dos_path_pointer, 0x0000_7fff_0000_1000 & usize::MAX);
    assert_eq!(
        fields.environment_pointer,
        0x0000_7fff_0000_2000 & usize::MAX
    );
}

#[test]
fn a_parameters_block_shorter_than_its_fields_is_a_typed_failure() {
    let one_byte_short = vec![0_u8; params_environment_field() + POINTER_BYTES - 1];
    assert_eq!(
        parse_process_parameters(&one_byte_short),
        Err(WindowsApiError::QueryFailed)
    );
    assert_eq!(
        parse_process_parameters(&[]),
        Err(WindowsApiError::QueryFailed)
    );
}

#[test]
fn environment_entries_keep_source_order_and_skip_noise() {
    let block = environment_block(&[
        "PATH=C:\\bin;C:\\windows",
        "NO_SEPARATOR_NOISE",
        "=EMPTY_KEY_NOISE",
        "TMP=C:\\tmp",
    ]);

    let (entries, truncated) = parse_environment_block(&block, true);

    assert_eq!(
        entries,
        vec![
            ("PATH".to_string(), "C:\\bin;C:\\windows".to_string()),
            ("TMP".to_string(), "C:\\tmp".to_string()),
        ]
    );
    assert_eq!(truncated, 0);
}

#[test]
fn entries_beyond_the_entry_cap_are_counted_not_retained() {
    let sources: Vec<String> = (0..(MAX_PROCESS_ENVIRONMENT_ENTRIES + 3))
        .map(|index| format!("K{index}=v{index}"))
        .collect();
    let refs: Vec<&str> = sources.iter().map(String::as_str).collect();
    let block = environment_block(&refs);

    let (entries, truncated) = parse_environment_block(&block, true);

    assert_eq!(entries.len(), MAX_PROCESS_ENVIRONMENT_ENTRIES);
    // The first retained entries keep source order; the three over-budget
    // entries are reported through the truncation count.
    assert_eq!(entries[0], ("K0".to_string(), "v0".to_string()));
    assert_eq!(truncated, 3);
}

#[test]
fn a_clipped_tail_counts_truncated_and_earlier_entries_survive() {
    let full = environment_block(&["PATH=C:\\bin", "CLIPPED=partial"]);
    // Cut mid-way through the last entry's bytes so it has no closing NUL.
    let clipped_at = full.len() - 5;
    let clipped = &full[..clipped_at];
    assert!(!ends_with_utf16_terminator(clipped));

    let (entries, truncated) = parse_environment_block(clipped, false);

    assert_eq!(entries, vec![("PATH".to_string(), "C:\\bin".to_string())]);
    assert_eq!(truncated, 1);
}

#[test]
fn an_unterminated_block_closed_by_a_nul_keeps_its_last_entry() {
    // A read that stops right after an entry's NUL has no terminator, but
    // the bytes prove that entry closed, so it is kept — only bytes after
    // the last NUL would be unprovable.
    let full = environment_block(&["PATH=C:\\bin", "TMP=C:\\tmp"]);
    let terminator_tail = 2; // the block's terminating empty string: one UTF-16 unit
    let closed = &full[..full.len() - terminator_tail];

    let (entries, truncated) = parse_environment_block(closed, false);

    assert_eq!(
        entries,
        vec![
            ("PATH".to_string(), "C:\\bin".to_string()),
            ("TMP".to_string(), "C:\\tmp".to_string()),
        ]
    );
    assert_eq!(truncated, 0);
}

#[test]
fn an_empty_terminated_block_and_an_empty_prefix_differ_by_proof() {
    let (entries, truncated) = parse_environment_block(&environment_block(&[]), true);
    assert_eq!(entries, Vec::new());
    assert_eq!(truncated, 0);
}

#[cfg(not(windows))]
#[test]
fn the_native_environment_query_is_typed_unsupported_off_windows() {
    assert_eq!(
        query_process_environment(4_242),
        Err(WindowsApiError::Unsupported)
    );
}
