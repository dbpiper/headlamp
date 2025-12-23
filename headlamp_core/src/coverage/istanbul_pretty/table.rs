use crate::format::ansi;

#[derive(Debug, Clone)]
pub struct ColumnSpec {
    pub label: &'static str,
    pub min: usize,
    pub max: usize,
    pub align_right: bool,
}

#[derive(Debug, Clone)]
pub enum Decor {
    None,
    Bold,
    Dim,
    TintPct { pct: f64 },
    Bar { pct: f64 },
    DimShortenPath { rel: String },
    ShortenPath { rel: String },
}

#[derive(Debug, Clone)]
pub struct Cell {
    pub raw: String,
    pub decor: Decor,
}

pub fn cell(raw: impl Into<String>) -> Cell {
    Cell {
        raw: raw.into(),
        decor: Decor::None,
    }
}

pub fn cell_with(raw: impl Into<String>, decor: Decor) -> Cell {
    Cell {
        raw: raw.into(),
        decor,
    }
}

pub fn render_table(total_columns: usize, columns: &[ColumnSpec], rows: &[Vec<Cell>]) -> String {
    let mins = columns.iter().map(|c| c.min).collect::<Vec<_>>();
    let maxs = columns.iter().map(|c| c.max).collect::<Vec<_>>();
    let widths =
        super::column_widths::compute_column_widths(total_columns, &mins, &maxs, columns.len());

    let hr = |left: &str, mid: &str, right: &str| {
        let inner = widths
            .iter()
            .map(|w| "─".repeat(*w))
            .collect::<Vec<_>>()
            .join(mid);
        format!("{left}{inner}{right}")
    };

    let hr_top = hr("┌", "┬", "┐");
    let hr_sep = hr("┼", "┼", "┼");
    let hr_bot = hr("└", "┴", "┘");
    let header = format!(
        "│{}│",
        columns
            .iter()
            .zip(widths.iter())
            .map(|(c, w)| ansi::bold(&pad_visible(c.label, *w, c.align_right)))
            .collect::<Vec<_>>()
            .join("│")
    );

    let lines = rows
        .iter()
        .map(|row| {
            let cells = row
                .iter()
                .enumerate()
                .map(|(i, cell)| {
                    let width = widths.get(i).copied().unwrap_or(1);
                    let col = columns.get(i).unwrap();
                    let padded = pad_visible(&cell.raw, width, col.align_right);
                    apply_decor(&cell.decor, &padded)
                })
                .collect::<Vec<_>>();
            format!("│{}│", cells.join("│"))
        })
        .collect::<Vec<_>>();

    [hr_top, header, hr_sep]
        .into_iter()
        .chain(lines)
        .chain([hr_bot])
        .collect::<Vec<_>>()
        .join("\n")
}

fn pad_visible(text: &str, width: usize, align_right: bool) -> String {
    let len = text.chars().count();
    if len == width {
        return text.to_string();
    }
    if len < width {
        let pad = " ".repeat(width - len);
        return if align_right {
            format!("{pad}{text}")
        } else {
            format!("{text}{pad}")
        };
    }
    text.chars().take(width).collect::<String>()
}

fn apply_decor(decor: &Decor, padded: &str) -> String {
    match decor {
        Decor::None => padded.to_string(),
        Decor::Bold => ansi::bold(padded),
        Decor::Dim => ansi::dim(padded),
        Decor::TintPct { pct } => super::bars::tint_pct(*pct, padded),
        Decor::Bar { pct } => super::bars::bar(*pct, padded.chars().count()),
        Decor::DimShortenPath { rel } => {
            let display = super::path_shorten::shorten_path_preserving_filename(rel, padded.len());
            ansi::dim(&pad_visible(&display, padded.len(), false))
        }
        Decor::ShortenPath { rel } => {
            let display = super::path_shorten::shorten_path_preserving_filename(rel, padded.len());
            pad_visible(&display, padded.len(), false)
        }
    }
}
