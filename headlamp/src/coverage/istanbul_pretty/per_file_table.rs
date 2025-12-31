use super::analysis::composite_bar_pct;
use super::model::FullFileCoverage;
use super::path_shorten::shorten_path_preserving_filename;
use super::table::{
    Cell, ColumnSpec, Decor, TableFrame, build_table_frame, cell, cell_with, compute_column_widths,
    write_table_with_frame_const,
};
use std::sync::Arc;
use std::sync::LazyLock;

pub(super) struct PerFileTableLayout {
    pub(super) columns: Vec<ColumnSpec>,
    pub(super) widths: Vec<usize>,
    pub(super) frame: TableFrame,
}

pub(super) fn build_per_file_table_layout(total_width: usize) -> PerFileTableLayout {
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
    let widths = compute_column_widths(total_width, &columns);
    let frame = build_table_frame(&columns, &widths);
    PerFileTableLayout {
        columns,
        widths,
        frame,
    }
}

static ARC_DASH: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("—"));
static ARC_EMPTY: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from(""));
static ARC_LABEL_LINE: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("Line"));
static ARC_LABEL_UNCOVERED: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("uncovered"));
static ARC_LABEL_SUMMARY: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("Summary"));
static ARC_LABEL_TOTALS: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("Totals"));
static ARC_LABEL_HOTSPOTS: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("Hotspots"));
static ARC_LABEL_HOTSPOT: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("Hotspot"));
static ARC_LABEL_FUNCTIONS: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("Functions"));
static ARC_LABEL_FUNC: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("Func"));
static ARC_LABEL_BRANCHES: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("Branches"));
static ARC_LABEL_BRANCH: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("Branch"));
static ARC_NA: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("N/A"));
static ARC_PCT_0: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("0.0%"));
static ARC_PCT_100: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("100.0%"));
static ARC_HOTSPOTS_NOTE: LazyLock<Arc<str>> =
    LazyLock::new(|| Arc::from("(largest uncovered ranges)"));
static ARC_FUNCTIONS_NOTE: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("(never executed)"));
static ARC_BRANCHES_NOTE: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("(paths with 0 hits)"));

fn pct_text(pct: f64) -> Arc<str> {
    let tenths = (pct * 10.0).round() as i64;
    if tenths == 0 {
        return ARC_PCT_0.clone();
    }
    if tenths == 1000 {
        return ARC_PCT_100.clone();
    }
    Arc::from(format!("{:.1}%", (tenths as f64) / 10.0))
}

pub(super) struct PerFileCompositeTableInput<'a> {
    pub(super) file: &'a FullFileCoverage,
    pub(super) summary: &'a super::model::FileSummary,
    pub(super) blocks: &'a [super::model::UncoveredRange],
    pub(super) missed_functions: &'a [super::model::MissedFunction],
    pub(super) missed_branches: &'a [super::model::MissedBranch],
    pub(super) max_rows: usize,
    pub(super) layout: &'a PerFileTableLayout,
    pub(super) max_hotspots: Option<u32>,
    pub(super) tty: bool,
}

