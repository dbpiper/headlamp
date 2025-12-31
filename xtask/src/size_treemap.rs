use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::dwarf_addr2line::ResolvedLocation;
use crate::dwarf_inprocess::resolve_locations_inprocess;
use crate::size_report::{parse_map_symbols, parse_object_index_to_path, MapSymbol};
use crate::treemap::{build_treemap, SymbolWithLocation, TreemapNode};

#[derive(Debug, Clone)]
struct AggregatedSymbol {
    address: u64,
    bytes: u64,
    object_index: u32,
    raw_symbol_name: String,
}

pub struct SizeTreemapInputs {
    pub map_path: PathBuf,
    pub binary_path: PathBuf,
    pub focus_headlamp: bool,
}

pub fn generate_treemap_json(inputs: SizeTreemapInputs) -> anyhow::Result<TreemapNode> {
    let object_index_to_path = parse_object_index_to_path(&inputs.map_path)?;
    let mut symbols = parse_map_symbols(&inputs.map_path)?;

    if inputs.focus_headlamp {
        symbols.retain(|symbol| is_headlamp_symbol(symbol, &object_index_to_path));
    }

    let aggregated = aggregate_symbols(&symbols);
    let addresses = aggregated
        .iter()
        .map(|symbol| symbol.address)
        .collect::<Vec<_>>();
    let raw_symbol_names = aggregated
        .iter()
        .map(|symbol| symbol.raw_symbol_name.clone())
        .collect::<Vec<_>>();
    let resolved_locations =
        resolve_locations_inprocess(&inputs.binary_path, &addresses, &raw_symbol_names)?;

    let symbols_with_locations = join_aggregated_symbols_with_locations(
        &aggregated,
        &resolved_locations,
        &object_index_to_path,
    );
    Ok(build_treemap(&symbols_with_locations))
}

fn aggregate_symbols(symbols: &[MapSymbol]) -> Vec<AggregatedSymbol> {
    let mut key_to_index: HashMap<(&str, u32), usize> = HashMap::new();
    let mut aggregated: Vec<AggregatedSymbol> = Vec::new();

    for symbol in symbols {
        let key = (symbol.raw_symbol_name.as_str(), symbol.object_index);
        if let Some(existing_index) = key_to_index.get(&key).copied() {
            let existing = &mut aggregated[existing_index];
            existing.bytes += symbol.size_bytes;
            continue;
        }

        let index = aggregated.len();
        aggregated.push(AggregatedSymbol {
            address: symbol.address,
            bytes: symbol.size_bytes,
            object_index: symbol.object_index,
            raw_symbol_name: symbol.raw_symbol_name.clone(),
        });
        key_to_index.insert(key, index);
    }

    aggregated
}

fn join_aggregated_symbols_with_locations(
    symbols: &[AggregatedSymbol],
    resolved_locations: &[ResolvedLocation],
    object_index_to_path: &HashMap<u32, String>,
) -> Vec<SymbolWithLocation> {
    symbols
        .iter()
        .zip(resolved_locations.iter())
        .filter_map(|(symbol, resolved)| {
            let object_path = object_index_to_path.get(&symbol.object_index)?;
            Some(SymbolWithLocation {
                symbol: MapSymbol {
                    address: symbol.address,
                    size_bytes: symbol.bytes,
                    object_index: symbol.object_index,
                    raw_symbol_name: symbol.raw_symbol_name.clone(),
                },
                object_path: object_path.clone(),
                resolved: resolved.clone(),
            })
        })
        .collect::<Vec<_>>()
}

fn is_headlamp_symbol(symbol: &MapSymbol, object_index_to_path: &HashMap<u32, String>) -> bool {
    let Some(object_path) = object_index_to_path.get(&symbol.object_index) else {
        return false;
    };
    is_headlamp_object_path(object_path)
}

fn is_headlamp_object_path(object_path: &str) -> bool {
    if object_path.contains("libheadlamp-") {
        return true;
    }
    if object_path.contains("/target/")
        && object_path.contains("/release/deps/")
        && object_path.contains("headlamp-")
        && object_path.contains(".headlamp.")
    {
        return true;
    }
    false
}

pub fn write_treemap_json(output_path: &Path, treemap: &TreemapNode) -> anyhow::Result<()> {
    let json_text = serde_json::to_string_pretty(treemap)?;
    std::fs::write(output_path, json_text)?;
    Ok(())
}
