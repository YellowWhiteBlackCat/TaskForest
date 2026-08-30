use super::*;

/// Test-local raw type-17 fixture builder, shaped exactly like a
/// kernel-exported `/sys/firmware/dmi/entries/17-N/raw` file: a `length`-byte
/// formatted area followed by a string set.
struct Fixture {
    bytes: Vec<u8>,
}

impl Fixture {
    /// A type-17 formatted area of `length` bytes (no string set yet).
    fn formatted(length: usize) -> Self {
        let mut bytes = vec![0u8; length];
        bytes[0] = 17;
        bytes[1] = length as u8;
        Fixture { bytes }
    }

    /// Append a NUL-separated, double-NUL-terminated string set.
    fn string_set(mut self, strings: &[&str]) -> Self {
        for entry in strings {
            self.bytes.extend_from_slice(entry.as_bytes());
            self.bytes.push(0);
        }
        self.bytes.push(0);
        self
    }

    /// Append raw trailing bytes (truncated sets, post-terminator garbage).
    fn trailing(mut self, bytes: &[u8]) -> Self {
        self.bytes.extend_from_slice(bytes);
        self
    }

    /// Formatted area + string set in one step.
    fn record(length: usize, strings: &[&str]) -> Self {
        Fixture::formatted(length).string_set(strings)
    }

    fn size(mut self, value: u16) -> Self {
        self.bytes[12..14].copy_from_slice(&value.to_le_bytes());
        self
    }

    fn form_factor(mut self, code: u8) -> Self {
        self.bytes[14] = code;
        self
    }

    /// Point the device-locator string field (offset 15) at string `index`.
    fn locator(mut self, index: u8) -> Self {
        self.bytes[15] = index;
        self
    }

    fn memory_type(mut self, code: u8) -> Self {
        self.bytes[18] = code;
        self
    }

    fn speed(mut self, value: u16) -> Self {
        self.bytes[21..23].copy_from_slice(&value.to_le_bytes());
        self
    }

    fn manufacturer(mut self, index: u8) -> Self {
        self.bytes[23] = index;
        self
    }

    fn serial(mut self, index: u8) -> Self {
        self.bytes[24] = index;
        self
    }

    fn part_number(mut self, index: u8) -> Self {
        self.bytes[26] = index;
        self
    }

    fn configured_speed(mut self, value: u16) -> Self {
        self.bytes[32..34].copy_from_slice(&value.to_le_bytes());
        self
    }

    fn build(self) -> Vec<u8> {
        self.bytes
    }
}

/// A real-shaped SMBIOS 3.x record: every field present and populated.
fn populated_fixture() -> Fixture {
    Fixture::record(
        34,
        &["ChannelA-DIMM0", "Crucial", "SER4242X0", "CT16G48C40S5"],
    )
    .size(16384)
    .form_factor(0x0D)
    .locator(1)
    .memory_type(0x22)
    .speed(5600)
    .manufacturer(2)
    .serial(3)
    .part_number(4)
    .configured_speed(4800)
}

#[test]
fn full_record_parses_every_field() {
    let record = parse_memory_device(&populated_fixture().build()).expect("valid type-17 record");
    assert_eq!(record.size_mb, Some(16384));
    assert_eq!(record.speed_mts, Some(5600));
    assert_eq!(record.configured_speed_mts, Some(4800));
    assert_eq!(record.form_factor, Some("SODIMM"));
    assert_eq!(record.memory_type, Some("DDR5"));
    assert_eq!(record.manufacturer.as_deref(), Some("Crucial"));
    assert_eq!(record.serial_number.as_deref(), Some("SER4242X0"));
    assert_eq!(record.part_number.as_deref(), Some("CT16G48C40S5"));
    assert_eq!(record.device_locator.as_deref(), Some("ChannelA-DIMM0"));
}

