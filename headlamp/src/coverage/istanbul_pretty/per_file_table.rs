use super::analysis::{
    composite_bar_pct, compute_uncovered_blocks, file_summary, missed_branches, missed_functions,
};
use super::model::FullFileCoverage;
use super::table::{ColumnSpec, Decor, cell, cell_with};

pub(super) fn render_per_file_composite_table(
    file: &FullFileCoverage,
    max_rows: usize,
    total_width: usize,
    max_hotspots: Option<u32>,
    tty: bool,
) -> Vec<String> {
    let rows_avail = 40usize;
    let table_budget = 14usize.max(max_rows.min(rows_avail + 8));
    let row_budget = 6usize.max(table_budget.saturating_sub(6));

    let total = if total_width > 20 { total_width } else { 100 };
    let file_max = 32usize.max(((total as f64) * 0.42).floor() as usize);
    let detail_max = 20usize.max(((total as f64) * 0.22).floor() as usize);
    let bar_max = 6usize.max(((total as f64) * 0.06).floor() as usize);

    let columns = vec![
        ColumnSpec {
            label: "File",
            min: 28,
            max: file_max,
            align_right: false,
        },
        ColumnSpec {
            label: "Section",
            min: 8,
            max: 10,
            align_right: false,
        },
        ColumnSpec {
            label: "Where",
            min: 10,
            max: 14,
            align_right: false,
        },
        ColumnSpec {
            label: "Lines%",
            min: 6,
            max: 7,
            align_right: true,
        },
        ColumnSpec {
            label: "Bar",
            min: 6,
            max: bar_max,
            align_right: false,
        },
        ColumnSpec {
            label: "Funcs%",
            min: 6,
            max: 7,
            align_right: true,
        },
        ColumnSpec {
            label: "Branch%",
            min: 7,
            max: 8,
            align_right: true,
        },
        ColumnSpec {
            label: "Detail",
            min: 18,
            max: detail_max,
            align_right: false,
        },
    ];

    let summary = file_summary(file);
    let blocks = compute_uncovered_blocks(file);
    let miss_fns = missed_functions(file);
    let misses = missed_branches(file);

    let mut rows: Vec<Vec<super::table::Cell>> = vec![];

    let rel = file.rel_path.clone();
    let dash = "—".to_string();
    let l_pct = summary.lines.pct();
    let f_pct = summary.functions.pct();
    let b_pct = summary.branches.pct();
    rows.push(vec![
        cell_with(rel.clone(), Decor::ShortenPath { rel: rel.clone() }),
        cell_with("Summary", Decor::Bold),
        cell(dash.clone()),
        cell_with(format!("{l_pct:.1}%"), Decor::TintPct { pct: l_pct }),
        cell_with(
            String::new(),
            Decor::Bar {
                pct: composite_bar_pct(&summary, &blocks),
            },
        ),
        cell_with(format!("{f_pct:.1}%"), Decor::TintPct { pct: f_pct }),
        cell_with(format!("{b_pct:.1}%"), Decor::TintPct { pct: b_pct }),
        cell(""),
    ]);
    rows.push(vec![
        cell_with(rel.clone(), Decor::DimShortenPath { rel: rel.clone() }),
        cell_with("Totals", Decor::Dim),
        cell_with("—", Decor::Dim),
        cell_with(format!("{l_pct:.1}%"), Decor::Dim),
        cell_with(String::new(), Decor::Dim),
        cell_with(format!("{f_pct:.1}%"), Decor::Dim),
        cell_with(format!("{b_pct:.1}%"), Decor::Dim),
        cell(""),
    ]);

    if !blocks.is_empty() || !miss_fns.is_empty() || !misses.is_empty() {
        let want_hs = max_hotspots
            .map(|n| (n.max(1) as usize).min(blocks.len()))
            .unwrap_or_else(|| ((row_budget as f64) * 0.45).ceil() as usize)
            .min(blocks.len());
        if want_hs > 0 {
            rows.push(vec![
                cell_with(rel.clone(), Decor::DimShortenPath { rel: rel.clone() }),
                cell_with("Hotspots", Decor::Dim),
                cell_with("", Decor::Dim),
                cell_with("", Decor::Dim),
                cell_with("", Decor::Dim),
                cell_with("", Decor::Dim),
                cell_with("", Decor::Dim),
                cell_with("(largest uncovered ranges)", Decor::Dim),
            ]);
            for hotspot in blocks.iter().take(want_hs) {
                rows.push(vec![
                    cell_with(rel.clone(), Decor::ShortenPath { rel: rel.clone() }),
                    cell("Hotspot"),
                    cell(format!("L{}–L{}", hotspot.start, hotspot.end)),
                    cell(""),
                    cell(""),
                    cell(""),
                    cell(""),
                    cell(format!("{} lines", hotspot.end - hotspot.start + 1)),
                ]);
            }
        }

        let want_fn = ((row_budget as f64) * 0.25).ceil() as usize;
        let want_fn = want_fn.min(miss_fns.len());
        if want_fn > 0 {
            rows.push(vec![
                cell_with(rel.clone(), Decor::DimShortenPath { rel: rel.clone() }),
                cell_with("Functions", Decor::Dim),
                cell_with("", Decor::Dim),
                cell_with("", Decor::Dim),
                cell_with("", Decor::Dim),
                cell_with("", Decor::Dim),
                cell_with("", Decor::Dim),
                cell_with("(never executed)", Decor::Dim),
            ]);
            for missed in miss_fns.iter().take(want_fn) {
                rows.push(vec![
                    cell_with(rel.clone(), Decor::ShortenPath { rel: rel.clone() }),
                    cell("Func"),
                    cell(format!("L{}", missed.line)),
                    cell(""),
                    cell(""),
                    cell(""),
                    cell(""),
                    cell(missed.name.clone()),
                ]);
            }
        }

        let want_br = ((row_budget as f64) * 0.2).ceil() as usize;
        let want_br = want_br.min(misses.len());
        if want_br > 0 {
            rows.push(vec![
                cell_with(rel.clone(), Decor::DimShortenPath { rel: rel.clone() }),
                cell_with("Branches", Decor::Dim),
                cell_with("", Decor::Dim),
                cell_with("", Decor::Dim),
                cell_with("", Decor::Dim),
                cell_with("", Decor::Dim),
                cell_with("", Decor::Dim),
                cell_with("(paths with 0 hits)", Decor::Dim),
            ]);
            for missed in misses.iter().take(want_br) {
                rows.push(vec![
                    cell_with(rel.clone(), Decor::ShortenPath { rel: rel.clone() }),
                    cell("Branch"),
                    cell(format!("L{}", missed.line)),
                    cell(""),
                    cell(""),
                    cell(""),
                    cell(""),
                    cell(format!(
                        "#{} missed [{}]",
                        missed.id,
                        missed
                            .zero_paths
                            .iter()
                            .map(|p| p.to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )),
                ]);
            }
        }

        let target = if tty {
            row_budget.saturating_add(1)
        } else {
            row_budget
        };
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
                        cell_with(rel.clone(), Decor::ShortenPath { rel: rel.clone() }),
                        cell("Line"),
                        cell(format!("L{ln}")),
                        cell(""),
                        cell(""),
                        cell(""),
                        cell(""),
                        cell("uncovered"),
                    ]),
                    None => {
                        rows.push((0..8).map(|_| cell("")).collect());
                    }
                }
            }
        }
    }

    super::table::render_table(total_width, &columns, &rows)
        .lines()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
}
