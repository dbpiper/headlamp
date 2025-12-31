use dioxus::prelude::*;

use crate::app::LoadState;
use treemap_viewer::model::TreemapNode;

pub fn find_node_by_path<'a>(
    root: &'a TreemapNode,
    breadcrumb: &[String],
) -> Option<&'a TreemapNode> {
    let mut current = root;
    for segment in breadcrumb {
        current = current
            .children
            .iter()
            .find(|child| &child.name == segment)?;
    }
    Some(current)
}

pub fn children_total_bytes(node: &TreemapNode) -> u64 {
    let total = node.children.iter().map(|child| child.bytes).sum::<u64>();
    if total == 0 { node.bytes } else { total }
}

pub fn percent_of(bytes: u64, total: u64) -> f64 {
    if total == 0 {
        return 0.0;
    }
    (bytes as f64) * 100.0 / (total as f64)
}

pub fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;

    let bytes_f64 = bytes as f64;
    if bytes_f64 >= MIB {
        return format!("{:.2} MiB", bytes_f64 / MIB);
    }
    if bytes_f64 >= KIB {
        return format!("{:.2} KiB", bytes_f64 / KIB);
    }
    format!("{bytes} B")
}

pub fn color_for_name(name: &str) -> String {
    let mut hash = 1469598103934665603u64;
    for &byte in name.as_bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(1099511628211u64);
    }
    let hue = (hash % 360) as f64;
    format!("hsl({hue:.1}, 55%, 28%)")
}

pub fn export_current_tree(tree: Option<TreemapNode>, load_state: Signal<LoadState>) {
    let mut load_state = load_state;
    let Some(tree) = tree else {
        load_state.set(LoadState::Error("nothing to export yet".to_string()));
        return;
    };

    let Some(path) = rfd::FileDialog::new()
        .set_file_name("headlamp.treemap.json")
        .save_file()
    else {
        return;
    };

    match serde_json::to_string_pretty(&tree) {
        Err(error) => load_state.set(LoadState::Error(format!(
            "failed to serialize JSON: {error}"
        ))),
        Ok(text) => match std::fs::write(&path, text) {
            Ok(()) => load_state.set(LoadState::Ready),
            Err(error) => load_state.set(LoadState::Error(format!(
                "failed to write {}: {error}",
                path.display()
            ))),
        },
    }
}
