use object::Object;
use object::ObjectSection;
use object::ObjectSegment;
use object::ObjectSymbol;

use crate::analysis::dwarf;
use crate::analysis::symbols;
use crate::analysis::tree;
use crate::analysis::AnalysisStats;
use crate::model::{AttributionSource, ResolvedLocation, SymbolRecord, TreemapNode};

pub(crate) fn build_tree_from_function_ranges_with_optional_dwarf(
    binary_path: &std::path::Path,
    symbols_in: &[SymbolRecord],
    ranges: &[(u64, u64)],
    sample_stats: AnalysisStats,
) -> (TreemapNode, AnalysisStats) {
    let mut symbol_lookup = symbols_in.to_vec();
    symbol_lookup.sort_by_key(|record| record.address);

    let mut synthetic_symbols = Vec::with_capacity(ranges.len());
    let mut synthetic_locations = Vec::with_capacity(ranges.len());

    let dwarf_is_probably_present = sample_stats.resolved_file_count > 0;
    let dwarf_locations = if dwarf_is_probably_present {
        let starts = ranges.iter().map(|(start, _)| *start).collect::<Vec<_>>();
        dwarf::resolve_locations_for_addresses(binary_path, &starts)
            .ok()
            .filter(|locations| locations.len() == starts.len())
    } else {
        None
    };

    for (index, (start, end)) in ranges.iter().copied().enumerate() {
        let size_bytes = end.saturating_sub(start);
        if size_bytes == 0 {
            continue;
        }

        let fallback_label = label_address_from_symbols(&symbol_lookup, start);
        let maybe_location = dwarf_locations
            .as_ref()
            .and_then(|locations| locations.get(index))
            .cloned();

        let resolved = maybe_location.unwrap_or(ResolvedLocation {
            function_name: fallback_label.clone(),
            file_path: None,
            line_number: None,
        });

        let function_name = if resolved.function_name == "unknown" || resolved.function_name.is_empty() {
            fallback_label.clone()
        } else {
            resolved.function_name.clone()
        };

        synthetic_symbols.push(SymbolRecord {
            address: start,
            size_bytes,
            raw_symbol_name: function_name.clone(),
        });
        synthetic_locations.push(ResolvedLocation {
            function_name,
            file_path: resolved.file_path.clone(),
            line_number: resolved.line_number,
        });
    }

    let tree = tree::build_treemap_from_symbols_and_locations(&synthetic_symbols, &synthetic_locations)
        .unwrap_or_else(|_| TreemapNode {
            name: "root".to_string(),
            bytes: 0,
            children: Vec::new(),
        });
    let stats = dwarf::stats_from_locations(&synthetic_locations);
    (tree, stats)
}

pub(crate) fn macho_function_ranges(file: &object::File<'_>) -> Option<(AttributionSource, Vec<(u64, u64)>)> {
    let text_segment_address = file.segments().find_map(|segment| {
        segment
            .name()
            .ok()
            .flatten()
            .filter(|name| *name == "__TEXT")
            .map(|_| segment.address())
    })?;
    let text_segment_end = file.segments().find_map(|segment| {
        segment
            .name()
            .ok()
            .flatten()
            .filter(|name| *name == "__TEXT")
            .map(|_| segment.address().saturating_add(segment.size()))
    })?;

    if let Some((source, starts)) = macho_function_starts_from_section(file, text_segment_address, text_segment_end) {
        let ranges = build_ranges_from_starts(&starts, text_segment_end);
        if !ranges.is_empty() {
            return Some((source, ranges));
        }
    }

    let mut starts = file
        .symbols()
        .chain(file.dynamic_symbols())
        .filter(|symbol| symbol.is_definition())
        .filter(|symbol| symbol.kind() == object::SymbolKind::Text)
        .map(|symbol| symbol.address())
        .filter(|address| *address >= text_segment_address && *address < text_segment_end)
        .collect::<Vec<_>>();
    starts.sort_unstable();
    starts.dedup();
    if starts.is_empty() {
        return None;
    }

    let ranges = build_ranges_from_starts(&starts, text_segment_end);
    if ranges.is_empty() {
        return None;
    }
    Some((AttributionSource::MachOTextSymbols, ranges))
}