#[test]
fn firmware_space_padding_is_trimmed_from_string_facts() {
    // Captured on-box: firmware pads part numbers with trailing spaces (a
    // real receipt carried "M561K2LC4EE1-CCUYD" + 12 spaces). The padding is
    // transport noise; the one authority for the format trims it so every
    // consumer sees the same clean fact. Leading spaces are content and stay.
    let record = parse_memory_device(
        &Fixture::record(
            34,
            &["ChannelA-DIMM0", "Samsung    ", "SN1", "Padded-Part   "],
        )
        .manufacturer(2)
        .serial(3)
        .part_number(4)
        .locator(1)
        .build(),
    )
    .expect("valid type-17 record");
    assert_eq!(record.manufacturer.as_deref(), Some("Samsung"));
    assert_eq!(record.part_number.as_deref(), Some("Padded-Part"));
    assert_eq!(record.device_locator.as_deref(), Some("ChannelA-DIMM0"));
}

#[test]
fn a_whitespace_only_string_fact_stays_absent() {
    // After trimming, a space-padded empty string must not surface as a
    // present-but-blank fact.
    let record = parse_memory_device(&Fixture::record(23, &["   "]).manufacturer(1).build())
        .expect("valid type-17 record");
    assert_eq!(record.manufacturer, None);
}

#[test]
fn unknown_and_absent_size_words_are_none() {
    for size in [0u16, 0x7FFF] {
        let record =
            parse_memory_device(&Fixture::record(21, &[]).size(size).build()).expect("record");
        assert_eq!(record.size_mb, None, "size {size:#06x} must be unknown");
    }
}

#[test]
fn kb_unit_size_converts_to_mb_with_ceiling() {
    // 2048 KB = exactly 2 MB.
    let exact =
        parse_memory_device(&Fixture::record(21, &[]).size(0x8000 | 2048).build()).expect("record");
    assert_eq!(exact.size_mb, Some(2));
    // 1 KB rounds up to 1 MB.
    let ceil =
        parse_memory_device(&Fixture::record(21, &[]).size(0x8000 | 1).build()).expect("record");
    assert_eq!(ceil.size_mb, Some(1));
    // KB magnitude 0 (the unit bit alone) stays an honest 0 MB.
    let zero = parse_memory_device(&Fixture::record(21, &[]).size(0x8000).build()).expect("record");
    assert_eq!(zero.size_mb, Some(0));
}

#[test]
fn speed_sentinels_are_none() {
    for speed in [0u16, 0xFFFF] {
        let record =
            parse_memory_device(&Fixture::record(23, &[]).speed(speed).build()).expect("record");
        assert_eq!(record.speed_mts, None, "speed {speed:#06x} must be unknown");
    }
}

#[test]
fn speed_is_absent_when_length_is_below_23() {
    // Length 21: bytes 21.. hold the string set, not the speed word. A
    // non-sentinel word there must not leak through as a speed value.
    let record = parse_memory_device(&Fixture::formatted(21).trailing(&[0xDC, 0x15, 0, 0]).build())
        .expect("record");
    assert_eq!(record.speed_mts, None);
}

#[test]
fn configured_speed_tracks_the_smbios_26_length_gate() {
    // Length 34 (SMBIOS 2.6+): configured speed decodes.
    let present = parse_memory_device(&populated_fixture().build()).expect("record");
    assert_eq!(present.configured_speed_mts, Some(4800));
    // Length 33: bytes 32.. hold the string set, not the configured word.
    let absent = parse_memory_device(&Fixture::formatted(33).trailing(&[0xC0, 0x12, 0, 0]).build())
        .expect("record");
    assert_eq!(absent.configured_speed_mts, None);
}

#[test]
fn unmapped_and_zero_enum_bytes_are_none() {
    for (form_factor, memory_type) in [(0x00u8, 0x00u8), (0x30, 0x40)] {
        let record = parse_memory_device(
            &Fixture::record(21, &[])
                .form_factor(form_factor)
                .memory_type(memory_type)
                .build(),
        )
        .expect("record");
        assert_eq!(record.form_factor, None, "form factor {form_factor:#04x}");
        assert_eq!(record.memory_type, None, "memory type {memory_type:#04x}");
    }
}

