//! Bounded process environment and working-directory observation.

use super::*;

// ---------------------------------------------------------------------------
// Host-independent layout arithmetic and pure walkers.
// ---------------------------------------------------------------------------

/// Pointer width of the compiled target in bytes; valid for the target
/// process because cross-bitness reads are refused before any walk runs.
pub(super) const POINTER_BYTES: usize = core::mem::size_of::<usize>();

/// Round a byte offset up to the pointer alignment of the target.
pub(super) const fn align_to_pointer(value: usize) -> usize {
    (value + POINTER_BYTES - 1) & !(POINTER_BYTES - 1)
}

/// Byte offset of `PebBaseAddress` inside a serialized
/// `PROCESS_BASIC_INFORMATION` (after the 4-byte `ExitStatus`).
pub(super) const fn basic_information_peb_field() -> usize {
    align_to_pointer(4)
}

/// Serialized `PROCESS_BASIC_INFORMATION` size: `ExitStatus`, `PebBaseAddress`,
/// `AffinityMask`, `BasePriority` (each padded to the pointer alignment),
/// `UniqueProcessId`, `InheritedFromUniqueProcessId`.
pub(super) const fn basic_information_bytes() -> usize {
    2 * align_to_pointer(4) + 4 * POINTER_BYTES
}

/// Byte offset of `ProcessParameters` inside a serialized `PEB`: three flag
/// bytes padded to the pointer alignment, then `Mutant`, `ImageBaseAddress`,
/// and `Ldr` — pointer-sized each (0x20 on 64-bit, 0x10 on 32-bit).
pub(super) const fn peb_process_parameters_field() -> usize {
    4 * POINTER_BYTES
}

/// Serialized `UNICODE_STRING` size: two u16 lengths, padding, one pointer
/// (16 bytes on 64-bit, 8 on 32-bit — two pointer widths either way).
pub(super) const fn unicode_string_bytes() -> usize {
    2 * POINTER_BYTES
}

/// Byte offset of the `Buffer` pointer inside one serialized
/// `UNICODE_STRING`.
pub(super) const fn unicode_string_buffer_field() -> usize {
    POINTER_BYTES
}

/// Byte offset of `CurrentDirectory.DosPath` inside a serialized
/// `RTL_USER_PROCESS_PARAMETERS`: after the 16 reserved bytes,
/// `ConsoleHandle`, `ConsoleFlags` (both padded together), and the three
/// standard handles (0x38 on 64-bit, 0x24 on 32-bit).
pub(super) const fn params_current_directory_field() -> usize {
    align_to_pointer(16 + POINTER_BYTES + 4) + 3 * POINTER_BYTES
}

/// Byte offset of the `Environment` pointer: after `CurrentDirectory`
/// (DosPath plus its `Handle`) and the `DllPath`/`ImagePathName`/`CommandLine`
/// `UNICODE_STRING`s (0x80 on 64-bit, 0x48 on 32-bit).
pub(super) const fn params_environment_field() -> usize {
    params_current_directory_field()
        + unicode_string_bytes()
        + POINTER_BYTES
        + 3 * unicode_string_bytes()
}

/// The environment and cwd facts lifted from one serialized
/// `RTL_USER_PROCESS_PARAMETERS` block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ProcessParametersFields {
    /// `CurrentDirectory.DosPath.Length` in bytes (NUL excluded).
    pub(super) dos_path_bytes: u16,
    /// `CurrentDirectory.DosPath.Buffer` address in the target.
    pub(super) dos_path_pointer: usize,
    /// `Environment` block address in the target (0 when the process has no
    /// environment block).
    pub(super) environment_pointer: usize,
}

/// Parse the cwd/environment fields out of a serialized
/// `RTL_USER_PROCESS_PARAMETERS` block. A block shorter than the fields is a
/// typed `QueryFailed`, never an out-of-bounds read.
pub(super) fn parse_process_parameters(
    buffer: &[u8],
) -> Result<ProcessParametersFields, WindowsApiError> {
    let dos_path_bytes = read_le_u16(buffer, params_current_directory_field())?;
    Ok(ProcessParametersFields {
        dos_path_bytes,
        dos_path_pointer: read_le_usize(
            buffer,
            params_current_directory_field() + unicode_string_buffer_field(),
        )?,
        environment_pointer: read_le_usize(buffer, params_environment_field())?,
    })
}