pub(crate) fn elf_function_ranges(file: &object::File<'_>) -> Vec<(u64, u64)> {
    let mut functions = file
        .symbols()
        .chain(file.dynamic_symbols())
        .filter(|symbol| symbol.is_definition())
        .filter(|symbol| symbol.kind() == object::SymbolKind::Text)
        .filter(|symbol| symbol.address() != 0)
        .map(|symbol| (symbol.address(), symbol.size()))
        .collect::<Vec<_>>();

    functions.sort_by_key(|(address, _)| *address);
    let mut ranges = Vec::with_capacity(functions.len());
    for index in 0..functions.len() {
        let (start, size) = functions[index];
        let end = if size != 0 {
            start.saturating_add(size)
        } else {
            functions
                .get(index + 1)
                .map(|(next_start, _)| *next_start)
                .unwrap_or(start)
        };
        if end > start {
            ranges.push((start, end));
        }
    }
    ranges
}

fn macho_function_starts_from_section(
    file: &object::File<'_>,
    text_segment_address: u64,
    text_segment_end: u64,
) -> Option<(AttributionSource, Vec<u64>)> {
    let section = file
        .section_by_name("__TEXT,__function_starts")
        .or_else(|| file.section_by_name("__TEXT,__func_starts"))?;
    let data = section.data().ok()?;
    let offsets = decode_macho_function_start_deltas(data);
    if offsets.is_empty() {
        return None;
    }

    let mut starts = offsets
        .into_iter()
        .map(|offset| text_segment_address.saturating_add(offset))
        .filter(|start| *start >= text_segment_address && *start < text_segment_end)
        .collect::<Vec<_>>();
    starts.sort_unstable();
    starts.dedup();
    if starts.is_empty() {
        return None;
    }

    Some((AttributionSource::MachOFunctionStarts, starts))
}

fn build_ranges_from_starts(starts: &[u64], end_address: u64) -> Vec<(u64, u64)> {
    if starts.is_empty() {
        return Vec::new();
    }
    let mut ranges = Vec::with_capacity(starts.len());
    for index in 0..starts.len() {
        let start = starts[index];
        let end = starts
            .get(index + 1)
            .copied()
            .unwrap_or(end_address)
            .min(end_address);
        if end > start {
            ranges.push((start, end));
        }
    }
    ranges
}

#[doc(hidden)]
pub fn test_only_build_ranges_from_starts(starts: &[u64], end_address: u64) -> Vec<(u64, u64)> {
    build_ranges_from_starts(starts, end_address)
}

fn decode_macho_function_start_deltas(data: &[u8]) -> Vec<u64> {
    let mut cursor = 0usize;
    let mut current = 0u64;
    let mut out = Vec::new();

    loop {
        let (value, read) = decode_uleb128(&data[cursor..]);
        if read == 0 {
            break;
        }
        cursor += read;
        if value == 0 {
            break;
        }
        current = current.saturating_add(value);
        out.push(current);
        if cursor >= data.len() {
            break;
        }
    }
    out
}

fn decode_uleb128(data: &[u8]) -> (u64, usize) {
    let mut result = 0u64;
    let mut shift = 0u32;
    let mut index = 0usize;
    while index < data.len() {
        let byte = data[index];
        let low = (byte & 0x7f) as u64;
        if shift >= 64 {
            return (0, 0);
        }
        result |= low << shift;
        index += 1;
        if (byte & 0x80) == 0 {
            return (result, index);
        }
        shift = shift.saturating_add(7);
    }
    (0, 0)
}

#[doc(hidden)]
pub fn test_only_decode_uleb128(data: &[u8]) -> (u64, usize) {
    decode_uleb128(data)
}

#[doc(hidden)]
pub fn test_only_decode_macho_function_start_deltas(data: &[u8]) -> Vec<u64> {
    decode_macho_function_start_deltas(data)
}

fn label_address_from_symbols(symbols: &[SymbolRecord], address: u64) -> String {
    if symbols.is_empty() {
        return format!("function@0x{address:x}");
    }

    match symbols.binary_search_by_key(&address, |record| record.address) {
        Ok(index) => symbols::demangle_symbol(&symbols[index].raw_symbol_name),
        Err(0) => format!("function@0x{address:x}"),
        Err(insert_index) => {
            let base = &symbols[insert_index - 1];
            let base_name = symbols::demangle_symbol(&base.raw_symbol_name);
            let delta = address.saturating_sub(base.address);
            if delta == 0 {
                base_name
            } else {
                format!("{base_name}+0x{delta:x}")
            }
        }
    }
}

