use treemap_viewer::layout::{Rect, TreemapItem, layout_treemap};

#[test]
fn layout_rects_stay_within_bounds() {
    let items = vec![
        TreemapItem {
            name: "a",
            bytes: 10,
        },
        TreemapItem {
            name: "b",
            bytes: 20,
        },
        TreemapItem {
            name: "c",
            bytes: 30,
        },
        TreemapItem {
            name: "d",
            bytes: 40,
        },
    ];
    let bounds = Rect {
        x: 0.0,
        y: 0.0,
        width: 800.0,
        height: 600.0,
    };

    let laid_out = layout_treemap(&items, bounds);
    assert_eq!(laid_out.len(), items.len());

    for entry in laid_out {
        assert!(entry.rect.width >= 0.0);
        assert!(entry.rect.height >= 0.0);

        assert!(entry.rect.x + entry.rect.width <= bounds.x + bounds.width + 1e-6);
        assert!(entry.rect.y + entry.rect.height <= bounds.y + bounds.height + 1e-6);
        assert!(entry.rect.x >= bounds.x - 1e-6);
        assert!(entry.rect.y >= bounds.y - 1e-6);
    }
}

#[test]
fn layout_area_is_conserved_reasonably() {
    let items = vec![
        TreemapItem {
            name: "a",
            bytes: 1,
        },
        TreemapItem {
            name: "b",
            bytes: 1,
        },
        TreemapItem {
            name: "c",
            bytes: 2,
        },
        TreemapItem {
            name: "d",
            bytes: 6,
        },
    ];
    let bounds = Rect {
        x: 0.0,
        y: 0.0,
        width: 1000.0,
        height: 500.0,
    };

    let laid_out = layout_treemap(&items, bounds);
    let laid_out_area = laid_out
        .iter()
        .map(|entry| entry.rect.width * entry.rect.height)
        .sum::<f64>();
    let bounds_area = bounds.width * bounds.height;
    let error = (bounds_area - laid_out_area).abs();

    assert!(error <= bounds_area * 1e-6);
}

#[test]
fn layout_returns_empty_for_empty_or_zero_input() {
    let bounds = Rect {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 100.0,
    };
    let empty: Vec<TreemapItem<'_>> = Vec::new();
    assert!(layout_treemap(&empty, bounds).is_empty());

    let zero = vec![
        TreemapItem {
            name: "a",
            bytes: 0,
        },
        TreemapItem {
            name: "b",
            bytes: 0,
        },
    ];
    assert!(layout_treemap(&zero, bounds).is_empty());
}

#[test]
fn layout_is_fast_for_large_inputs() {
    let bounds = Rect {
        x: 0.0,
        y: 0.0,
        width: 1200.0,
        height: 800.0,
    };

    let items_10k = build_items(10_000);
    let items_20k = build_items(20_000);

    let duration_10k =
        measure_fastest_duration(|| std::hint::black_box(layout_treemap(&items_10k, bounds)).len());
    let duration_20k =
        measure_fastest_duration(|| std::hint::black_box(layout_treemap(&items_20k, bounds)).len());

    assert!(duration_10k <= std::time::Duration::from_secs(2));

    let baseline = duration_10k.as_secs_f64().max(1e-9);
    let ratio = duration_20k.as_secs_f64() / baseline;
    assert!(ratio <= 10.0);
}

fn build_items(count: usize) -> Vec<TreemapItem<'static>> {
    let mut seed = 0x9e3779b97f4a7c15u64 ^ (count as u64);
    let mut items = Vec::with_capacity(count);
    for index in 0..count {
        let name: &'static str = Box::leak(format!("n{index}").into_boxed_str());
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let bytes = (seed % 10_000) + 1;
        items.push(TreemapItem { name, bytes });
    }

    items
}

fn measure_fastest_duration(mut f: impl FnMut() -> usize) -> std::time::Duration {
    std::hint::black_box(f());
    let mut best = std::time::Duration::MAX;
    for _ in 0..5 {
        let start = std::time::Instant::now();
        std::hint::black_box(f());
        best = best.min(start.elapsed());
    }
    best
}
