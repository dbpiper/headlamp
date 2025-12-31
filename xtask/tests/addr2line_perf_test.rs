use std::time::{Duration, Instant};

#[test]
#[ignore]
fn addr2line_inprocess_resolves_headlamp_addresses_under_one_second() {
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repo root");
    let map_path = repo_root.join("target/size/headlamp.map");
    let binary_path = repo_root.join("target/release/headlamp");

    let symbols = xtask::size_report::parse_map_symbols(&map_path).expect("parse map symbols");
    let object_index_to_path =
        xtask::size_report::parse_object_index_to_path(&map_path).expect("parse object table");

    let mut seen = std::collections::HashSet::<(String, u32)>::new();
    let mut headlamp_addresses = Vec::<u64>::new();
    let mut headlamp_symbol_names = Vec::<String>::new();
    for symbol in symbols {
        let Some(object_path) = object_index_to_path.get(&symbol.object_index) else {
            continue;
        };
        if !object_path.contains("headlamp") {
            continue;
        }
        let key = (symbol.raw_symbol_name.clone(), symbol.object_index);
        if !seen.insert(key) {
            continue;
        }
        headlamp_addresses.push(symbol.address);
        headlamp_symbol_names.push(symbol.raw_symbol_name);
        if headlamp_addresses.len() >= 20_000 {
            break;
        }
    }

    let start = Instant::now();
    let resolved = xtask::dwarf_inprocess::resolve_locations_inprocess(
        &binary_path,
        &headlamp_addresses,
        &headlamp_symbol_names,
    )
    .expect("resolve");
    let elapsed = start.elapsed();

    assert_eq!(resolved.len(), headlamp_addresses.len());
    assert!(
        resolved.iter().any(|location| location
            .file_path
            .as_deref()
            .is_some_and(|path| path.contains("/headlamp/src/"))),
        "expected at least one resolved location to land in headlamp/src"
    );

    let budget = Duration::from_secs(1);
    assert!(
        elapsed <= budget,
        "addr2line in-process took {elapsed:?}, expected <= {budget:?}"
    );
}
