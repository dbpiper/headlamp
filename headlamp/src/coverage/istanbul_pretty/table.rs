use crate::format::ansi;
use std::sync::Arc;
use std::sync::LazyLock;

static SPACE_SLAB: LazyLock<String> = LazyLock::new(|| " ".repeat(1024));

fn push_spaces(out: &mut String, mut count: usize) {
    while count > 0 {
        let chunk = count.min(SPACE_SLAB.len());
        out.push_str(&SPACE_SLAB.as_str()[..chunk]);
        count -= chunk;
    }
}

#[derive(Debug, Clone)]
pub struct TableFrame {
    pub hr_top: String,
    pub hr_sep: String,
    pub hr_bot: String,
    pub header: String,
    pub blank_row: String,
}

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
}

#[derive(Debug, Clone)]
pub struct Cell {
    pub raw: Arc<str>,
    pub decor: Decor,
    pub href: Option<String>,
}

pub fn cell(raw: impl Into<Arc<str>>) -> Cell {
    Cell {
        raw: raw.into(),
        decor: Decor::None,
        href: None,
    }
}

pub fn cell_with(raw: impl Into<Arc<str>>, decor: Decor) -> Cell {
    Cell {
        raw: raw.into(),
        decor,
        href: None,
    }
}

pub fn compute_column_widths(total_columns: usize, columns: &[ColumnSpec]) -> Vec<usize> {
    let mins = columns.iter().map(|c| c.min).collect::<Vec<_>>();
    let maxs = columns.iter().map(|c| c.max).collect::<Vec<_>>();
    super::column_widths::compute_column_widths(total_columns, &mins, &maxs, columns.len())
}

pub fn build_table_frame(columns: &[ColumnSpec], widths: &[usize]) -> TableFrame {
    fn build_hr(left: char, mid: char, right: char, widths: &[usize]) -> String {
        let mut out = String::new();
        out.push(left);
        for (index, width) in widths.iter().enumerate() {
            if index > 0 {
                out.push(mid);
            }
            out.extend(std::iter::repeat_n('─', *width));
        }
        out.push(right);
        out
    }

    let hr_top = build_hr('┌', '┬', '┐', widths);
    let hr_sep = build_hr('┼', '┼', '┼', widths);
    let hr_bot = build_hr('└', '┴', '┘', widths);

    let mut header = String::new();
    header.push('│');
    for (index, (column, width)) in columns.iter().zip(widths.iter()).enumerate() {
        if index > 0 {
            header.push('│');
        }
        header.push_str(&ansi::bold(&pad_visible(
            column.label,
            *width,
            column.align_right,
        )));
    }
    header.push('│');

    let mut blank_row = String::new();
    blank_row.push('│');
    for (index, width) in widths.iter().enumerate() {
        if index > 0 {
            blank_row.push('│');
        }
        push_spaces(&mut blank_row, *width);
    }
    blank_row.push('│');

    TableFrame {
        hr_top,
        hr_sep,
        hr_bot,
        header,
        blank_row,
    }
}

pub fn write_table_with_frame_const<const N: usize>(
    out: &mut String,
    frame: &TableFrame,
    columns: &[ColumnSpec],
    widths: &[usize],
    rows: &[[Cell; N]],
) {
    out.push_str(&frame.hr_top);
    out.push('\n');
    out.push_str(&frame.header);
    out.push('\n');
    out.push_str(&frame.hr_sep);
    out.push('\n');
    for (row_index, row) in rows.iter().enumerate() {
        if row_index > 0 {
            out.push('\n');
        }
        if is_blank_row(row) {
            out.push_str(&frame.blank_row);
            continue;
        }
        out.push('│');
        for (cell_index, cell) in row.iter().enumerate() {
            if cell_index > 0 {
                out.push('│');
            }
            let width = widths.get(cell_index).copied().unwrap_or(1);
            let col = columns.get(cell_index).unwrap();
            write_cell_fast(out, cell, width, col.align_right);
        }
        out.push('│');
    }
    out.push('\n');
    out.push_str(&frame.hr_bot);
}

