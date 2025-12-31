use treemap_viewer::analysis::{
    build_treemap_from_symbols_and_locations, crate_name_from_function_name,
    crate_name_from_function_name_and_file_path, fill_missing_symbol_sizes,
};
use treemap_viewer::model::{ResolvedLocation, SymbolRecord};

#[test]
fn fill_missing_symbol_sizes_uses_next_symbol_delta() {
    let mut records = vec![
        SymbolRecord {
            address: 100,
            size_bytes: 0,
            raw_symbol_name: "a".to_string(),
        },
        SymbolRecord {
            address: 140,
            size_bytes: 12,
            raw_symbol_name: "b".to_string(),
        },
    ];

    fill_missing_symbol_sizes(&mut records);
    assert_eq!(records[0].size_bytes, 40);
    assert_eq!(records[1].size_bytes, 12);
}

#[test]
fn crate_name_from_function_name_strips_angle_bracket() {
    assert_eq!(
        crate_name_from_function_name("<core::fmt::Arguments"),
        "core"
    );
    assert_eq!(crate_name_from_function_name("alloc::vec::Vec"), "alloc");
    assert_eq!(crate_name_from_function_name(""), "unknown");
}

#[test]
fn crate_name_falls_back_to_file_path_for_anon_like_symbols() {
    assert_eq!(
        crate_name_from_function_name_and_file_path(
            "_anon.f4568efcf1497c0b2190af8bb2d487d3.4",
            Some(
                "/Users/me/.cargo/registry/src/index.crates.io-abc123/serde_json-1.0.134/src/lib.rs"
            ),
        ),
        "serde_json"
    );
}

#[test]
fn build_treemap_groups_by_crate_file_function() {
    let symbols = vec![
        SymbolRecord {
            address: 10,
            size_bytes: 5,
            raw_symbol_name: "sym1".to_string(),
        },
        SymbolRecord {
            address: 20,
            size_bytes: 7,
            raw_symbol_name: "sym2".to_string(),
        },
    ];
    let locations = vec![
        ResolvedLocation {
            function_name: "crate_a::mod1::f".to_string(),
            file_path: Some("src/mod1.rs".to_string()),
            line_number: Some(1),
        },
        ResolvedLocation {
            function_name: "crate_a::mod1::f".to_string(),
            file_path: Some("src/mod1.rs".to_string()),
            line_number: Some(2),
        },
    ];

    let tree = build_treemap_from_symbols_and_locations(&symbols, &locations).expect("tree");
    assert_eq!(tree.name, "root");
    assert_eq!(tree.bytes, 12);
    assert_eq!(tree.children.len(), 1);
    assert_eq!(tree.children[0].name, "crate_a");
    assert_eq!(tree.children[0].bytes, 12);
    assert_eq!(tree.children[0].children.len(), 1);
    assert_eq!(tree.children[0].children[0].name, "src/mod1.rs");
    assert_eq!(tree.children[0].children[0].bytes, 12);
    assert_eq!(tree.children[0].children[0].children.len(), 1);
    assert_eq!(
        tree.children[0].children[0].children[0].name,
        "crate_a::mod1::f"
    );
    assert_eq!(tree.children[0].children[0].children[0].bytes, 12);
}

#[test]
fn build_treemap_is_fast_and_scales_reasonably() {
    let symbol_count_small = 30_000usize;
    let symbol_count_large = 60_000usize;

    let (symbols_small, locations_small) = synthetic_inputs(symbol_count_small);
    let (symbols_large, locations_large) = synthetic_inputs(symbol_count_large);

    let duration_small = measure_fastest_duration(|| {
        build_treemap_from_symbols_and_locations(&symbols_small, &locations_small)
            .expect("tree")
            .bytes as usize
    });
    let duration_large = measure_fastest_duration(|| {
        build_treemap_from_symbols_and_locations(&symbols_large, &locations_large)
            .expect("tree")
            .bytes as usize
    });

    assert!(duration_small <= std::time::Duration::from_secs(1));
    let ratio = duration_large.as_secs_f64() / duration_small.as_secs_f64().max(1e-9);
    assert!(ratio <= 3.0);
}

#[test]
fn decode_uleb128_round_trip_small_values() {
    // 0x81 0x01 => 129
    let (value, read) = treemap_viewer::analysis::test_only_decode_uleb128(&[0x81, 0x01]);
    assert_eq!(value, 129);
    assert_eq!(read, 2);
}

#[test]
fn decode_macho_function_starts_deltas_accumulates_and_stops_on_zero() {
    // deltas: 1,2,3,0 => starts: 1,3,6
    let data = [0x01u8, 0x02u8, 0x03u8, 0x00u8];
    let starts = treemap_viewer::analysis::test_only_decode_macho_function_start_deltas(&data);
    assert_eq!(starts, vec![1, 3, 6]);
}

#[test]
fn build_ranges_from_starts_is_monotone_and_capped() {
    let starts = vec![10u64, 20u64, 35u64];
    let ranges = treemap_viewer::analysis::test_only_build_ranges_from_starts(&starts, 50);
    assert_eq!(ranges, vec![(10, 20), (20, 35), (35, 50)]);
}

fn synthetic_inputs(count: usize) -> (Vec<SymbolRecord>, Vec<ResolvedLocation>) {
    let mut symbols = Vec::with_capacity(count);
    let mut locations = Vec::with_capacity(count);

    for index in 0..count {
        symbols.push(SymbolRecord {
            address: index as u64,
            size_bytes: 1,
            raw_symbol_name: format!("sym{index}"),
        });
        locations.push(ResolvedLocation {
            function_name: format!("crate_{:02}::mod::f{}", index % 10, index % 100),
            file_path: Some(format!("src/mod{:02}.rs", index % 10)),
            line_number: Some((index % 1000) as u32),
        });
    }

    (symbols, locations)
}

fn measure_fastest_duration(mut f: impl FnMut() -> usize) -> std::time::Duration {
    std::hint::black_box(f());
    let mut best = std::time::Duration::MAX;
    for _ in 0..5 {
        let start = std::time::Instant::now();
        std::hint::black_box(f());
        best = best.min(start.elapsed());
    }
    best
}
