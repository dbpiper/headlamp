use std::collections::HashMap;

use crate::dwarf_addr2line::ResolvedLocation;
use crate::size_report::MapSymbol;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TreemapNode {
    pub name: String,
    pub bytes: u64,
    pub children: Vec<TreemapNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolWithLocation {
    pub symbol: MapSymbol,
    pub object_path: String,
    pub resolved: ResolvedLocation,
}

pub fn build_treemap(symbols: &[SymbolWithLocation]) -> TreemapNode {
    let mut root_children_by_crate: HashMap<String, Vec<SymbolWithLocation>> = HashMap::new();
    for entry in symbols {
        let crate_name = crate_name_from_object_path(&entry.object_path);
        root_children_by_crate
            .entry(crate_name)
            .or_default()
            .push(entry.clone());
    }

    let mut crate_nodes = root_children_by_crate
        .into_iter()
        .map(|(crate_name, entries)| build_crate_node(&crate_name, &entries))
        .collect::<Vec<_>>();
    crate_nodes.sort_by(|left, right| right.bytes.cmp(&left.bytes));

    let total_bytes = crate_nodes.iter().map(|node| node.bytes).sum::<u64>();
    TreemapNode {
        name: "root".to_string(),
        bytes: total_bytes,
        children: crate_nodes,
    }
}

fn build_crate_node(crate_name: &str, entries: &[SymbolWithLocation]) -> TreemapNode {
    let mut file_to_entries: HashMap<String, Vec<&SymbolWithLocation>> = HashMap::new();
    for entry in entries {
        let file_bucket = entry
            .resolved
            .file_path
            .clone()
            .unwrap_or_else(|| "unknown".to_string());
        file_to_entries.entry(file_bucket).or_default().push(entry);
    }

    let mut file_nodes = file_to_entries
        .into_iter()
        .map(|(file_path, file_entries)| build_file_node(&file_path, &file_entries))
        .collect::<Vec<_>>();
    file_nodes.sort_by(|left, right| right.bytes.cmp(&left.bytes));

    let total_bytes = file_nodes.iter().map(|node| node.bytes).sum::<u64>();
    TreemapNode {
        name: crate_name.to_string(),
        bytes: total_bytes,
        children: file_nodes,
    }
}

fn build_file_node(file_path: &str, entries: &[&SymbolWithLocation]) -> TreemapNode {
    let mut function_to_bytes: HashMap<String, u64> = HashMap::new();
    for entry in entries {
        let function_name = entry.resolved.function_name.clone();
        function_to_bytes
            .entry(function_name)
            .and_modify(|bytes| *bytes += entry.symbol.size_bytes)
            .or_insert(entry.symbol.size_bytes);
    }

    let mut function_nodes = function_to_bytes
        .into_iter()
        .map(|(function_name, bytes)| TreemapNode {
            name: function_name,
            bytes,
            children: Vec::new(),
        })
        .collect::<Vec<_>>();
    function_nodes.sort_by(|left, right| right.bytes.cmp(&left.bytes));

    let total_bytes = function_nodes.iter().map(|node| node.bytes).sum::<u64>();
    TreemapNode {
        name: file_path.to_string(),
        bytes: total_bytes,
        children: function_nodes,
    }
}

fn crate_name_from_object_path(object_path: &str) -> String {
    if object_path.contains("/rustlib/") {
        return "rust-stdlib".to_string();
    }
    if object_path == "linker synthesized" {
        return "linker".to_string();
    }

    if let Some(crate_name) = crate_name_from_rlib_reference(object_path) {
        return crate_name.to_string();
    }

    if object_path.contains("/target/")
        && object_path.contains("/deps/")
        && object_path.contains("headlamp-")
    {
        return "headlamp(bin)".to_string();
    }

    "unknown".to_string()
}

fn crate_name_from_rlib_reference(object_path: &str) -> Option<&str> {
    let filename = object_path.rsplit('/').next().unwrap_or(object_path);
    let rlib_offset = filename.find(".rlib")?;
    let before_rlib = &filename[..rlib_offset];
    let after_lib_prefix = before_rlib.strip_prefix("lib")?;
    let hash_separator_offset = after_lib_prefix.rfind('-')?;
    Some(&after_lib_prefix[..hash_separator_offset])
}