#[test]
fn known_enum_labels_decode() {
    let record = parse_memory_device(
        &Fixture::record(21, &[])
            .form_factor(0x08)
            .memory_type(0x1A)
            .build(),
    )
    .expect("record");
    assert_eq!(record.form_factor, Some("DIMM"));
    assert_eq!(record.memory_type, Some("DDR4"));
    let lpddr5 =
        parse_memory_device(&Fixture::record(21, &[]).memory_type(0x23).build()).expect("record");
    assert_eq!(lpddr5.memory_type, Some("LPDDR5"));
}

#[test]
fn empty_string_index_yields_none_but_neighbors_decode() {
    // String 1 is empty, string 2 carries the value.
    let record = parse_memory_device(
        &Fixture::record(34, &["", "Micron"])
            .manufacturer(1)
            .part_number(2)
            .build(),
    )
    .expect("record");
    assert_eq!(record.manufacturer, None);
    assert_eq!(record.part_number.as_deref(), Some("Micron"));
}

#[test]
fn index_zero_and_beyond_the_set_are_none() {
    let record = parse_memory_device(
        &Fixture::record(34, &["A", "B"])
            .manufacturer(0)
            .serial(9)
            .build(),
    )
    .expect("record");
    assert_eq!(record.manufacturer, None, "index 0 means no string");
    assert_eq!(record.serial_number, None, "index beyond the set");
}

#[test]
fn trailing_bytes_after_the_double_nul_are_not_strings() {
    // The set ends at the double NUL; bytes after it stay outside the record
    // even though they are inside the raw slice. String 1 still decodes.
    let record = parse_memory_device(
        &Fixture::record(34, &["A"])
            .locator(1)
            .manufacturer(2)
            .trailing(b"Garbage\0")
            .build(),
    )
    .expect("record");
    assert_eq!(record.device_locator.as_deref(), Some("A"));
    assert_eq!(record.manufacturer, None, "index 2 is past the terminator");
}

#[test]
fn truncated_string_set_decodes_what_terminated() {
    // "Foo" with its closing NUL but no double-NUL terminator (file truncated
    // right after the last string's NUL): string 1 decodes, nothing further.
    let bytes = Fixture::formatted(24)
        .manufacturer(1)
        .trailing(b"Foo\0")
        .build();
    let record = parse_memory_device(&bytes).expect("record");
    assert_eq!(record.manufacturer.as_deref(), Some("Foo"));
    let truncated_two = Fixture::formatted(24)
        .manufacturer(2)
        .trailing(b"Foo\0")
        .build();
    let beyond = parse_memory_device(&truncated_two).expect("record");
    assert_eq!(beyond.manufacturer, None, "no second string exists");
}

#[test]
fn string_fields_missing_from_a_short_record_are_none() {
    // Manufacturer/serial/part live at offsets 23/24/26 — beyond a length-21
    // formatted area, so those bytes are string-set content here. The set is
    // shaped so each of those bytes IS a small valid string index (1); only
    // the length gate keeps them from being read as fields. The device
    // locator (offset 15, inside the area) still decodes.
    let record = parse_memory_device(
        &Fixture::formatted(21)
            .locator(1)
            .trailing(&[b'L', 0, 1, 1, 0, 1, 0, 0])
            .build(),
    )
    .expect("record");
    assert_eq!(record.device_locator.as_deref(), Some("L"));
    assert_eq!(record.manufacturer, None);
    assert_eq!(record.serial_number, None);
    assert_eq!(record.part_number, None);
}

#[test]
fn non_utf8_string_decodes_lossily() {
    let bytes = Fixture::formatted(24)
        .manufacturer(1)
        .trailing(&[0xFF, 0xFE, 0x00, 0x00])
        .build();
    let record = parse_memory_device(&bytes).expect("record");
    let decoded = record.manufacturer.expect("string present");
    assert!(
        decoded.contains('\u{FFFD}'),
        "lossy decode keeps the bytes honest: {decoded:?}"
    );
}

#[test]
fn wrong_type_byte_is_not_a_memory_device() {
    let mut bytes = populated_fixture().build();
    bytes[0] = 4;
    assert_eq!(parse_memory_device(&bytes), None);
}

