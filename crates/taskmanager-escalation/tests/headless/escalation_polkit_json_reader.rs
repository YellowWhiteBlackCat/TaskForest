//! Grammar-level coverage for the JSON reader. Contract-shape coverage
//! (SUCCESS/ERROR objects) lives in the parent `polkit` module's tests.
use super::*;

#[test]
fn parses_scalar_literals() {
    assert_eq!(JsonReader::parse("true"), Ok(Json::Bool(true)));
    assert_eq!(JsonReader::parse("false"), Ok(Json::Bool(false)));
    assert_eq!(JsonReader::parse("null"), Ok(Json::Null));
}

#[test]
fn parses_integers_and_floats_and_exponents() {
    assert!(matches!(JsonReader::parse("0"), Ok(Json::Number(n)) if n == 0.0));
    assert!(matches!(JsonReader::parse("42"), Ok(Json::Number(n)) if n == 42.0));
    assert!(matches!(JsonReader::parse("-7"), Ok(Json::Number(n)) if n == -7.0));
    assert!(matches!(JsonReader::parse("3.5"), Ok(Json::Number(n)) if n == 3.5));
    assert!(matches!(JsonReader::parse("1e3"), Ok(Json::Number(n)) if n == 1000.0));
}

#[test]
fn parses_nested_object_and_array() {
    let parsed = JsonReader::parse(r#"{"a":[1,2,{"b":true}]}"#).expect("valid json");
    let entry = parsed.get("a").and_then(Json::as_array).expect("array");
    assert_eq!(entry.len(), 3);
    assert!(entry[2].get("b").is_some());
}

#[test]
fn rejects_trailing_garbage_and_truncated_input() {
    assert!(JsonReader::parse("{}x").is_err());
    assert!(JsonReader::parse("{").is_err());
    assert!(JsonReader::parse("").is_err());
    assert!(JsonReader::parse("tru").is_err());
}

#[test]
fn decodes_unicode_escape_and_surrogate_pair() {
    // U+00E9 (é) as a single \u escape.
    let parsed = JsonReader::parse(r#""é""#).expect("valid");
    assert_eq!(parsed, Json::String("é".to_owned()));
    // U+1F600 (grinning face) as a surrogate pair.
    let parsed = JsonReader::parse(r#""😀""#).expect("valid pair");
    assert_eq!(parsed, Json::String("😀".to_owned()));
}

// --- nesting-depth bound (stack-overflow guard) ----------------------------

#[test]
fn deeply_nested_input_is_rejected_not_a_stack_abort() {
    // Thousands of levels used to recurse until the stack overflowed, which
    // aborts the whole process and no error type can catch.
    for deep in [
        format!("{}{}", "[".repeat(1000), "]".repeat(1000)),
        format!("{}{}", r#"{"a":"#.repeat(1000), "1}".repeat(1000)),
        format!("{}{}", r#"[{"a":"#.repeat(500), "1}]".repeat(500)),
    ] {
        assert!(
            JsonReader::parse(&deep).is_err(),
            "deeply nested input must fail closed, not recurse unbounded",
        );
    }
}

#[test]
fn shallow_nesting_still_parses() {
    let parsed = JsonReader::parse(r#"{"a":[1,2,{"b":true}]}"#).expect("3 levels are valid");
    assert!(parsed.get("a").is_some());
}

#[test]
fn the_nesting_limit_is_exactly_sixty_four_containers() {
    let at_limit = format!("{}{}", "[".repeat(64), "]".repeat(64));
    assert_eq!(JsonReader::parse(&at_limit).map(|_| ()), Ok(()));
    let over_limit = format!("{}{}", "[".repeat(65), "]".repeat(65));
    assert!(JsonReader::parse(&over_limit).is_err());
}
