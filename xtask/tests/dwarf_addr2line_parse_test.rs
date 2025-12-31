#[test]
fn parses_basic_addr2line_line_into_function_and_file() {
    let line = "foo::bar at /Users/me/src/headlamp/headlamp/src/main.rs:123";
    let parsed = xtask::dwarf_addr2line::parse_addr2line_output_line(line).expect("parse");
    assert_eq!(parsed.function_name, "foo::bar");
    assert_eq!(
        parsed.file_path.as_deref(),
        Some("/Users/me/src/headlamp/headlamp/src/main.rs")
    );
    assert_eq!(parsed.line_number, Some(123));
}

#[test]
fn parses_unknown_location() {
    let line = "foo::bar at ??:0";
    let parsed = xtask::dwarf_addr2line::parse_addr2line_output_line(line).expect("parse");
    assert_eq!(parsed.function_name, "foo::bar");
    assert_eq!(parsed.file_path, None);
    assert_eq!(parsed.line_number, None);
}

#[test]
fn resolve_locations_fast_path_for_empty_input() {
    let resolved = xtask::dwarf_addr2line::resolve_locations_with_llvm_addr2line(
        std::path::Path::new("/tmp/does_not_matter"),
        &[],
    )
    .expect("empty should succeed");
    assert!(resolved.is_empty());
}
