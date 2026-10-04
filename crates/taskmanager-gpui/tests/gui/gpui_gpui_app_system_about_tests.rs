use super::*;

#[test]
fn display_value_keeps_a_prefix_and_ellipsis_for_long_provider_text() {
    let value = "(linux-cachyos@cachyos) (clang version 22.1.8, LLD 22.1.8) #1 SMP";
    let displayed = display_value(value);
    assert!(displayed.starts_with("(linux-cachyos@cachyos)"));
    assert!(displayed.ends_with('…'));
    assert!(displayed.chars().count() <= 36);
}
