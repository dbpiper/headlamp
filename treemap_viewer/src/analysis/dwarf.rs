use std::path::Path;
use std::sync::Arc;

use crate::model::{ResolvedLocation, SymbolRecord, TreemapNode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnalysisStats {
    pub symbol_count: usize,
    pub resolved_file_count: usize,
    pub resolved_function_count: usize,
}

pub fn sample_stats(binary_path: &Path, symbols: &[SymbolRecord], limit: usize) -> AnalysisStats {
    let sample = &symbols[..limit.min(symbols.len())];
    let locations = resolve_locations(binary_path, sample).unwrap_or_default();
    super::tree::stats_from_locations(&locations)
}

pub fn build_tree_from_ranges(
    binary_path: &Path,
    symbols: &[SymbolRecord],
    ranges: &[(u64, u64)],
    sample_stats: AnalysisStats,
) -> (TreemapNode, AnalysisStats) {
    let mut symbol_lookup = symbols.to_vec();
    symbol_lookup.sort_by_key(|record| record.address);

    let dwarf_is_probably_present = sample_stats.resolved_file_count > 0;
    let dwarf_locations = if dwarf_is_probably_present {
        let starts = ranges.iter().map(|(start, _)| *start).collect::<Vec<_>>();
        resolve_locations_for_addresses(binary_path, &starts).ok()
    } else {
        None
    };

    let mut synthetic_symbols = Vec::with_capacity(ranges.len());
    let mut synthetic_locations = Vec::with_capacity(ranges.len());

    for (index, (start, end)) in ranges.iter().copied().enumerate() {
        let size_bytes = end.saturating_sub(start);
        if size_bytes == 0 {
            continue;
        }
        let fallback_label = super::symbols::label_address_from_symbols(&symbol_lookup, start);
        let resolved = dwarf_locations
            .as_ref()
            .and_then(|locations| locations.get(index))
            .cloned()
            .unwrap_or(ResolvedLocation {
                function_name: fallback_label.clone(),
                file_path: None,
                line_number: None,
            });

        let function_name =
            if resolved.function_name == "unknown" || resolved.function_name.is_empty() {
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

    let tree = super::tree::build_treemap_from_symbols_and_locations(
        &synthetic_symbols,
        &synthetic_locations,
    )
    .unwrap_or_else(|_| TreemapNode {
        name: "root".to_string(),
        bytes: 0,
        children: Vec::new(),
    });
    let stats = super::tree::stats_from_locations(&synthetic_locations);
    (tree, stats)
}

pub fn resolve_locations(
    binary_path: &Path,
    symbols: &[SymbolRecord],
) -> Result<Vec<ResolvedLocation>, String> {
    if symbols.is_empty() {
        return Ok(Vec::new());
    }

    let worker_count = super::tree::worker_count_for(symbols.len());
    let chunk_size = symbols.len().div_ceil(worker_count);
    let symbols = Arc::new(symbols.to_vec());

    let mut threads = Vec::with_capacity(worker_count);
    for worker_index in 0..worker_count {
        let start = worker_index * chunk_size;
        let end = (start + chunk_size).min(symbols.len());
        let slice_range = start..end;
        let symbols = Arc::clone(&symbols);
        let binary_path = binary_path.to_path_buf();
        threads.push(std::thread::spawn(move || {
            resolve_locations_for_range(&binary_path, &symbols, slice_range)
        }));
    }

    let mut results = Vec::with_capacity(symbols.len());
    for handle in threads {
        let mut part = handle
            .join()
            .map_err(|_| "addr2line worker panicked".to_string())??;
        results.append(&mut part);
    }
    Ok(results)
}

fn resolve_locations_for_range(
    binary_path: &Path,
    symbols: &[SymbolRecord],
    range: std::ops::Range<usize>,
) -> Result<Vec<ResolvedLocation>, String> {
    let loader = addr2line::Loader::new(binary_path)
        .map_err(|error| format!("failed to create addr2line loader: {error}"))?;
    let base = loader.relative_address_base();

    let mut out = Vec::with_capacity(range.len());
    for record in symbols[range].iter() {
        out.push(resolve_one_location(&loader, base, record));
    }
    Ok(out)
}

fn resolve_one_location(
    loader: &addr2line::Loader,
    base: u64,
    record: &SymbolRecord,
) -> ResolvedLocation {
    let probes = candidate_probes(record.address, base);
    let mut best = ResolvedLocation {
        function_name: super::tree::demangle_symbol(&record.raw_symbol_name),
        file_path: None,
        line_number: None,
    };

    for probe in probes.into_iter() {
        if let Some(frame_resolved) = resolve_from_frames(loader, probe) {
            return frame_resolved;
        }
        if let Some(loc_resolved) = resolve_from_location(loader, probe, &best.function_name) {
            best = loc_resolved;
        }
    }

    best
}

fn candidate_probes(address: u64, base: u64) -> [u64; 6] {
    let absolute = address;
    let relative = address.saturating_sub(base);
    [
        absolute,
        absolute.saturating_add(1),
        absolute.saturating_sub(1),
        relative,
        relative.saturating_add(1),
        relative.saturating_sub(1),
    ]
}

fn resolve_from_location(
    loader: &addr2line::Loader,
    probe: u64,
    function_name: &str,
) -> Option<ResolvedLocation> {
    let location = loader.find_location(probe).ok().flatten()?;
    Some(ResolvedLocation {
        function_name: function_name.to_string(),
        file_path: location.file.map(|path| path.to_string()),
        line_number: location.line,
    })
}

fn resolve_from_frames(loader: &addr2line::Loader, probe: u64) -> Option<ResolvedLocation> {
    let mut frames = loader.find_frames(probe).ok()?;
    while let Ok(Some(frame)) = frames.next() {
        let function_name = frame
            .function
            .as_ref()
            .and_then(|function| {
                function
                    .demangle()
                    .ok()
                    .or_else(|| function.raw_name().ok())
            })
            .map(|name| name.to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let (file_path, line_number) = match frame.location {
            None => (None, None),
            Some(location) => (location.file.map(|path| path.to_string()), location.line),
        };

        if function_name != "unknown" || file_path.is_some() || line_number.is_some() {
            return Some(ResolvedLocation {
                function_name,
                file_path,
                line_number,
            });
        }
    }
    None
}

fn resolve_locations_for_addresses(
    binary_path: &Path,
    addresses: &[u64],
) -> Result<Vec<ResolvedLocation>, String> {
    if addresses.is_empty() {
        return Ok(Vec::new());
    }

    let worker_count = super::tree::worker_count_for(addresses.len());
    let chunk_size = addresses.len().div_ceil(worker_count);
    let addresses = Arc::new(addresses.to_vec());

    let mut threads = Vec::with_capacity(worker_count);
    for worker_index in 0..worker_count {
        let start = worker_index * chunk_size;
        let end = (start + chunk_size).min(addresses.len());
        let range = start..end;
        let addresses = Arc::clone(&addresses);
        let binary_path = binary_path.to_path_buf();
        threads.push(std::thread::spawn(move || {
            resolve_locations_for_address_range(&binary_path, &addresses, range)
        }));
    }

    let mut results = Vec::with_capacity(addresses.len());
    for handle in threads {
        let mut part = handle
            .join()
            .map_err(|_| "addr2line worker panicked".to_string())??;
        results.append(&mut part);
    }
    Ok(results)
}

fn resolve_locations_for_address_range(
    binary_path: &Path,
    addresses: &[u64],
    range: std::ops::Range<usize>,
) -> Result<Vec<ResolvedLocation>, String> {
    let loader = addr2line::Loader::new(binary_path)
        .map_err(|error| format!("failed to create addr2line loader: {error}"))?;
    let base = loader.relative_address_base();

    let mut out = Vec::with_capacity(range.len());
    for address in addresses[range].iter().copied() {
        let probes = candidate_probes(address, base);
        let mut best = ResolvedLocation {
            function_name: "unknown".to_string(),
            file_path: None,
            line_number: None,
        };

        for probe in probes.into_iter() {
            if let Some(frame_resolved) = resolve_from_frames(&loader, probe) {
                best = frame_resolved;
                break;
            }
            if let Some(loc_resolved) = resolve_from_location(&loader, probe, &best.function_name) {
                best = loc_resolved;
            }
        }

        out.push(best);
    }

    Ok(out)
}