#[test]
fn malformed_length_is_rejected() {
    // Declared length below the type-17 minimum.
    let mut short = populated_fixture().build();
    short[1] = 20;
    assert_eq!(parse_memory_device(&short), None);
    // Declared length beyond the raw slice.
    let mut overlong = populated_fixture().build();
    overlong[1] = 0x60;
    assert_eq!(parse_memory_device(&overlong), None);
    // Empty slice.
    assert_eq!(parse_memory_device(&[]), None);
}

/// Generic raw fixture for the identity record types: a `length`-byte
/// formatted area with the type byte preset, plus planters for string indexes
/// and raw bytes, then the same string-set/trailing helpers as [`Fixture`].
struct RawFixture {
    bytes: Vec<u8>,
}

impl RawFixture {
    /// A formatted area of `length` bytes with type byte `kind` (no strings).
    fn typed(kind: u8, length: usize) -> Self {
        let mut bytes = vec![0u8; length];
        bytes[0] = kind;
        bytes[1] = length as u8;
        RawFixture { bytes }
    }

    /// Append a NUL-separated, double-NUL-terminated string set.
    fn string_set(mut self, strings: &[&str]) -> Self {
        for entry in strings {
            self.bytes.extend_from_slice(entry.as_bytes());
            self.bytes.push(0);
        }
        self.bytes.push(0);
        self
    }

    /// Append raw trailing bytes (truncated sets, post-terminator garbage).
    fn trailing(mut self, bytes: &[u8]) -> Self {
        self.bytes.extend_from_slice(bytes);
        self
    }

    /// Point the string field at `offset` at string `index`.
    fn str_at(mut self, offset: usize, index: u8) -> Self {
        self.bytes[offset] = index;
        self
    }

    /// Plant raw bytes at `offset` (e.g. the 16 UUID bytes at 0x08).
    fn bytes_at(mut self, offset: usize, bytes: &[u8]) -> Self {
        self.bytes[offset..offset + bytes.len()].copy_from_slice(bytes);
        self
    }

    fn build(self) -> Vec<u8> {
        self.bytes
    }
}

/// Wire bytes of a classic on-box type-1 UUID whose canonical rendering is
/// `4c4c4544-0042-3510-8054-b7c04f4d3532` (first three fields little-endian).
const UUID_WIRE: [u8; 16] = [
    0x44, 0x45, 0x4C, 0x4C, 0x42, 0x00, 0x10, 0x35, 0x80, 0x54, 0xB7, 0xC0, 0x4F, 0x4D, 0x35, 0x32,
];
const UUID_CANONICAL: &str = "4c4c4544-0042-3510-8054-b7c04f4d3532";

#[test]
fn bios_information_decodes_vendor_version_and_date() {
    let bytes = RawFixture::typed(0, 0x12)
        .str_at(0x04, 1)
        .str_at(0x05, 2)
        .str_at(0x08, 3)
        .string_set(&["AMI", "1.27.0", "04/17/2024"])
        .build();
    let record = parse_bios_information(&bytes).expect("valid type-0 record");
    assert_eq!(record.vendor.as_deref(), Some("AMI"));
    assert_eq!(record.version.as_deref(), Some("1.27.0"));
    assert_eq!(record.release_date.as_deref(), Some("04/17/2024"));
}

#[test]
fn bios_information_below_the_minimum_length_is_rejected() {
    // dmidecode gates type 0 at length 0x12; a shorter area is malformed.
    let bytes = RawFixture::typed(0, 0x11)
        .str_at(0x04, 1)
        .string_set(&["AMI"])
        .build();
    assert_eq!(parse_bios_information(&bytes), None);
    // Empty slice and a beyond-raw declared length are also not records.
    assert_eq!(parse_bios_information(&[]), None);
    let mut overlong = RawFixture::typed(0, 0x12).string_set(&["AMI"]).build();
    overlong[1] = 0x30;
    assert_eq!(parse_bios_information(&overlong), None);
}

#[test]
fn bios_information_absent_strings_are_none() {
    // Index 0 (no string) for vendor, index 2 beyond the one-string set for
    // the date; the version (string 1) still decodes.
    let bytes = RawFixture::typed(0, 0x12)
        .str_at(0x04, 0)
        .str_at(0x05, 1)
        .str_at(0x08, 2)
        .string_set(&["1.27.0"])
        .build();
    let record = parse_bios_information(&bytes).expect("valid type-0 record");
    assert_eq!(record.vendor, None);
    assert_eq!(record.version.as_deref(), Some("1.27.0"));
    assert_eq!(record.release_date, None);
}