pub(super) fn write_per_file_composite_table(
    out: &mut String,
    input: &PerFileCompositeTableInput<'_>,
) {
    let PerFileCompositeTableInput {
        file,
        summary,
        blocks,
        missed_functions,
        missed_branches,
        max_rows,
        layout,
        max_hotspots,
        tty,
    } = input;
    let max_rows = *max_rows;
    let max_hotspots = *max_hotspots;
    let tty = *tty;
    let rows_avail = 40usize;
    let table_budget = 14usize.max(max_rows.min(rows_avail + 8));
    let row_budget = 6usize.max(table_budget.saturating_sub(6));

    let mut rows: Vec<[Cell; 8]> = Vec::with_capacity(32);

    let file_col_width = layout.widths.first().copied().unwrap_or(28);
    let shortened_file_text: Arc<str> = Arc::from(shorten_path_preserving_filename(
        file.rel_path.as_str(),
        file_col_width,
    ));
    let dash = ARC_DASH.clone();
    let empty = ARC_EMPTY.clone();
    let label_line = ARC_LABEL_LINE.clone();
    let label_uncovered = ARC_LABEL_UNCOVERED.clone();
    let blank_row: [Cell; 8] = [
        cell(empty.clone()),
        cell(empty.clone()),
        cell(empty.clone()),
        cell(empty.clone()),
        cell(empty.clone()),
        cell(empty.clone()),
        cell(empty.clone()),
        cell(empty.clone()),
    ];
    let l_pct = summary.lines.pct();
    let f_pct = summary.functions.pct();
    let b_pct = summary.branches.pct();
    let l_pct_text: Arc<str> = pct_text(l_pct);
    let f_pct_text: Arc<str> = pct_text(f_pct);
    let b_pct_text: Arc<str> = if summary.branches.total == 0 {
        ARC_NA.clone()
    } else {
        pct_text(b_pct)
    };
    rows.push([
        cell(shortened_file_text.clone()),
        cell_with(ARC_LABEL_SUMMARY.clone(), Decor::Bold),
        cell(dash.clone()),
        cell_with(l_pct_text.clone(), Decor::TintPct { pct: l_pct }),
        cell_with(
            empty.clone(),
            Decor::Bar {
                pct: composite_bar_pct(summary, blocks),
            },
        ),
        cell_with(f_pct_text.clone(), Decor::TintPct { pct: f_pct }),
        cell_with(b_pct_text.clone(), Decor::TintPct { pct: b_pct }),
        cell(empty.clone()),
    ]);
    rows.push([
        cell_with(shortened_file_text.clone(), Decor::Dim),
        cell_with(ARC_LABEL_TOTALS.clone(), Decor::Dim),
        cell_with(dash.clone(), Decor::Dim),
        cell_with(l_pct_text.clone(), Decor::Dim),
        cell_with(empty.clone(), Decor::Dim),
        cell_with(f_pct_text.clone(), Decor::Dim),
        cell_with(b_pct_text.clone(), Decor::Dim),
        cell(empty.clone()),
    ]);

    if !blocks.is_empty() || !missed_functions.is_empty() || !missed_branches.is_empty() {
        let want_hs = max_hotspots
            .map(|n| (n.max(1) as usize).min(blocks.len()))
            .unwrap_or_else(|| ((row_budget as f64) * 0.45).ceil() as usize)
            .min(blocks.len());
        if want_hs > 0 {
            rows.push([
                cell_with(shortened_file_text.clone(), Decor::Dim),
                cell_with(ARC_LABEL_HOTSPOTS.clone(), Decor::Dim),
                cell_with(empty.clone(), Decor::Dim),
                cell_with(empty.clone(), Decor::Dim),
                cell_with(empty.clone(), Decor::Dim),
                cell_with(empty.clone(), Decor::Dim),
                cell_with(empty.clone(), Decor::Dim),
                cell_with(ARC_HOTSPOTS_NOTE.clone(), Decor::Dim),
            ]);
            for hotspot in blocks.iter().take(want_hs) {
                rows.push([
                    cell(shortened_file_text.clone()),
                    cell(ARC_LABEL_HOTSPOT.clone()),
                    cell(format!("L{}–L{}", hotspot.start, hotspot.end)),
                    cell(empty.clone()),
                    cell(empty.clone()),
                    cell(empty.clone()),
                    cell(empty.clone()),
                    cell(format!("{} lines", hotspot.end - hotspot.start + 1)),
                ]);
            }
        }

        let want_fn = ((row_budget as f64) * 0.25).ceil() as usize;
        let want_fn = want_fn.min(missed_functions.len());
        if want_fn > 0 {
            rows.push([
                cell_with(shortened_file_text.clone(), Decor::Dim),
                cell_with(ARC_LABEL_FUNCTIONS.clone(), Decor::Dim),
                cell_with(empty.clone(), Decor::Dim),
                cell_with(empty.clone(), Decor::Dim),
                cell_with(empty.clone(), Decor::Dim),
                cell_with(empty.clone(), Decor::Dim),
                cell_with(empty.clone(), Decor::Dim),
                cell_with(ARC_FUNCTIONS_NOTE.clone(), Decor::Dim),
            ]);
            for missed in missed_functions.iter().take(want_fn) {
                rows.push([
                    cell(shortened_file_text.clone()),
                    cell(ARC_LABEL_FUNC.clone()),
                    cell(format!("L{}", missed.line)),
                    cell(empty.clone()),
                    cell(empty.clone()),
                    cell(empty.clone()),
                    cell(empty.clone()),
                    cell(missed.name.clone()),
                ]);
            }
        }

        let want_br = ((row_budget as f64) * 0.2).ceil() as usize;
        let want_br = want_br.min(missed_branches.len());
        if want_br > 0 {
            rows.push([
                cell_with(shortened_file_text.clone(), Decor::Dim),
                cell_with(ARC_LABEL_BRANCHES.clone(), Decor::Dim),
                cell_with(empty.clone(), Decor::Dim),
                cell_with(empty.clone(), Decor::Dim),
                cell_with(empty.clone(), Decor::Dim),
                cell_with(empty.clone(), Decor::Dim),
                cell_with(empty.clone(), Decor::Dim),
                cell_with(ARC_BRANCHES_NOTE.clone(), Decor::Dim),
            ]);
            for missed in missed_branches.iter().take(want_br) {
                rows.push([
                    cell(shortened_file_text.clone()),
                    cell(ARC_LABEL_BRANCH.clone()),
                    cell(format!("L{}", missed.line)),
                    cell(empty.clone()),
                    cell(empty.clone()),
                    cell(empty.clone()),
                    cell(empty.clone()),
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
            let mut iter = uncovered_lines_iter(blocks).take(5000);
            while rows.len() < target {
                match iter.next() {
                    Some(ln) => rows.push([
                        cell(shortened_file_text.clone()),
                        cell(label_line.clone()),
                        cell(format!("L{ln}")),
                        cell(empty.clone()),
                        cell(empty.clone()),
                        cell(empty.clone()),
                        cell(empty.clone()),
                        cell(label_uncovered.clone()),
                    ]),
                    None => {
                        rows.push(blank_row.clone());
                    }
                }
            }
        }
    }

    write_table_with_frame_const::<8>(out, &layout.frame, &layout.columns, &layout.widths, &rows);
}

fn uncovered_lines_iter(blocks: &[super::model::UncoveredRange]) -> impl Iterator<Item = u32> + '_ {
    struct It<'a> {
        blocks: &'a [super::model::UncoveredRange],
        block_index: usize,
        next_line: Option<u32>,
    }

    impl<'a> Iterator for It<'a> {
        type Item = u32;
        fn next(&mut self) -> Option<Self::Item> {
            loop {
                if self.block_index >= self.blocks.len() {
                    return None;
                }
                let block = &self.blocks[self.block_index];
                let current = self.next_line.unwrap_or(block.start);
                if current > block.end {
                    self.block_index += 1;
                    self.next_line = None;
                    continue;
                }
                self.next_line = Some(current.saturating_add(1));
                return Some(current);
            }
        }
    }

    It {
        blocks,
        block_index: 0,
        next_line: None,
    }
}
