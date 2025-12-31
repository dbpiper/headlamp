use std::path::PathBuf;

use dioxus::prelude::*;

use crate::ui;
use crate::util;
use crate::view;
use treemap_viewer::analysis;
use treemap_viewer::model::TreemapNode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadState {
    Idle,
    Loading(String),
    Ready,
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChildEntry {
    pub name: String,
    pub bytes: u64,
}

#[derive(Clone, Copy, PartialEq)]
struct AppState {
    reload_nonce: Signal<u64>,
    selected_binary: Signal<Option<PathBuf>>,
    active_binary: Signal<Option<PathBuf>>,
    breadcrumb: Signal<Vec<String>>,
    load_state: Signal<LoadState>,
    root: Signal<Option<TreemapNode>>,
    stats: Signal<Option<analysis::AnalysisStats>>,
    source: Signal<Option<treemap_viewer::model::AttributionSource>>,
}

#[component]
pub fn App() -> Element {
    let state = create_app_state();
    start_loader(&state);
    let hovered = use_signal(|| None::<(String, u64)>);
    render_app_view(state, hovered)
}

fn render_app_view(state: AppState, hovered: Signal<Option<(String, u64)>>) -> Element {
    rsx! {
        div { style: "font-family: -apple-system, system-ui; padding: 12px; min-height: 100vh; background: var(--bg); color: var(--fg); \
                       --bg: #0b0f14; --fg: rgba(255,255,255,0.92); --muted: rgba(255,255,255,0.70); \
                       --border: rgba(255,255,255,0.14); --surface: rgba(255,255,255,0.06); --danger: #ff6b81;",
            h2 { "headlamp size treemap" }
            {render_app_header(state)}
            {render_app_body(state, hovered)}
        }
    }
}

fn render_app_header(state: AppState) -> Element {
    let mut reload_nonce_bumper = state.reload_nonce;
    let selected_binary_value = state.selected_binary;
    let load_state_value = state.load_state;
    let stats_value = state.stats;
    let source_value = state.source;
    let root_value = state.root;
    let breadcrumb_value = state.breadcrumb;
    let mut breadcrumb_setter = state.breadcrumb;

    rsx! {
        ui::TopBar {
            selected_binary: selected_binary_value(),
            is_loading: matches!(load_state_value(), LoadState::Loading(_)),
            on_open: move |path: PathBuf| {
                let mut selected_binary = state.selected_binary;
                let mut active_binary = state.active_binary;
                selected_binary.set(Some(path.clone()));
                active_binary.set(Some(path));
                reload_nonce_bumper += 1;
            },
            on_reload: move |_| {
                let selected = selected_binary_value();
                let mut active_binary = state.active_binary;
                active_binary.set(selected);
                reload_nonce_bumper += 1;
            },
            on_cancel: move |_| {
                let mut active_binary = state.active_binary;
                active_binary.set(None);
            },
            on_export: move |_| util::export_current_tree(root_value(), state.load_state),
        }

        ui::LoadStatusRow { state: load_state_value() }
        ui::StatsRow { stats: stats_value(), source: source_value() }
        ui::SourceBanner { source: source_value(), stats: stats_value() }

        ui::DropZone {
            on_open: move |path: PathBuf| {
                let mut selected_binary = state.selected_binary;
                let mut active_binary = state.active_binary;
                selected_binary.set(Some(path.clone()));
                active_binary.set(Some(path));
                reload_nonce_bumper += 1;
            }
        }

        ui::BreadcrumbBar {
            breadcrumb: breadcrumb_value(),
            on_navigate: move |path| breadcrumb_setter.set(path),
        }
    }
}

fn render_app_body(state: AppState, hovered: Signal<Option<(String, u64)>>) -> Element {
    let breadcrumb_snapshot = state.breadcrumb.read().clone();
    let root_ref = state.root.read();
    let current = root_ref
        .as_ref()
        .and_then(|tree| util::find_node_by_path(tree, &breadcrumb_snapshot));
    let has_root = root_ref.is_some();

    let breadcrumb_for_zoom = state.breadcrumb;
    let current_view = current.map(|node| {
        let child_entries = node
            .children
            .iter()
            .map(|child| ChildEntry {
                name: child.name.clone(),
                bytes: child.bytes,
            })
            .collect::<Vec<_>>();
        let total_bytes = util::children_total_bytes(node);
        (node, child_entries, total_bytes)
    });

    if let Some((node, child_entries, total_bytes)) = current_view {
        return rsx! {
            div { style: "display: flex; gap: 12px; align-items: flex-start;",
                div { style: "flex: 1; min-width: 0;",
                    {view::render_treemap(node, hovered, move |child_name| {
                        let mut breadcrumb_for_zoom = breadcrumb_for_zoom;
                        breadcrumb_for_zoom.write().push(child_name);
                    })}
                }
                div { style: "width: 360px; flex: none;",
                    ui::TopChildrenPane {
                        entries: child_entries,
                        total_bytes,
                        on_zoom: move |child_name| {
                            let mut breadcrumb_for_zoom = breadcrumb_for_zoom;
                            breadcrumb_for_zoom.write().push(child_name);
                        },
                        on_hover: move |entry| {
                            let mut hovered = hovered;
                            *hovered.write() = Some(entry);
                        },
                        on_unhover: move |_| {
                            let mut hovered = hovered;
                            *hovered.write() = None;
                        }
                    }
                }
            }
        };
    }

    if has_root {
        rsx! { div { "no node selected" } }
    } else {
        rsx! { div { "open or drop a binary to begin" } }
    }
}

fn create_app_state() -> AppState {
    AppState {
        reload_nonce: use_signal(|| 0u64),
        selected_binary: use_signal(|| None::<PathBuf>),
        active_binary: use_signal(|| None::<PathBuf>),
        breadcrumb: use_signal(Vec::<String>::new),
        load_state: use_signal(|| LoadState::Idle),
        root: use_signal(|| None::<TreemapNode>),
        stats: use_signal(|| None::<analysis::AnalysisStats>),
        source: use_signal(|| None::<treemap_viewer::model::AttributionSource>),
    }
}

fn start_loader(state: &AppState) {
    let reload_nonce = state.reload_nonce;
    let active_binary = state.active_binary;
    let mut breadcrumb = state.breadcrumb;
    let mut load_state = state.load_state;
    let mut root = state.root;
    let mut stats = state.stats;
    let mut source = state.source;

    let _analysis_resource = use_resource(move || async move {
        let _ = reload_nonce();
        let Some(binary_path) = active_binary() else {
            root.set(None);
            load_state.set(LoadState::Idle);
            breadcrumb.set(Vec::new());
            stats.set(None);
            source.set(None);
            return;
        };

        load_state.set(LoadState::Loading(format!(
            "analyzing {}",
            binary_path.display()
        )));
        let binary_path_for_work = binary_path.clone();
        let result = tokio::task::spawn_blocking(move || {
            analysis::analyze_binary_with_fallback(&binary_path_for_work)
        })
        .await;

        match result {
            Err(error) => {
                root.set(None);
                load_state.set(LoadState::Error(format!("analysis task failed: {error}")));
                stats.set(None);
                source.set(None);
            }
            Ok(Err(error)) => {
                root.set(None);
                load_state.set(LoadState::Error(error));
                stats.set(None);
                source.set(None);
            }
            Ok(Ok(output)) => {
                root.set(Some(output.tree));
                stats.set(Some(output.stats));
                source.set(Some(output.source));
                load_state.set(LoadState::Ready);
                breadcrumb.set(Vec::new());
            }
        }
    });
}