#[test]
fn system_information_decodes_every_field() {
    let bytes = RawFixture::typed(1, 0x1B)
        .str_at(0x04, 1)
        .str_at(0x05, 2)
        .str_at(0x06, 3)
        .str_at(0x07, 4)
        .bytes_at(0x08, &UUID_WIRE)
        .str_at(0x19, 5)
        .str_at(0x1A, 6)
        .string_set(&[
            "LENOVO",
            "21JX",
            "ThinkPad P16s",
            "PF3XYZ42",
            "SKU-AB",
            "ThinkPad",
        ])
        .build();
    let record = parse_system_information(&bytes).expect("valid type-1 record");
    assert_eq!(record.manufacturer.as_deref(), Some("LENOVO"));
    assert_eq!(record.product_name.as_deref(), Some("21JX"));
    assert_eq!(record.version.as_deref(), Some("ThinkPad P16s"));
    assert_eq!(record.serial_number.as_deref(), Some("PF3XYZ42"));
    assert_eq!(record.uuid.as_deref(), Some(UUID_CANONICAL));
    assert_eq!(record.sku.as_deref(), Some("SKU-AB"));
    assert_eq!(record.family.as_deref(), Some("ThinkPad"));
}

#[test]
fn system_uuid_swaps_only_the_first_three_fields() {
    // The first three fields (8 bytes) render byte-swapped; the final 8 bytes
    // stay in wire order — exactly dmidecode's `p[3]p[2]p[1]p[0]-p[5]p[4]-
    // p[7]p[6]-p[8]p[9]-p[10..15]` layout.
    let bytes = RawFixture::typed(1, 0x19)
        .bytes_at(
            0x08,
            &[
                0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xAA, 0xBB, 0xCC, 0xDD,
                0xEE, 0xFF,
            ],
        )
        .string_set(&[])
        .build();
    let record = parse_system_information(&bytes).expect("valid type-1 record");
    assert_eq!(
        record.uuid.as_deref(),
        Some("33221100-5544-7766-8899-aabbccddeeff")
    );
}

#[test]
fn system_uuid_sentinel_patterns_are_none() {
    for pattern in [[0u8; 16], [0xFF; 16]] {
        let bytes = RawFixture::typed(1, 0x19)
            .bytes_at(0x08, &pattern)
            .string_set(&[])
            .build();
        let record = parse_system_information(&bytes).expect("valid type-1 record");
        assert_eq!(record.uuid, None, "{pattern:?} is not a present UUID");
    }
}

#[test]
fn system_information_length_gates_uuid_sku_and_family() {
    // Length 0x18: bytes 0x08.. are the string set. A fake 16-byte "UUID"
    // planted there must not leak; the four base strings still decode.
    let short = RawFixture::typed(1, 0x08)
        .str_at(0x04, 1)
        .str_at(0x05, 2)
        .str_at(0x06, 3)
        .str_at(0x07, 4)
        .string_set(&["A", "B", "C", "D"])
        .build();
    let record = parse_system_information(&short).expect("valid type-1 record");
    assert_eq!(record.manufacturer.as_deref(), Some("A"));
    assert_eq!(record.uuid, None);
    assert_eq!(record.sku, None);
    assert_eq!(record.family, None);
    // Length 0x1A: the UUID exists, SKU (0x19 < 0x1A) exists when the index
    // points into the set, family (0x1A >= 0x1A) does not — there is no byte
    // 0x1A inside a length-0x1A formatted area at all.
    let middle = RawFixture::typed(1, 0x1A)
        .bytes_at(0x08, &UUID_WIRE)
        .str_at(0x19, 1)
        .string_set(&["SKU-AB"])
        .build();
    let record = parse_system_information(&middle).expect("valid type-1 record");
    assert_eq!(record.uuid.as_deref(), Some(UUID_CANONICAL));
    assert_eq!(record.sku.as_deref(), Some("SKU-AB"));
    assert_eq!(record.family, None, "offset 0x1A lies at the length gate");
}