/// True when the block's bytes end with the UTF-16 empty-string terminator:
/// the NUL closing the last `KEY=VALUE` string followed by the block's own
/// terminating NUL (four zero bytes on a unit boundary).
pub(super) fn ends_with_utf16_terminator(bytes: &[u8]) -> bool {
    let units_end = bytes.len() & !1;
    let tail = bytes.get(units_end.saturating_sub(4)..units_end);
    tail == Some(&[0, 0, 0, 0])
}

/// Split a bounded UTF-16 environment block into ordered `KEY=VALUE` pairs.
///
/// Segments without a `=` separator (or with an empty key) are provider noise
/// and skipped. When `terminated` is false the read stopped without the
/// empty-string terminator, so the trailing segment after the last NUL is
/// clipped and counts as truncated; entries beyond the entry cap are counted,
/// not retained. Mirrors the Linux collector's bounds policy.
pub(super) fn parse_environment_block(
    bytes: &[u8],
    terminated: bool,
) -> (Vec<(String, String)>, u32) {
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|unit| u16::from_le_bytes(*unit))
        .collect();
    let units = units.as_slice();
    let mut segments: Vec<&[u16]> = Vec::new();
    let mut start = 0_usize;
    for (index, &unit) in units.iter().enumerate() {
        if unit == 0 {
            segments.push(&units[start..index]);
            start = index + 1;
        }
    }
    let tail = &units[start.min(units.len())..];
    let mut truncated_count = 0_u32;
    // The tail is complete only when the block terminator proved the whole
    // block read, or the bytes themselves closed the segment with a NUL.
    if terminated || units.last() == Some(&0) {
        if !tail.is_empty() {
            segments.push(tail);
        }
    } else if !tail.is_empty() {
        // A clipped final entry: dropped and reported, never half-parsed.
        truncated_count += 1;
    }

    let mut entries = Vec::new();
    for segment in segments {
        if segment.is_empty() {
            continue;
        }
        let text = String::from_utf16_lossy(segment);
        let Some((key, value)) = text.split_once('=') else {
            continue;
        };
        if key.is_empty() {
            continue;
        }
        if entries.len() >= MAX_PROCESS_ENVIRONMENT_ENTRIES {
            truncated_count = truncated_count.saturating_add(1);
            continue;
        }
        entries.push((key.to_string(), value.to_string()));
    }
    (entries, truncated_count)
}

