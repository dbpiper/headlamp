use std::time::{Duration, Instant};

use tempfile::NamedTempFile;

fn write_map(contents: &str) -> std::path::PathBuf {
    let mut map_file = NamedTempFile::new().expect("tempfile");
    std::io::Write::write_all(&mut map_file, contents.as_bytes()).expect("write map");
    map_file
        .into_temp_path()
        .keep()
        .expect("keep")
        .to_path_buf()
}

#[test]
fn parses_object_table_and_symbols_and_rolls_up_to_crates() {
    let map_contents = r#"
# Path: /tmp/headlamp
# Arch: arm64
# Object files:
[  0] linker synthesized
[  1] /tmp/libfoo-1234567890abcdef.rlib(foo-1234567890abcdef.foo.deadbeef-cgu.00.rcgu.o)
[  2] /tmp/libbar-1234567890abcdef.rlib(bar-1234567890abcdef.bar.deadbeef-cgu.00.rcgu.o)
[  3] /Users/me/.rustup/toolchains/stable-aarch64-apple-darwin/lib/rustlib/aarch64-apple-darwin/lib/libstd-aaaaaaaaaaaaaaaa.rlib(std-aaaaaaaaaaaaaaaa.std.bbbbbbbb-cgu.0.rcgu.o)
# Sections:
# Symbols:
0x0000000100001000 0x0000000000000010 [  1] _foo_one
0x0000000100001010 0x0000000000000020 [  1] _foo_two
0x0000000100001030 0x0000000000000008 [  2] _bar_one
0x0000000100001040 0x0000000000000004 [  3] _std_one
"#;

    let map_path = write_map(map_contents);
    let report = xtask::size_report::parse_map_file(&map_path).expect("parse");

    let foo_bytes = report
        .crate_sizes
        .iter()
        .find(|row| row.crate_name == "foo")
        .map(|row| row.bytes)
        .unwrap();
    let bar_bytes = report
        .crate_sizes
        .iter()
        .find(|row| row.crate_name == "bar")
        .map(|row| row.bytes)
        .unwrap();
    let std_bytes = report
        .crate_sizes
        .iter()
        .find(|row| row.crate_name == "rust-stdlib")
        .map(|row| row.bytes)
        .unwrap();

    assert_eq!(foo_bytes, 0x10 + 0x20);
    assert_eq!(bar_bytes, 0x08);
    assert_eq!(std_bytes, 0x04);
}

#[test]
fn parsing_is_linearish_and_fast_enough_for_large_maps() {
    let mut map_contents = String::from(
        r#"
# Path: /tmp/headlamp
# Arch: arm64
# Object files:
[  0] linker synthesized
[  1] /tmp/libfoo-1234567890abcdef.rlib(foo-1234567890abcdef.foo.deadbeef-cgu.00.rcgu.o)
# Sections:
# Symbols:
"#,
    );

    let symbol_line_count: usize = 50_000;
    for index in 0..symbol_line_count {
        let address = 0x0000_0001_0000_1000u64 + (index as u64) * 0x10;
        map_contents.push_str(&format!(
            "0x{address:016x}\t0x0000000000000001\t[  1] _foo_{index}\n"
        ));
    }

    let map_path = write_map(&map_contents);
    let start = Instant::now();
    let report = xtask::size_report::parse_map_file(&map_path).expect("parse");
    let elapsed = start.elapsed();

    let foo_bytes = report
        .crate_sizes
        .iter()
        .find(|row| row.crate_name == "foo")
        .map(|row| row.bytes)
        .unwrap();
    assert_eq!(foo_bytes, symbol_line_count as u64);

    let budget = if cfg!(debug_assertions) {
        Duration::from_secs(2)
    } else {
        Duration::from_millis(250)
    };
    assert!(
        elapsed <= budget,
        "parse took {elapsed:?}, expected <= {budget:?}"
    );
}
