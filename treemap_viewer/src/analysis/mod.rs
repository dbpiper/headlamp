mod dwarf;
mod elf;
mod macho;
mod symbols;
mod tree;

use std::path::Path;

use crate::model::AttributionSource;
use crate::model::TreemapNode;

pub use dwarf::AnalysisStats;
pub use symbols::fill_missing_symbol_sizes;
pub use tree::{
    build_treemap_from_symbols_and_locations, crate_name_from_function_name,
    crate_name_from_function_name_and_file_path,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalysisOutput {
    pub tree: TreemapNode,
    pub stats: AnalysisStats,
    pub source: AttributionSource,
}

pub fn analyze_binary_with_fallback(binary_path: &Path) -> Result<AnalysisOutput, String> {
    let parsed = symbols::parse_binary(binary_path)?;
    let symbols = symbols::read_symbols_from_object(&parsed.object_file);
    if symbols.is_empty() {
        return Err("no symbols found".to_string());
    }

    let sample_stats = dwarf::sample_stats(binary_path, &symbols, 200);

    match parsed.object_file.format() {
        object::BinaryFormat::MachO => {
            let (source, ranges) =
                macho::function_ranges(&parsed.object_file).ok_or_else(|| {
                    "no Mach-O function ranges found; cannot build fallback".to_string()
                })?;
            Ok(build_output_from_ranges(
                binary_path,
                &symbols,
                &ranges,
                source,
                sample_stats,
            ))
        }
        object::BinaryFormat::Elf => {
            let ranges = elf::function_ranges(&parsed.object_file);
            Ok(build_output_from_ranges(
                binary_path,
                &symbols,
                &ranges,
                AttributionSource::ElfSymbols,
                sample_stats,
            ))
        }
        _ => {
            let locations = dwarf::resolve_locations(binary_path, &symbols).unwrap_or_default();
            let tree = tree::build_treemap_from_symbols_and_locations(&symbols, &locations)?;
            let stats = tree::stats_from_locations(&locations);
            Ok(AnalysisOutput {
                tree,
                stats,
                source: AttributionSource::SectionsOnly,
            })
        }
    }
}

fn build_output_from_ranges(
    binary_path: &Path,
    symbols: &[crate::model::SymbolRecord],
    ranges: &[(u64, u64)],
    source: AttributionSource,
    sample_stats: AnalysisStats,
) -> AnalysisOutput {
    let (tree, stats) = dwarf::build_tree_from_ranges(binary_path, symbols, ranges, sample_stats);
    AnalysisOutput {
        tree,
        stats,
        source,
    }
}

#[doc(hidden)]
pub fn test_only_decode_uleb128(data: &[u8]) -> (u64, usize) {
    macho::decode_uleb128(data)
}

#[doc(hidden)]
pub fn test_only_decode_macho_function_start_deltas(data: &[u8]) -> Vec<u64> {
    macho::decode_macho_function_start_deltas(data)
}

#[doc(hidden)]
pub fn test_only_build_ranges_from_starts(starts: &[u64], end_address: u64) -> Vec<(u64, u64)> {
    macho::build_ranges_from_starts(starts, end_address)
}
