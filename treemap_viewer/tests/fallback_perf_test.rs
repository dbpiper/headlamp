use std::time::{Duration, Instant};

use treemap_viewer::analysis::build_treemap_from_symbols_and_locations;
use treemap_viewer::model::{ResolvedLocation, SymbolRecord};

#[test]
fn fallback_treemap_build_completes_under_one_second() {
    let leaf_count = 40_000usize;
    let (symbols, locations) = synthetic_function_leaves(leaf_count);

    let start = Instant::now();
    let tree = build_treemap_from_symbols_and_locations(&symbols, &locations).expect("tree");
    let elapsed = start.elapsed();

    assert_eq!(tree.bytes, leaf_count as u64);
    assert!(elapsed <= Duration::from_secs(1));
}

fn synthetic_function_leaves(count: usize) -> (Vec<SymbolRecord>, Vec<ResolvedLocation>) {
    let mut symbols = Vec::with_capacity(count);
    let mut locations = Vec::with_capacity(count);

    for index in 0..count {
        let function_name = format!("crate_{:02}::mod::f{}", index % 20, index % 200);
        let file_path = Some(format!("src/mod{:02}.rs", index % 20));

        symbols.push(SymbolRecord {
            address: index as u64,
            size_bytes: 1,
            raw_symbol_name: function_name.clone(),
        });
        locations.push(ResolvedLocation {
            function_name,
            file_path,
            line_number: Some((index % 1000) as u32),
        });
    }

    (symbols, locations)
}
