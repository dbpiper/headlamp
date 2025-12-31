#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TreemapItem<'a> {
    pub name: &'a str,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LaidOutItem<'a> {
    pub name: &'a str,
    pub bytes: u64,
    pub rect: Rect,
}

pub fn layout_treemap<'a>(items: &'a [TreemapItem<'a>], bounds: Rect) -> Vec<LaidOutItem<'a>> {
    let bounds_area = bounds.width.max(0.0) * bounds.height.max(0.0);
    if items.is_empty() || bounds_area <= 0.0 {
        return Vec::new();
    }

    let total_bytes = items.iter().map(|item| item.bytes).sum::<u64>();
    if total_bytes == 0 {
        return Vec::new();
    }

    let mut weighted_items = items
        .iter()
        .filter(|item| item.bytes > 0)
        .map(|item| WeightedItem {
            name: item.name,
            bytes: item.bytes,
            area: (item.bytes as f64) * bounds_area / (total_bytes as f64),
        })
        .collect::<Vec<_>>();

    if weighted_items.is_empty() {
        return Vec::new();
    }

    weighted_items.sort_by(|left, right| right.area.total_cmp(&left.area));
    squarify(weighted_items, bounds)
        .into_iter()
        .map(|laid_out| LaidOutItem {
            name: laid_out.name,
            bytes: laid_out.bytes,
            rect: laid_out.rect,
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct WeightedItem<'a> {
    name: &'a str,
    bytes: u64,
    area: f64,
}

#[derive(Debug, Clone, PartialEq)]
struct WeightedLayout<'a> {
    name: &'a str,
    bytes: u64,
    rect: Rect,
}

fn squarify<'a>(items: Vec<WeightedItem<'a>>, mut bounds: Rect) -> Vec<WeightedLayout<'a>> {
    let mut laid_out = Vec::with_capacity(items.len());
    let mut row = Vec::<WeightedItem<'a>>::new();
    let mut row_stats: Option<RowStats> = None;

    for next in items {
        if row.is_empty() {
            row.push(next);
            row_stats = Some(RowStats::from_item(next));
            continue;
        }

        let Some(current_stats) = row_stats else {
            row.push(next);
            row_stats = Some(RowStats::from_item(next));
            continue;
        };

        let short_side = bounds.width.min(bounds.height);
        let candidate_stats = current_stats.with_item(next);

        if worst_aspect_ratio_from_stats(candidate_stats, short_side)
            <= worst_aspect_ratio_from_stats(current_stats, short_side)
        {
            row.push(next);
            row_stats = Some(candidate_stats);
            continue;
        }

        let (row_layout, remaining) = layout_row(&row, bounds, current_stats.row_area);
        laid_out.extend(row_layout);
        bounds = remaining;
        row.clear();
        row.push(next);
        row_stats = Some(RowStats::from_item(next));
    }

    if !row.is_empty() {
        let final_row_area = row_stats.map(|stats| stats.row_area).unwrap_or(0.0);
        let (row_layout, _) = layout_row(&row, bounds, final_row_area);
        laid_out.extend(row_layout);
    }

    laid_out
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct RowStats {
    row_area: f64,
    min_area: f64,
    max_area: f64,
}

impl RowStats {
    fn from_item(item: WeightedItem<'_>) -> Self {
        Self {
            row_area: item.area,
            min_area: item.area,
            max_area: item.area,
        }
    }

    fn with_item(self, item: WeightedItem<'_>) -> Self {
        Self {
            row_area: self.row_area + item.area,
            min_area: self.min_area.min(item.area),
            max_area: self.max_area.max(item.area),
        }
    }
}

fn worst_aspect_ratio_from_stats(stats: RowStats, short_side: f64) -> f64 {
    if stats.row_area <= 0.0 {
        return f64::INFINITY;
    }
    if stats.min_area <= 0.0 || short_side <= 0.0 {
        return f64::INFINITY;
    }

    let short_side_squared = short_side * short_side;
    let row_area_squared = stats.row_area * stats.row_area;
    let first = (short_side_squared * stats.max_area) / row_area_squared;
    let second = row_area_squared / (short_side_squared * stats.min_area);
    first.max(second)
}

fn layout_row<'a>(
    row: &[WeightedItem<'a>],
    bounds: Rect,
    row_area: f64,
) -> (Vec<WeightedLayout<'a>>, Rect) {
    if row_area <= 0.0 || bounds.width <= 0.0 || bounds.height <= 0.0 {
        return (Vec::new(), bounds);
    }

    if bounds.width <= bounds.height {
        layout_row_horizontal(row, bounds, row_area)
    } else {
        layout_row_vertical(row, bounds, row_area)
    }
}

fn layout_row_horizontal<'a>(
    row: &[WeightedItem<'a>],
    bounds: Rect,
    row_area: f64,
) -> (Vec<WeightedLayout<'a>>, Rect) {
    let row_height = row_area / bounds.width;
    let mut x = bounds.x;
    let mut row_layout = Vec::with_capacity(row.len());

    for item in row {
        let item_width = item.area / row_height;
        row_layout.push(WeightedLayout {
            name: item.name,
            bytes: item.bytes,
            rect: Rect {
                x,
                y: bounds.y,
                width: item_width,
                height: row_height,
            },
        });
        x += item_width;
    }

    let remaining = Rect {
        x: bounds.x,
        y: bounds.y + row_height,
        width: bounds.width,
        height: (bounds.height - row_height).max(0.0),
    };
    (row_layout, remaining)
}

fn layout_row_vertical<'a>(
    row: &[WeightedItem<'a>],
    bounds: Rect,
    row_area: f64,
) -> (Vec<WeightedLayout<'a>>, Rect) {
    let row_width = row_area / bounds.height;
    let mut y = bounds.y;
    let mut row_layout = Vec::with_capacity(row.len());

    for item in row {
        let item_height = item.area / row_width;
        row_layout.push(WeightedLayout {
            name: item.name,
            bytes: item.bytes,
            rect: Rect {
                x: bounds.x,
                y,
                width: row_width,
                height: item_height,
            },
        });
        y += item_height;
    }

    let remaining = Rect {
        x: bounds.x + row_width,
        y: bounds.y,
        width: (bounds.width - row_width).max(0.0),
        height: bounds.height,
    };
    (row_layout, remaining)
}

// (worst_aspect_ratio is computed from RowStats to avoid row cloning / resumming)
