use comfy_table::{Cell, CellAlignment, ColumnConstraint, ContentArrangement, Row, Table, Width};

use super::analysis::{
    composite_bar_pct, compute_uncovered_blocks, file_summary, missed_branches, missed_functions,
};
use super::column_widths::compute_column_widths;
use super::model::FullFileCoverage;

pub(super) fn render_per_file_composite_table(
    file: &FullFileCoverage,
    max_rows: usize,
    total_width: usize,
    max_hotspots: Option<u32>,
) -> Vec<String> {
    let rows_avail = 40usize;
    let table_budget = 14usize.max(max_rows.min(rows_avail + 8));
    let row_budget = 6usize.max(table_budget.saturating_sub(6));

    let total = if total_width > 20 { total_width } else { 100 };
    let file_max = 32usize.max(((total as f64) * 0.42).floor() as usize);
    let detail_max = 20usize.max(((total as f64) * 0.22).floor() as usize);
    let bar_max = 6usize.max(((total as f64) * 0.06).floor() as usize);

    let columns = vec![
        ("File", 28usize, file_max, CellAlignment::Left),
        ("Section", 8usize, 10usize, CellAlignment::Left),
        ("Where", 10usize, 14usize, CellAlignment::Left),
        ("Lines%", 6usize, 7usize, CellAlignment::Right),
        ("Bar", 6usize, bar_max, CellAlignment::Left),
        ("Funcs%", 6usize, 7usize, CellAlignment::Right),
        ("Branch%", 7usize, 8usize, CellAlignment::Right),
        ("Detail", 18usize, detail_max, CellAlignment::Left),
    ];

    let mins = columns
        .iter()
        .map(|(_, min, _, _)| *min)
        .collect::<Vec<_>>();
    let maxs = columns
        .iter()
        .map(|(_, _, max, _)| *max)
        .collect::<Vec<_>>();
    let widths = compute_column_widths(total_width, &mins, &maxs, columns.len());
    let bar_col_width = widths.get(4).copied().unwrap_or(10) as usize;

    let summary = file_summary(file);
    let blocks = compute_uncovered_blocks(file);
    let miss_fns = missed_functions(file);
    let misses = missed_branches(file);

    let mut rows: Vec<Vec<String>> = vec![];

    let rel = file.rel_path.clone();
    let dash = "—".to_string();
    rows.push(vec![
        rel.clone(),
        "Summary".to_string(),
        dash.clone(),
        format!("{:.1}%", summary.lines.pct()),
        bar_text(composite_bar_pct(&summary, &blocks), bar_col_width),
        format!("{:.1}%", summary.functions.pct()),
        format!("{:.1}%", summary.branches.pct()),
        String::new(),
    ]);
    rows.push(vec![
        rel.clone(),
        "Totals".to_string(),
        dash,
        format!("{:.1}%", summary.lines.pct()),
        String::new(),
        format!("{:.1}%", summary.functions.pct()),
        format!("{:.1}%", summary.branches.pct()),
        String::new(),
    ]);

    if !blocks.is_empty() || !miss_fns.is_empty() || !misses.is_empty() {
        let want_hs = max_hotspots
            .map(|n| (n.max(1) as usize).min(blocks.len()))
            .unwrap_or_else(|| ((row_budget as f64) * 0.45).ceil() as usize)
            .min(blocks.len());
        if want_hs > 0 {
            rows.push(vec![
                rel.clone(),
                "Hotspots".to_string(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                "(largest uncovered ranges)".to_string(),
            ]);
            for hotspot in blocks.iter().take(want_hs) {
                rows.push(vec![
                    rel.clone(),
                    "Hotspot".to_string(),
                    format!("L{}–L{}", hotspot.start, hotspot.end),
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                    format!("{} lines", hotspot.end - hotspot.start + 1),
                ]);
            }
        }

        let want_fn = ((row_budget as f64) * 0.25).ceil() as usize;
        let want_fn = want_fn.min(miss_fns.len());
        if want_fn > 0 {
            rows.push(vec![
                rel.clone(),
                "Function".to_string(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                "(never executed)".to_string(),
            ]);
            for missed in miss_fns.iter().take(want_fn) {
                rows.push(vec![
                    rel.clone(),
                    "Func".to_string(),
                    format!("L{}", missed.line),
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                    missed.name.clone(),
                ]);
            }
        }

        let want_br = ((row_budget as f64) * 0.2).ceil() as usize;
        let want_br = want_br.min(misses.len());
        if want_br > 0 {
            rows.push(vec![
                rel.clone(),
                "Branches".to_string(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                "(paths with 0 hits)".to_string(),
            ]);
            for missed in misses.iter().take(want_br) {
                rows.push(vec![
                    rel.clone(),
                    "Branch".to_string(),
                    format!("L{}", missed.line),
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                    format!(
                        "#{} missed [{}]",
                        missed.id,
                        missed
                            .zero_paths
                            .iter()
                            .map(|p| p.to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                ]);
            }
        }

        let target = row_budget;
        if rows.len() < target {
            let line_queue = blocks
                .iter()
                .flat_map(|range| (range.start..=range.end).collect::<Vec<_>>())
                .take(5000)
                .collect::<Vec<_>>();
            let mut iter = line_queue.into_iter();
            while rows.len() < target {
                match iter.next() {
                    Some(ln) => rows.push(vec![
                        rel.clone(),
                        "Line".to_string(),
                        format!("L{ln}"),
                        String::new(),
                        String::new(),
                        String::new(),
                        String::new(),
                        "uncovered".to_string(),
                    ]),
                    None => {
                        rows.push((0..8).map(|_| String::new()).collect());
                    }
                }
            }
        }
    }

    let mut table = Table::new();
    table.load_preset(BOX_TABLE_PRESET);
    table.set_content_arrangement(ContentArrangement::Disabled);
    table.set_truncation_indicator("");
    table.set_width(total_width as u16);

    let header_cells = columns
        .iter()
        .map(|(label, _min, _max, _align)| Cell::new(*label))
        .collect::<Vec<_>>();
    table.set_header(Row::from(header_cells));

    for (index, (_label, _min, _max, alignment)) in columns.iter().enumerate() {
        if let Some(column) = table.column_mut(index) {
            column.set_padding((0, 0));
            column.set_cell_alignment(*alignment);
            let width = widths
                .get(index)
                .copied()
                .unwrap_or(1)
                .min(u16::MAX as usize) as u16;
            column.set_constraint(ColumnConstraint::Absolute(Width::Fixed(width)));
        }
    }

    for raw_row in rows {
        let adjusted_cells = raw_row
            .into_iter()
            .enumerate()
            .map(|(index, raw_value)| {
                let width = widths.get(index).copied().unwrap_or(1);
                truncate_to_width(&raw_value, width)
            })
            .map(Cell::new)
            .collect::<Vec<_>>();
        let mut row = Row::from(adjusted_cells);
        row.max_height(1);
        table.add_row(row);
    }

    table
        .to_string()
        .lines()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
}

fn bar_text(pct: f64, width: usize) -> String {
    let filled = ((pct / 100.0) * (width as f64)).round() as usize;
    let filled = filled.min(width);
    format!("{}{}", "#".repeat(filled), "-".repeat(width - filled))
}

fn truncate_to_width(text: &str, width: usize) -> String {
    let actual_width = width.max(1);
    if text.chars().count() <= actual_width {
        return text.to_string();
    }
    text.chars().take(actual_width).collect::<String>()
}

const BOX_TABLE_PRESET: &str = "││──┼─┼┼│    ┬┴┌┐└┘";
