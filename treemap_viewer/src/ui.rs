use std::path::PathBuf;

use dioxus::prelude::dioxus_elements::HasFileData;
use dioxus::prelude::*;

use crate::app::{ChildEntry, LoadState};
use crate::util;
use treemap_viewer::analysis;

#[component]
pub fn TopBar(
    selected_binary: Option<PathBuf>,
    is_loading: bool,
    on_open: EventHandler<PathBuf>,
    on_reload: EventHandler<()>,
    on_cancel: EventHandler<()>,
    on_export: EventHandler<()>,
) -> Element {
    let display = selected_binary
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "no binary selected".to_string());

    rsx! {
        div { style: "display: flex; gap: 8px; align-items: center; margin-bottom: 8px;",
            button {
                style: "background: var(--surface); color: var(--fg); border: 1px solid var(--border); padding: 6px 10px; border-radius: 10px;",
                onclick: move |_| {
                    if let Some(path) = rfd::FileDialog::new().pick_file() {
                        on_open.call(path);
                    }
                },
                "Open Binary"
            }
            button {
                style: "background: var(--surface); color: var(--fg); border: 1px solid var(--border); padding: 6px 10px; border-radius: 10px;",
                onclick: move |_| on_reload.call(()),
                "Reload"
            }
            if is_loading {
                button {
                    style: "background: var(--surface); color: var(--fg); border: 1px solid var(--border); padding: 6px 10px; border-radius: 10px;",
                    onclick: move |_| on_cancel.call(()),
                    "Cancel"
                }
            }
            button {
                style: "background: var(--surface); color: var(--fg); border: 1px solid var(--border); padding: 6px 10px; border-radius: 10px;",
                onclick: move |_| on_export.call(()),
                "Export JSON"
            }
            div { style: "flex: 1; font-family: ui-monospace, SFMono-Regular; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--muted);",
                "{display}"
            }
        }
    }
}

#[component]
pub fn LoadStatusRow(state: LoadState) -> Element {
    match state {
        LoadState::Idle => {
            rsx! { div { style: "margin-bottom: 8px; color: var(--muted);", "pick or drop a binary to analyze" } }
        }
        LoadState::Ready => {
            rsx! { div { style: "margin-bottom: 8px; color: var(--muted);", "ready" } }
        }
        LoadState::Loading(message) => {
            rsx! { div { style: "margin-bottom: 8px; color: var(--muted);", "{message}" } }
        }
        LoadState::Error(message) => {
            rsx! { div { style: "margin-bottom: 8px; color: var(--danger);", "{message}" } }
        }
    }
}

#[component]
pub fn StatsRow(
    stats: Option<analysis::AnalysisStats>,
    source: Option<treemap_viewer::model::AttributionSource>,
) -> Element {
    let Some(stats) = stats else {
        return rsx! { div { style: "margin-bottom: 10px;", "" } };
    };

    let file_percent = if stats.symbol_count == 0 {
        0.0
    } else {
        (stats.resolved_file_count as f64) * 100.0 / (stats.symbol_count as f64)
    };

    rsx! {
        div { style: "margin-bottom: 10px; color: var(--muted); font-family: ui-monospace, SFMono-Regular; font-size: 12px;",
            "{source_label(source)} — resolved file/line for {stats.resolved_file_count}/{stats.symbol_count} symbols ({file_percent:.1}%)"
        }
    }
}

#[component]
pub fn SourceBanner(
    source: Option<treemap_viewer::model::AttributionSource>,
    stats: Option<analysis::AnalysisStats>,
) -> Element {
    let (source, stats) = match (source, stats) {
        (Some(source), Some(stats)) => (source, stats),
        _ => return rsx! { div { style: "margin-bottom: 10px;", "" } },
    };

    use treemap_viewer::model::AttributionSource;
    if source == AttributionSource::Dwarf {
        return rsx! { div { style: "margin-bottom: 10px;", "" } };
    }

    let file_percent = if stats.symbol_count == 0 {
        0.0
    } else {
        (stats.resolved_file_count as f64) * 100.0 / (stats.symbol_count as f64)
    };

    rsx! {
        div { style: "margin-bottom: 10px; padding: 10px 12px; border: 1px solid var(--border); border-radius: 12px; background: rgba(255,255,255,0.04); color: var(--fg);",
            div { style: "font-weight: 650; margin-bottom: 4px;", "Using fallback attribution" }
            div { style: "color: var(--muted); font-size: 12px;",
                "{source_label(Some(source))}. DWARF file/line coverage is {file_percent:.1}%."
            }
        }
    }
}

