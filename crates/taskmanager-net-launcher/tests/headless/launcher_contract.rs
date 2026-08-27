use super::*;

#[test]
fn error_kinds_and_exit_codes_are_distinct_and_stable() {
    // The consumer parses these exit codes; they must be stable + distinct.
    let kinds = [
        LaunchError::ArgError,
        LaunchError::ConnectFailed,
        LaunchError::PermissionDenied,
        LaunchError::OpenFailed,
        LaunchError::SendFailed,
        LaunchError::AckFailed,
    ];
    let codes: Vec<u8> = kinds.iter().map(LaunchError::exit_code).collect();
    let named: Vec<&str> = kinds.iter().map(LaunchError::kind).collect();
    assert_eq!(
        codes.len(),
        codes.iter().collect::<std::collections::HashSet<_>>().len(),
        "exit codes must be distinct"
    );
    assert_eq!(
        named.len(),
        named.iter().collect::<std::collections::HashSet<_>>().len(),
        "kind names must be distinct"
    );
    assert_eq!(
        LaunchError::PermissionDenied.exit_code(),
        2,
        "permission_denied pins exit 2 (matches privilege-helper)"
    );
}

#[test]
fn decode_hex_accepts_the_handoff_name_shape() {
    // The app sends `tm-netl-` + 16 urandom bytes, hex-encoded: 24 bytes →
    // 48 hex chars. Decoding must reproduce the exact name bytes (it is the
    // connect address — one flipped bit means connecting to nothing or to the
    // wrong abstract socket).
    let name: Vec<u8> = [b"tm-netl-".as_slice(), [0x00, 0xff, 0x10, 0xab].as_slice()].concat();
    let hex: String = name.iter().map(|byte| format!("{byte:02x}")).collect();
    assert_eq!(decode_hex(&hex).expect("valid hex decodes"), name);
    assert_eq!(decode_hex("00ff").expect("pairs decode"), vec![0x00, 0xff]);
}

#[test]
fn decode_hex_rejects_malformed_names_fail_closed() {
    // Empty, odd-length, and non-hex input must all be rejected: the launcher
    // would otherwise connect to a guessed address.
    assert_eq!(decode_hex(""), None);
    assert_eq!(decode_hex("0"), None);
    assert_eq!(decode_hex("abc"), None);
    assert_eq!(decode_hex("zz"), None);
    assert_eq!(decode_hex("0g"), None);
}
