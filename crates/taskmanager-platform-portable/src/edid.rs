//! Pure EDID base-block parsing shared by native display inventories.
//!
//! This module owns only the byte-level interpretation of an EDID blob: the
//! 8-byte header, the base-block checksum, the manufacturer code, string and
//! serial descriptors, the preferred detailed timing, and the CTA-861 HDR
//! Static Metadata Data Block. It performs no I/O, owns no connector identity,
//! and assigns no product meaning; adapters map the proven fields into their
//! own display models and keep their own connector naming. An EDID that fails
//! validation is a typed `None` — no field is ever guessed or defaulted.

/// Proven EDID facts for one display; every field is an honest absence when
/// the block does not advertise it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EdidFacts {
    /// Three-letter manufacturer code from bytes 8-9.
    pub manufacturer: Option<String>,
    /// Display-name descriptor text (`0xfc`).
    pub model: Option<String>,
    /// Serial-number descriptor text (`0xff`) or the numeric serial when
    /// nonzero.
    pub serial: Option<String>,
    /// Physical panel dimensions in millimetres from bytes 21-22.
    pub width_mm: Option<u32>,
    pub height_mm: Option<u32>,
    /// Preferred detailed timing dimensions in pixels.
    pub width_px: Option<u32>,
    pub height_px: Option<u32>,
    /// Preferred detailed timing refresh rate in Hz.
    pub refresh_hz: Option<f32>,
    /// `Some(true)` when a valid CTA-861 extension carries an HDR Static
    /// Metadata Data Block, `Some(false)` when a valid CTA extension proves
    /// its absence, `None` when no valid extension can prove either.
    pub hdr_supported: Option<bool>,
}

/// Parse the EDID base block and its extension blocks. Returns `None` when the
/// blob is shorter than one 128-byte block, the header or base checksum is
/// invalid, or the manufacturer code is not three letters.
pub fn parse_edid(edid: &[u8]) -> Option<EdidFacts> {
    let base = edid.get(..128)?;
    if base.get(..8)? != [0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0]
        || base.iter().fold(0_u8, |sum, byte| sum.wrapping_add(*byte)) != 0
    {
        return None;
    }

    let (width_px, height_px, refresh_hz) = preferred_timing(base).unwrap_or((None, None, None));
    Some(EdidFacts {
        manufacturer: decode_manufacturer(u16::from_be_bytes([base[8], base[9]])),
        serial: descriptor_text(base, 0xff).or_else(|| {
            let value = u32::from_le_bytes([base[12], base[13], base[14], base[15]]);
            (value != 0).then(|| value.to_string())
        }),
        model: descriptor_text(base, 0xfc),
        width_mm: (base[21] != 0).then_some(u32::from(base[21]) * 10),
        height_mm: (base[22] != 0).then_some(u32::from(base[22]) * 10),
        width_px,
        height_px,
        refresh_hz,
        hdr_supported: edid_hdr_support(edid),
    })
}

/// Read the CTA-861 extension's HDR Static Metadata Data Block. A valid CTA
/// extension without that block is a confirmed `false`; no extension remains
/// `None` because an older or truncated EDID cannot prove HDR capability.
fn edid_hdr_support(edid: &[u8]) -> Option<bool> {
    let extension_count = usize::from(*edid.get(126)?);
    if extension_count == 0 {
        return None;
    }
    let mut saw_cta = false;
    for index in 0..extension_count.min(32) {
        let start = 128 * (index + 1);
        let block = edid.get(start..start + 128)?;
        if block.iter().fold(0_u8, |sum, byte| sum.wrapping_add(*byte)) != 0 {
            continue;
        }
        if block[0] != 0x02 {
            continue;
        }
        saw_cta = true;
        let data_end = usize::from(block[2]);
        if !(4..=127).contains(&data_end) {
            continue;
        }
        let mut offset = 4;
        while offset < data_end {
            let header = block[offset];
            let tag = header >> 5;
            let length = usize::from(header & 0x1f);
            let Some(end) = offset.checked_add(1 + length) else {
                break;
            };
            if end > data_end {
                break;
            }
            // Extended Data Block, extended tag 0x06 = HDR Static Metadata.
            if tag == 0x07 && length >= 1 && block[offset + 1] == 0x06 {
                return Some(true);
            }
            offset = end;
        }
    }
    saw_cta.then_some(false)
}

fn decode_manufacturer(code: u16) -> Option<String> {
    let letters = [
        ((code >> 10) & 0x1f) as u8,
        ((code >> 5) & 0x1f) as u8,
        (code & 0x1f) as u8,
    ];
    letters
        .into_iter()
        .map(|letter| {
            (1..=26)
                .contains(&letter)
                .then(|| (b'A' + letter - 1) as char)
        })
        .collect::<Option<String>>()
}

fn descriptor_text(base: &[u8], tag: u8) -> Option<String> {
    (0..4).find_map(|index| {
        let start = 54 + index * 18;
        let block = base.get(start..start + 18)?;
        if block[0] != 0 || block[1] != 0 || block[2] != 0 || block[3] != tag {
            return None;
        }
        let end = block[5..18]
            .iter()
            .position(|byte| *byte == 0 || *byte == b'\n' || *byte == b'\r')
            .unwrap_or(13);
        let text = String::from_utf8_lossy(&block[5..5 + end])
            .trim()
            .to_owned();
        (!text.is_empty()).then_some(text)
    })
}

fn preferred_timing(base: &[u8]) -> Option<(Option<u32>, Option<u32>, Option<f32>)> {
    (0..4).find_map(|index| {
        let start = 54 + index * 18;
        let block = base.get(start..start + 18)?;
        let pixel_clock_10khz = u16::from_le_bytes([block[0], block[1]]);
        if pixel_clock_10khz == 0 {
            return None;
        }
        let width = u32::from(block[2]) | (u32::from(block[4] & 0xf0) << 4);
        let horizontal_blank = u32::from(block[3]) | (u32::from(block[4] & 0x0f) << 8);
        let height = u32::from(block[5]) | (u32::from(block[7] & 0xf0) << 4);
        let vertical_blank = u32::from(block[6]) | (u32::from(block[7] & 0x0f) << 8);
        let horizontal_total = width.saturating_add(horizontal_blank);
        let vertical_total = height.saturating_add(vertical_blank);
        let refresh_hz = (horizontal_total > 0 && vertical_total > 0).then(|| {
            let value = (f64::from(pixel_clock_10khz) * 10_000.0)
                / f64::from(horizontal_total)
                / f64::from(vertical_total);
            value as f32
        });
        Some((
            (width > 0).then_some(width),
            (height > 0).then_some(height),
            refresh_hz.filter(|value| value.is_finite() && *value > 0.0 && *value < 1_000.0),
        ))
    })
}