#[component]
pub fn DropZone(on_open: EventHandler<PathBuf>) -> Element {
    rsx! {
        div {
            style: "margin-bottom: 10px; padding: 12px; border: 1px dashed var(--border); border-radius: 10px; color: var(--muted); background: rgba(255,255,255,0.03);",
            ondragover: move |event| {
                event.prevent_default();
            },
            ondrop: move |event| {
                event.prevent_default();
                let files = event.data().files();
                if let Some(first) = files.first() {
                    on_open.call(first.path());
                }
            },
            "Drop a binary here (or use Open Binary)"
        }
    }
}

#[component]
pub fn TopChildrenPane(
    entries: Vec<ChildEntry>,
    total_bytes: u64,
    on_zoom: EventHandler<String>,
    on_hover: EventHandler<(String, u64)>,
    on_unhover: EventHandler<()>,
) -> Element {
    let mut rows = entries;
    rows.sort_by(|left, right| right.bytes.cmp(&left.bytes));

    rsx! {
        div {
            div { style: "margin-bottom: 8px; font-weight: 650; color: var(--fg);", "Top children" }
            div { style: "display: flex; flex-direction: column; gap: 6px; max-height: 720px; overflow: auto;",
                for child in rows.into_iter() {
                    {
                        let name = child.name;
                        let bytes = child.bytes;
                        let percent = util::percent_of(bytes, total_bytes);
                        let name_for_click = name.clone();
                        let name_for_mouse_enter = name.clone();

                        rsx! {
                            div {
                                style: "border: 1px solid var(--border); border-radius: 10px; padding: 8px; cursor: pointer; background: var(--surface);",
                                onclick: move |_| on_zoom.call(name_for_click.clone()),
                                onmouseenter: move |_| on_hover.call((name_for_mouse_enter.clone(), bytes)),
                                onmouseleave: move |_| on_unhover.call(()),
                                div { style: "font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--fg);", "{name}" }
                                div { style: "display: flex; justify-content: space-between; margin-top: 6px; font-size: 12px;",
                                    span { style: "font-family: ui-monospace, SFMono-Regular; color: var(--fg);", "{util::format_bytes(bytes)}" }
                                    span { style: "color: var(--muted);", "{percent:.2}%" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn BreadcrumbBar(breadcrumb: Vec<String>, on_navigate: EventHandler<Vec<String>>) -> Element {
    let button_paths = breadcrumb
        .iter()
        .enumerate()
        .map(|(index, segment)| {
            let mut truncated_path = breadcrumb.clone();
            truncated_path.truncate(index + 1);
            (segment.clone(), truncated_path)
        })
        .collect::<Vec<_>>();
    rsx! {
        div { style: "display: flex; gap: 6px; align-items: center; margin-bottom: 10px; flex-wrap: wrap;",
            button { onclick: move |_| on_navigate.call(Vec::new()), "root" }
            for (segment, truncated_path) in button_paths.into_iter() {
                span { ">" }
                button {
                    onclick: move |_| {
                        on_navigate.call(truncated_path.clone());
                    },
                    "{segment}"
                }
            }
        }
    }
}

fn source_label(source: Option<treemap_viewer::model::AttributionSource>) -> &'static str {
    use treemap_viewer::model::AttributionSource;
    match source {
        None => "unknown",
        Some(AttributionSource::Dwarf) => "DWARF",
        Some(AttributionSource::MachOFunctionStarts) => "fallback: Mach-O function_starts",
        Some(AttributionSource::MachOTextSymbols) => "fallback: Mach-O text symbols",
        Some(AttributionSource::ElfSymbols) => "fallback: ELF symbols",
        Some(AttributionSource::SectionsOnly) => "fallback: sections",
    }
}