#[test]
fn system_information_wrong_shape_is_not_a_record() {
    // Wrong type byte (a type-17 fixture), length below the 0x08 minimum.
    assert_eq!(parse_system_information(&populated_fixture().build()), None);
    assert_eq!(
        parse_system_information(&RawFixture::typed(1, 0x07).string_set(&[]).build()),
        None
    );
    assert_eq!(parse_system_information(&[]), None);
}

#[test]
fn baseboard_information_decodes_every_field() {
    let bytes = RawFixture::typed(2, 0x0B)
        .str_at(0x04, 1)
        .str_at(0x05, 2)
        .str_at(0x06, 3)
        .str_at(0x07, 4)
        .str_at(0x08, 5)
        .str_at(0x0A, 6)
        .string_set(&[
            "ASUSTeK",
            "PROART X670E",
            "Rev 1.02",
            "MB-SN-1234",
            "ASSET-42",
            "Base Board",
        ])
        .build();
    let record = parse_baseboard_information(&bytes).expect("valid type-2 record");
    assert_eq!(record.manufacturer.as_deref(), Some("ASUSTeK"));
    assert_eq!(record.product_name.as_deref(), Some("PROART X670E"));
    assert_eq!(record.version.as_deref(), Some("Rev 1.02"));
    assert_eq!(record.serial_number.as_deref(), Some("MB-SN-1234"));
    assert_eq!(record.asset_tag.as_deref(), Some("ASSET-42"));
    assert_eq!(record.location_in_chassis.as_deref(), Some("Base Board"));
}

#[test]
fn baseboard_information_length_gates_asset_tag_and_location() {
    // Length 0x08: the header only. The trailing bytes plant valid-looking
    // string indexes AT offsets 0x08 and 0x0A (byte 8 = 1, byte 10 = 1) and
    // a decodable string 1 ("\x01\x09\x01") — only the length gate keeps
    // those bytes from being read as the asset tag / location fields.
    let header_only = RawFixture::typed(2, 0x08)
        .trailing(&[1, 0x09, 1, 0, 0])
        .build();
    let record = parse_baseboard_information(&header_only).expect("valid type-2 record");
    assert_eq!(record.asset_tag, None);
    assert_eq!(record.location_in_chassis, None);
    // Length 0x09: asset tag decodes, location (offset 0x0A beyond the area)
    // does not.
    let with_tag = RawFixture::typed(2, 0x09)
        .str_at(0x08, 1)
        .string_set(&["ASSET-42"])
        .build();
    let record = parse_baseboard_information(&with_tag).expect("valid type-2 record");
    assert_eq!(record.asset_tag.as_deref(), Some("ASSET-42"));
    assert_eq!(record.location_in_chassis, None);
}

#[test]
fn baseboard_information_index_beyond_the_set_is_none() {
    let bytes = RawFixture::typed(2, 0x0B)
        .str_at(0x07, 9)
        .string_set(&["ASUSTeK"])
        .build();
    let record = parse_baseboard_information(&bytes).expect("valid type-2 record");
    assert_eq!(record.serial_number, None);
    assert_eq!(record.manufacturer, None, "index 0 means no string");
}

#[test]
fn baseboard_truncated_string_set_decodes_what_terminated() {
    // Serial (string 1) decodes from a set truncated after its NUL; the asset
    // tag (string 2) does not exist.
    let bytes = RawFixture::typed(2, 0x09)
        .str_at(0x07, 1)
        .str_at(0x08, 2)
        .trailing(b"MB-SN-1234\0")
        .build();
    let record = parse_baseboard_information(&bytes).expect("valid type-2 record");
    assert_eq!(record.serial_number.as_deref(), Some("MB-SN-1234"));
    assert_eq!(record.asset_tag, None);
}

#[test]
fn baseboard_wrong_shape_is_not_a_record() {
    assert_eq!(
        parse_baseboard_information(&populated_fixture().build()),
        None
    );
    assert_eq!(
        parse_baseboard_information(&RawFixture::typed(2, 0x07).string_set(&[]).build()),
        None
    );
    assert_eq!(parse_baseboard_information(&[]), None);
}
