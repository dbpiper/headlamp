use tempfile::NamedTempFile;

fn write_map_bytes(contents: &[u8]) -> std::path::PathBuf {
    let mut map_file = NamedTempFile::new().expect("tempfile");
    std::io::Write::write_all(&mut map_file, contents).expect("write map");
    map_file
        .into_temp_path()
        .keep()
        .expect("keep")
        .to_path_buf()
}

#[test]
fn parses_symbol_table_with_addresses_sizes_object_indexes_and_names() {
    let map_bytes = br#"
# Path: /tmp/headlamp
# Arch: arm64
# Object files:
[  0] linker synthesized
[  1] /tmp/libfoo-1234567890abcdef.rlib(foo-1234567890abcdef.foo.deadbeef-cgu.00.rcgu.o)
# Sections:
# Symbols:
0x0000000100001000 0x0000000000000010 [  1] __ZN3foo3bar17h0123456789abcdefE
"#;

    let map_path = write_map_bytes(map_bytes);
    let symbols = xtask::size_report::parse_map_symbols(&map_path).expect("parse symbols");
    assert_eq!(symbols.len(), 1);

    let first = &symbols[0];
    assert_eq!(first.address, 0x0000_0001_0000_1000);
    assert_eq!(first.size_bytes, 0x10);
    assert_eq!(first.object_index, 1);
    assert_eq!(first.raw_symbol_name, "__ZN3foo3bar17h0123456789abcdefE");
}

#[test]
fn parsing_tolerates_non_utf8_bytes_in_symbol_names() {
    let mut map_bytes: Vec<u8> = Vec::new();
    map_bytes.extend_from_slice(
        br#"
# Path: /tmp/headlamp
# Arch: arm64
# Object files:
[  0] linker synthesized
[  1] /tmp/libfoo-1234567890abcdef.rlib(foo-1234567890abcdef.foo.deadbeef-cgu.00.rcgu.o)
# Sections:
# Symbols:
0x0000000100001000 0x0000000000000001 [  1] _foo_"#,
    );
    map_bytes.push(0xFF);
    map_bytes.extend_from_slice(
        br#"
"#,
    );

    let map_path = write_map_bytes(&map_bytes);
    let symbols = xtask::size_report::parse_map_symbols(&map_path).expect("parse symbols");
    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].size_bytes, 1);
}
