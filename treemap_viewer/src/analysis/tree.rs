use std::borrow::Cow;

use crate::model::{ResolvedLocation, SymbolRecord, TreemapNode};

use super::dwarf::AnalysisStats;

pub fn build_treemap_from_symbols_and_locations(
    symbols: &[SymbolRecord],
    locations: &[ResolvedLocation],
) -> Result<TreemapNode, String> {
    if symbols.len() != locations.len() {
        return Err("symbols/locations length mismatch".to_string());
    }

    let mut entries = build_entries_for_tree(symbols, locations);
    if entries.is_empty() {
        return Ok(TreemapNode {
            name: "root".to_string(),
            bytes: 0,
            children: Vec::new(),
        });
    }
    entries.sort_by(|left, right| {
        (
            left.crate_name.as_ref(),
            left.file_name.as_ref(),
            left.function_name,
        )
            .cmp(&(
                right.crate_name.as_ref(),
                right.file_name.as_ref(),
                right.function_name,
            ))
    });

    Ok(build_tree_from_sorted_entries(&entries))
}

pub fn stats_from_locations(locations: &[ResolvedLocation]) -> AnalysisStats {
    let resolved_file_count = locations
        .iter()
        .filter(|loc| loc.file_path.is_some())
        .count();
    let resolved_function_count = locations
        .iter()
        .filter(|loc| loc.function_name != "unknown" && !loc.function_name.is_empty())
        .count();

    AnalysisStats {
        symbol_count: locations.len(),
        resolved_file_count,
        resolved_function_count,
    }
}

pub fn demangle_symbol(raw_symbol_name: &str) -> String {
    let trimmed = raw_symbol_name.trim_start_matches('_');
    if let Ok(demangled) = rustc_demangle::try_demangle(trimmed) {
        return format!("{demangled:#}");
    }
    raw_symbol_name.to_string()
}

pub fn worker_count_for(item_count: usize) -> usize {
    let Ok(parallelism) = std::thread::available_parallelism() else {
        return 1;
    };
    let maximum = parallelism.get().max(1);
    let bounded = maximum.min(item_count.max(1));
    bounded.max(1)
}

#[derive(Debug, Clone)]
struct TreeEntry<'a> {
    crate_name: Cow<'a, str>,
    file_name: Cow<'a, str>,
    function_name: &'a str,
    bytes: u64,
}

fn build_entries_for_tree<'a>(
    symbols: &'a [SymbolRecord],
    locations: &'a [ResolvedLocation],
) -> Vec<TreeEntry<'a>> {
    let mut entries = Vec::with_capacity(symbols.len());
    for (record, location) in symbols.iter().zip(locations.iter()) {
        let bytes = record.size_bytes;
        if bytes == 0 {
            continue;
        }

        let crate_name = crate_name_from_function_name_and_file_path_cow(
            location.function_name.as_str(),
            location.file_path.as_deref(),
        );
        let file_name = location
            .file_path
            .as_deref()
            .map(Cow::Borrowed)
            .unwrap_or_else(|| Cow::Owned(format!("unknown@0x{:x}", record.address)));
        let function_name = location.function_name.as_str();

        entries.push(TreeEntry {
            crate_name,
            file_name,
            function_name,
            bytes,
        });
    }
    entries
}