fn read_le_u16(buffer: &[u8], offset: usize) -> Result<u16, WindowsApiError> {
    let bytes = buffer
        .get(offset..offset.saturating_add(2))
        .ok_or(WindowsApiError::QueryFailed)?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_le_usize(buffer: &[u8], offset: usize) -> Result<usize, WindowsApiError> {
    let end = offset
        .checked_add(POINTER_BYTES)
        .ok_or(WindowsApiError::QueryFailed)?;
    let bytes = buffer
        .get(offset..end)
        .ok_or(WindowsApiError::QueryFailed)?;
    let mut raw = [0_u8; core::mem::size_of::<usize>()];
    raw.copy_from_slice(bytes);
    Ok(usize::from_le_bytes(raw))
}

// ---------------------------------------------------------------------------
// Windows FFI section.
// ---------------------------------------------------------------------------

#[cfg(windows)]
pub(super) fn query_process_environment_windows(
    pid: u32,
) -> Result<WindowsProcessEnvironmentBlock, WindowsApiError> {
    use windows::Wdk::System::Threading::{NtQueryInformationProcess, ProcessBasicInformation};
    use windows::Win32::System::Threading::{
        GetCurrentProcess, IsWow64Process, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
    };

    if pid == 0 {
        return Err(WindowsApiError::InvalidInput);
    }

    // Same-user processes answer this open; protected or higher-integrity
    // owners refuse it as a typed permission failure, and a vanished pid is
    // an identity change — the same access semantics as the other insights.
    let process = open_process(pid, PROCESS_QUERY_INFORMATION | PROCESS_VM_READ)?;

    // Cross-bitness guard: a WOW64 target has a second PEB whose layout
    // differs from this build's, so the walk refuses to guess it. Equal WOW64
    // status on both sides implies equal pointer widths in every host mix
    // (64/64, 32/32, and 32-bit Windows where both answers are false).
    let mut self_wow64 = windows::core::BOOL::default();
    let mut target_wow64 = windows::core::BOOL::default();
    // SAFETY: `GetCurrentProcess` returns the current pseudo-handle, and
    // `self_wow64` is a matching writable output for this synchronous call.
    unsafe { IsWow64Process(GetCurrentProcess(), &mut self_wow64) }
        .map_err(|_| WindowsApiError::QueryFailed)?;
    // SAFETY: `process.0` is an owned valid process handle, and
    // `target_wow64` is a matching writable output.
    unsafe { IsWow64Process(process.0, &mut target_wow64) }
        .map_err(|_| WindowsApiError::QueryFailed)?;
    if self_wow64.as_bool() != target_wow64.as_bool() {
        return Err(WindowsApiError::Unsupported);
    }

    let mut basic_information = [0_u8; basic_information_bytes()];
    let mut returned_bytes = 0_u32;
    // SAFETY: `process.0` is an owned valid handle, `basic_information` is a
    // writable allocation whose length is passed in the matching parameter,
    // and `returned_bytes` receives the kernel's byte count; the query opens
    // no handles and retains no caller pointers.
    let status = unsafe {
        NtQueryInformationProcess(
            process.0,
            ProcessBasicInformation,
            basic_information.as_mut_ptr().cast(),
            u32::try_from(basic_information.len()).map_err(|_| WindowsApiError::QueryFailed)?,
            &mut returned_bytes,
        )
    };
    if status.0 == STATUS_ACCESS_DENIED {
        return Err(WindowsApiError::PermissionDenied);
    }
    if status.0 < 0 {
        return Err(WindowsApiError::QueryFailed);
    }
    let returned_bytes =
        usize::try_from(returned_bytes).map_err(|_| WindowsApiError::QueryFailed)?;
    let minimum_returned = basic_information_peb_field()
        .checked_add(POINTER_BYTES)
        .ok_or(WindowsApiError::QueryFailed)?;
    if returned_bytes < minimum_returned || returned_bytes > basic_information.len() {
        return Err(WindowsApiError::QueryFailed);
    }
    let peb_base = read_le_usize(&basic_information, basic_information_peb_field())?;
    if peb_base == 0 {
        // No PEB means no native environment source; an empty "healthy"
        // result would fabricate one.
        return Err(WindowsApiError::QueryFailed);
    }

    let mut peb_bytes = [0_u8; peb_process_parameters_field() + POINTER_BYTES];
    if read_target_bytes(process.0, peb_base, &mut peb_bytes)? != peb_bytes.len() {
        return Err(WindowsApiError::QueryFailed);
    }
    let parameters = read_le_usize(&peb_bytes, peb_process_parameters_field())?;
    if parameters == 0 {
        return Err(WindowsApiError::QueryFailed);
    }

    let mut parameters_bytes = vec![0_u8; params_environment_field() + POINTER_BYTES];
    if read_target_bytes(process.0, parameters, &mut parameters_bytes)? != parameters_bytes.len() {
        return Err(WindowsApiError::QueryFailed);
    }
    let fields = parse_process_parameters(&parameters_bytes)?;

    let working_directory =
        read_working_directory(process.0, fields.dos_path_bytes, fields.dos_path_pointer);

    let (block, terminated) = read_environment_block(process.0, fields.environment_pointer)?;
    if block.is_empty() && !terminated {
        // Nothing of the block could be read, so no entry count — not even
        // zero — is provable; a fabricated empty environment is worse than
        // the typed failure.
        return Err(WindowsApiError::QueryFailed);
    }
    let (entries, truncated_count) = parse_environment_block(&block, terminated);

    Ok(WindowsProcessEnvironmentBlock {
        working_directory,
        entries,
        truncated_count,
    })
}

/// Read the target's `CurrentDirectory.DosPath` string. Every guard failure —
/// null pointer, empty/odd/overlong length, or a short read — is an honest
/// `None` cwd, never a fabricated or truncated path.
#[cfg(windows)]
fn read_working_directory(
    process: windows::Win32::Foundation::HANDLE,
    dos_path_bytes: u16,
    dos_path_pointer: usize,
) -> Option<String> {
    let byte_len = usize::from(dos_path_bytes);
    if dos_path_pointer == 0 || byte_len == 0 || byte_len % 2 != 0 {
        return None;
    }
    if byte_len > MAX_WORKING_DIRECTORY_BYTES {
        return None;
    }
    let mut buffer = vec![0_u8; byte_len];
    let read = read_target_bytes(process, dos_path_pointer, &mut buffer).ok()?;
    if read != byte_len {
        return None;
    }
    let units = buffer
        .as_chunks::<2>()
        .0
        .iter()
        .map(|unit| u16::from_le_bytes(*unit))
        .collect::<Vec<_>>();
    Some(String::from_utf16_lossy(&units))
}

/// Read the UTF-16 environment block in bounded chunks until the
/// empty-string terminator is seen, the byte budget runs out, or the reads
/// stop succeeding. The second return value says whether the terminator was
/// proven; without it the caller treats the tail as clipped.
#[cfg(windows)]
fn read_environment_block(
    process: windows::Win32::Foundation::HANDLE,
    environment_pointer: usize,
) -> Result<(Vec<u8>, bool), WindowsApiError> {
    let mut block = Vec::new();
    if environment_pointer == 0 {
        // A null Environment pointer is the process's own fact: it has no
        // environment block, which is a provable empty, not a read failure.
        return Ok((block, true));
    }
    let mut address = environment_pointer;
    while block.len() < MAX_PROCESS_ENVIRONMENT_BYTES {
        let want = ENVIRONMENT_READ_CHUNK_BYTES.min(MAX_PROCESS_ENVIRONMENT_BYTES - block.len());
        let mut chunk = [0_u8; ENVIRONMENT_READ_CHUNK_BYTES];
        // A failed chunk means the walk reached past the block's committed
        // pages (or the block is unreadable); the bytes kept so far are the
        // honest bounded prefix, marked un-terminated so the parser drops a
        // clipped tail instead of trusting it.
        let Ok(read) = read_target_bytes(process, address, &mut chunk[..want]) else {
            return Ok((block, false));
        };
        if read == 0 {
            return Ok((block, false));
        }
        if read > want || !read.is_multiple_of(2) {
            return Err(WindowsApiError::QueryFailed);
        }
        block.extend_from_slice(&chunk[..read]);
        address = address
            .checked_add(read)
            .ok_or(WindowsApiError::QueryFailed)?;
        if ends_with_utf16_terminator(&block) {
            return Ok((block, true));
        }
    }
    Ok((block, false))
}

/// Read exactly `buffer.len()`-at-most bytes from the target's address space.
/// `Ok(n)` with `n < buffer.len()` is a short read; `Err` is a failed read.
#[cfg(windows)]
fn read_target_bytes(
    process: windows::Win32::Foundation::HANDLE,
    address: usize,
    buffer: &mut [u8],
) -> Result<usize, WindowsApiError> {
    use windows::Win32::System::Diagnostics::Debug::ReadProcessMemory;

    if buffer.is_empty() {
        return Ok(0);
    }
    let mut read = 0_usize;
    // SAFETY: `process` is an owned valid handle carrying PROCESS_VM_READ;
    // `address` came from the target's own PEB structures; `buffer` is a
    // writable allocation of exactly `buffer.len()` bytes outliving this
    // synchronous call, and `read` receives the kernel's byte count.
    let ok = unsafe {
        ReadProcessMemory(
            process,
            core::ptr::with_exposed_provenance(address),
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            Some(&mut read),
        )
    };
    if ok.is_err() {
        return Err(WindowsApiError::QueryFailed);
    }
    if read > buffer.len() {
        return Err(WindowsApiError::QueryFailed);
    }
    Ok(read)
}