fn is_blank_row<const N: usize>(row: &[Cell; N]) -> bool {
    row.iter().all(|cell| {
        cell.href.is_none() && matches!(cell.decor, Decor::None) && cell.raw.as_ref().is_empty()
    })
}

fn write_cell_fast(out: &mut String, cell: &Cell, width: usize, align_right: bool) {
    let raw = cell.raw.as_ref();
    if raw.is_ascii() {
        write_cell_ascii(out, cell, raw, width, align_right);
        return;
    }

    // Fallback for non-ASCII: preserve behavior exactly, even if it's slower.
    let padded = pad_visible(raw, width, align_right);
    let decorated = apply_decor(&cell.decor, &padded);
    out.push_str(&apply_href(cell.href.as_deref(), &decorated));
}

fn write_cell_ascii(out: &mut String, cell: &Cell, raw: &str, width: usize, align_right: bool) {
    fn write_osc8_start(out: &mut String, url: &str) {
        out.push_str("\u{1b}]8;;");
        out.push_str(url);
        out.push('\u{7}');
    }
    fn write_osc8_end(out: &mut String) {
        out.push_str("\u{1b}]8;;\u{7}");
    }

    let href = cell.href.as_deref();
    if let Some(url) = href {
        write_osc8_start(out, url);
    }

    match &cell.decor {
        Decor::None => {
            write_padded_ascii(out, raw, width, align_right);
        }
        Decor::Bold => {
            out.push_str("\u{1b}[1m");
            write_padded_ascii(out, raw, width, align_right);
            out.push_str("\u{1b}[22m");
        }
        Decor::Dim => {
            out.push_str("\u{1b}[2m");
            write_padded_ascii(out, raw, width, align_right);
            out.push_str("\u{1b}[22m");
        }
        Decor::TintPct { pct } => {
            // Match `bars::tint_pct(pct, padded)` exactly (uses \x1b[0m reset when enabled).
            let maybe_prefix = super::bars::rgb_prefix_for_pct_for_table(*pct);
            if let Some(prefix) = maybe_prefix {
                out.push_str(prefix);
            }
            write_padded_ascii(out, raw, width, align_right);
            if maybe_prefix.is_some() {
                out.push_str("\u{1b}[0m");
            }
        }
        Decor::Bar { pct } => {
            // Preserve behavior: Bar ignores the raw content and renders the bar at the cell width.
            super::bars::write_bar(out, *pct, width);
        }
    }

    if href.is_some() {
        write_osc8_end(out);
    }
}

fn write_padded_ascii(out: &mut String, text: &str, width: usize, align_right: bool) {
    let slice = text.get(..width).unwrap_or(text);
    let len = slice.len();
    if len >= width {
        out.push_str(slice);
        return;
    }
    let pad_len = width - len;
    if align_right {
        push_spaces(out, pad_len);
        out.push_str(slice);
    } else {
        out.push_str(slice);
        push_spaces(out, pad_len);
    }
}

fn pad_visible(text: &str, width: usize, align_right: bool) -> String {
    let len = if text.is_ascii() {
        text.len()
    } else {
        text.chars().count()
    };
    if len == width {
        return text.to_string();
    }
    if len < width {
        let pad_len = width - len;
        let mut out = String::with_capacity(width);
        if align_right {
            push_spaces(&mut out, pad_len);
            out.push_str(text);
            return out;
        }
        out.push_str(text);
        push_spaces(&mut out, pad_len);
        return out;
    }
    if text.is_ascii() {
        return text.get(..width).unwrap_or(text).to_string();
    }
    text.chars().take(width).collect::<String>()
}

fn apply_decor(decor: &Decor, padded: &str) -> String {
    match decor {
        Decor::None => padded.to_string(),
        Decor::Bold => ansi::bold(padded),
        Decor::Dim => ansi::dim(padded),
        Decor::TintPct { pct } => super::bars::tint_pct(*pct, padded),
        Decor::Bar { pct } => {
            let width = if padded.is_ascii() {
                padded.len()
            } else {
                padded.chars().count()
            };
            super::bars::bar(*pct, width)
        }
    }
}

fn apply_href(href: Option<&str>, decorated: &str) -> String {
    href.map_or_else(|| decorated.to_string(), |url| ansi::osc8(decorated, url))
}