fn build_tree_from_sorted_entries(entries: &[TreeEntry<'_>]) -> TreemapNode {
    let mut root_children = Vec::<TreemapNode>::new();

    let mut current_crate: Option<TreemapNode> = None;
    let mut current_file: Option<TreemapNode> = None;
    let mut current_function_name: Option<&str> = None;
    let mut current_function_bytes: u64 = 0;

    for entry in entries {
        let crate_changed = current_crate
            .as_ref()
            .is_none_or(|node| node.name != entry.crate_name.as_ref());
        let file_changed = current_file
            .as_ref()
            .is_none_or(|node| node.name != entry.file_name.as_ref());
        let function_changed = current_function_name.is_none_or(|name| name != entry.function_name);

        if crate_changed || file_changed || function_changed {
            flush_function_into_file(
                &mut current_file,
                &mut current_function_name,
                &mut current_function_bytes,
            );
        }
        if crate_changed || file_changed {
            flush_file_into_crate(&mut current_crate, &mut current_file);
        }
        if crate_changed {
            flush_crate_into_root(&mut root_children, &mut current_crate);
            current_crate = Some(TreemapNode {
                name: entry.crate_name.as_ref().to_string(),
                bytes: 0,
                children: Vec::new(),
            });
        }
        if crate_changed || file_changed {
            current_file = Some(TreemapNode {
                name: entry.file_name.as_ref().to_string(),
                bytes: 0,
                children: Vec::new(),
            });
        }
        if crate_changed || file_changed || function_changed {
            current_function_name = Some(entry.function_name);
            current_function_bytes = 0;
        }

        current_function_bytes = current_function_bytes.saturating_add(entry.bytes);
    }

    flush_pending_nodes(
        &mut root_children,
        &mut current_crate,
        &mut current_file,
        &mut current_function_name,
        &mut current_function_bytes,
    );

    finalize_root_children(&mut root_children);
    let total_bytes = root_children.iter().map(|node| node.bytes).sum::<u64>();

    TreemapNode {
        name: "root".to_string(),
        bytes: total_bytes,
        children: root_children,
    }
}

fn flush_pending_nodes(
    root_children: &mut Vec<TreemapNode>,
    current_crate: &mut Option<TreemapNode>,
    current_file: &mut Option<TreemapNode>,
    current_function_name: &mut Option<&str>,
    current_function_bytes: &mut u64,
) {
    flush_function_into_file(current_file, current_function_name, current_function_bytes);
    flush_file_into_crate(current_crate, current_file);
    flush_crate_into_root(root_children, current_crate);
}

fn finalize_root_children(root_children: &mut [TreemapNode]) {
    for crate_node in root_children.iter_mut() {
        for file_node in crate_node.children.iter_mut() {
            file_node.bytes = file_node.children.iter().map(|n| n.bytes).sum();
        }
        crate_node.bytes = crate_node.children.iter().map(|n| n.bytes).sum();
        crate_node.children.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    }
    root_children.sort_by(|a, b| b.bytes.cmp(&a.bytes));
}

fn flush_function_into_file(
    current_file: &mut Option<TreemapNode>,
    current_function_name: &mut Option<&str>,
    current_function_bytes: &mut u64,
) {
    let Some(file_node) = current_file.as_mut() else {
        current_function_name.take();
        *current_function_bytes = 0;
        return;
    };
    let Some(function_name) = current_function_name.take() else {
        *current_function_bytes = 0;
        return;
    };
    if *current_function_bytes == 0 {
        return;
    }
    file_node.children.push(TreemapNode {
        name: function_name.to_string(),
        bytes: *current_function_bytes,
        children: Vec::new(),
    });
    *current_function_bytes = 0;
}

fn flush_file_into_crate(
    current_crate: &mut Option<TreemapNode>,
    current_file: &mut Option<TreemapNode>,
) {
    let Some(crate_node) = current_crate.as_mut() else {
        current_file.take();
        return;
    };
    let Some(mut file_node) = current_file.take() else {
        return;
    };
    file_node.children.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    file_node.bytes = file_node.children.iter().map(|n| n.bytes).sum();
    if file_node.bytes == 0 {
        return;
    }
    crate_node.children.push(file_node);
}

fn flush_crate_into_root(
    root_children: &mut Vec<TreemapNode>,
    current_crate: &mut Option<TreemapNode>,
) {
    let Some(mut crate_node) = current_crate.take() else {
        return;
    };
    crate_node.children.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    crate_node.bytes = crate_node.children.iter().map(|n| n.bytes).sum();
    if crate_node.bytes == 0 {
        return;
    }
    root_children.push(crate_node);
}

pub fn crate_name_from_function_name(function_name: &str) -> String {
    crate_name_from_function_name_cow(function_name).into_owned()
}

pub fn crate_name_from_function_name_and_file_path(
    function_name: &str,
    file_path: Option<&str>,
) -> String {
    crate_name_from_function_name_and_file_path_cow(function_name, file_path).into_owned()
}

fn crate_name_from_function_name_cow(function_name: &str) -> Cow<'_, str> {
    let first = function_name.split("::").next().unwrap_or("unknown").trim();
    if first.is_empty() {
        return Cow::Borrowed("unknown");
    }
    Cow::Borrowed(first.strip_prefix('<').unwrap_or(first))
}

fn crate_name_from_function_name_and_file_path_cow<'a>(
    function_name: &'a str,
    file_path: Option<&'a str>,
) -> Cow<'a, str> {
    let from_function = crate_name_from_function_name_cow(function_name);
    let from_function_str = from_function.as_ref();
    let function_is_probably_not_a_crate = !function_name.contains("::")
        || from_function_str == "unknown"
        || from_function_str.starts_with('_')
        || from_function_str.contains('.');

    if function_is_probably_not_a_crate
        && let Some(path) = file_path
        && let Some(from_path) = crate_name_from_file_path(path)
    {
        return Cow::Owned(from_path);
    }

    from_function
}

fn crate_name_from_file_path(file_path: &str) -> Option<String> {
    if file_path.contains("/headlamp/") {
        return Some("headlamp".to_string());
    }

    let marker = "/registry/src/";
    let marker_index = file_path.find(marker)?;
    let after_marker = &file_path[(marker_index + marker.len())..];
    let crate_dir = after_marker.split('/').nth(1)?;
    let without_hash = crate_dir
        .rsplit_once('-')
        .map(|(left, _)| left)
        .unwrap_or(crate_dir);
    if without_hash.is_empty() {
        return None;
    }
    Some(without_hash.to_string())
}
