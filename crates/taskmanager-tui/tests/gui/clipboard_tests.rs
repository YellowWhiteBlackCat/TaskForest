use super::*;

#[test]
fn base64_encoder_matches_the_rfc_4648_vectors() {
    assert_eq!(base64_encode(b""), "");
    assert_eq!(base64_encode(b"f"), "Zg==");
    assert_eq!(base64_encode(b"fo"), "Zm8=");
    assert_eq!(base64_encode(b"foo"), "Zm9v");
    assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
    assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
    assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
}

#[test]
fn osc52_write_emits_the_prefix_payload_and_terminator() {
    let mut sink = Vec::new();
    write_clipboard(&mut sink, "123\tmy-process").expect("clipboard write");
    let bytes = String::from_utf8(sink).expect("valid utf8");
    assert!(bytes.starts_with("\x1b]52;c;"), "OSC52 prefix");
    assert!(bytes.ends_with('\u{7}'), "BEL terminator");
    let payload = bytes
        .trim_start_matches("\x1b]52;c;")
        .trim_end_matches('\u{7}');
    assert_eq!(payload, base64_encode(b"123\tmy-process"));
}
