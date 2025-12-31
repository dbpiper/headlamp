use dioxus::prelude::*;

use crate::util;
use treemap_viewer::layout;
use treemap_viewer::model::TreemapNode;

pub fn render_treemap(
    node: &TreemapNode,
    hovered: Signal<Option<(String, u64)>>,
    on_zoom: impl Fn(String) + Clone + 'static,
) -> Element {
    let total = util::children_total_bytes(node);
    let mut laid_out = layout_children(
        node,
        RectSpec {
            width: 980.0,
            height: 620.0,
        },
    );

    rsx! {
        div {
            HeaderRow { name: node.name.clone(), bytes: node.bytes }
            HoverRow { hovered: hovered() }

            div { style: "position: relative; width: 100%; max-width: 980px; height: 620px; border: 1px solid var(--border); border-radius: 10px; overflow: hidden; background: rgba(255,255,255,0.03);",
                for entry in laid_out.drain(..) {
                    {render_rect(entry, total, hovered, on_zoom.clone())}
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct RectSpec {
    width: f64,
    height: f64,
}

#[derive(Debug, Clone)]
struct LaidOutChild {
    name: String,
    bytes: u64,
    left: f64,
    top: f64,
    width: f64,
    height: f64,
}

fn render_rect(
    child: LaidOutChild,
    total: u64,
    hovered: Signal<Option<(String, u64)>>,
    on_zoom: impl Fn(String) + Clone + 'static,
) -> Element {
    let percent = util::percent_of(child.bytes, total);
    let background = util::color_for_name(&child.name);
    let label_visible = child.width >= 110.0 && child.height >= 42.0;
    let name_for_hover = child.name.clone();
    let name_for_click = child.name.clone();
    let hovered_for_mouse_enter = hovered;
    let hovered_for_mouse_leave = hovered;

    rsx! {
        div {
            style: format!(
                "position: absolute; left: {:.3}px; top: {:.3}px; width: {:.3}px; height: {:.3}px; \
                 background: {}; border: 1px solid rgba(255,255,255,0.10); box-sizing: border-box; padding: 6px; \
                 cursor: pointer; overflow: hidden; color: rgba(255,255,255,0.92);",
                child.left, child.top, child.width, child.height, background
            ),
            onclick: move |_| on_zoom.clone()(name_for_click.clone()),
            onmouseenter: move |_| {
                let mut hovered_for_mouse_enter = hovered_for_mouse_enter;
                *hovered_for_mouse_enter.write() = Some((name_for_hover.clone(), child.bytes));
            },
            onmouseleave: move |_| {
                let mut hovered_for_mouse_leave = hovered_for_mouse_leave;
                *hovered_for_mouse_leave.write() = None;
            },
            if label_visible {
                div { style: "font-weight: 650; font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                    "{child.name}"
                }
                div { style: "display: flex; justify-content: space-between; margin-top: 4px; font-size: 12px;",
                    span { style: "font-family: ui-monospace, SFMono-Regular;", "{util::format_bytes(child.bytes)}" }
                    span { style: "color: rgba(0,0,0,0.65);", "{percent:.2}%" }
                }
            }
        }
    }
}

fn layout_children(node: &TreemapNode, spec: RectSpec) -> Vec<LaidOutChild> {
    let items = node
        .children
        .iter()
        .map(|child| layout::TreemapItem {
            name: child.name.as_str(),
            bytes: child.bytes,
        })
        .collect::<Vec<_>>();

    let bounds = layout::Rect {
        x: 0.0,
        y: 0.0,
        width: spec.width,
        height: spec.height,
    };
    let laid_out = layout::layout_treemap(&items, bounds);

    laid_out
        .into_iter()
        .map(|entry| LaidOutChild {
            name: entry.name.to_string(),
            bytes: entry.bytes,
            left: entry.rect.x,
            top: entry.rect.y,
            width: entry.rect.width,
            height: entry.rect.height,
        })
        .collect()
}

#[component]
fn HeaderRow(name: String, bytes: u64) -> Element {
    rsx! {
        div { style: "margin-bottom: 8px; display: flex; justify-content: space-between;",
            div { style: "font-weight: 600;", "{name}" }
            div { style: "font-family: ui-monospace, SFMono-Regular;", "{util::format_bytes(bytes)}" }
        }
    }
}

#[component]
fn HoverRow(hovered: Option<(String, u64)>) -> Element {
    let Some((name, bytes)) = hovered else {
        return rsx! { div { style: "margin-bottom: 8px;", "" } };
    };
    rsx! {
        div { style: "margin-bottom: 8px;",
            span { style: "font-weight: 600;", "{name}" }
            span { " — " }
            span { style: "font-family: ui-monospace, SFMono-Regular;", "{util::format_bytes(bytes)}" }
        }
    }
}
