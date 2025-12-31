use object::Object;
use object::ObjectSection;
use object::ObjectSegment;
use object::ObjectSymbol;

use crate::model::AttributionSource;

pub fn function_ranges(file: &object::File<'_>) -> Option<(AttributionSource, Vec<(u64, u64)>)> {
    let (text_segment_address, text_segment_end) = file.segments().find_map(|segment| {
        segment
            .name()
            .ok()
            .flatten()
            .filter(|name| *name == "__TEXT")
            .map(|_| {
                (
                    segment.address(),
                    segment.address().saturating_add(segment.size()),
                )
            })
    })?;

    if let Some((source, starts)) =
        function_starts_from_section(file, text_segment_address, text_segment_end)
    {
        let ranges = build_ranges_from_starts(&starts, text_segment_end);
        if !ranges.is_empty() {
            return Some((source, ranges));
        }
    }

    let mut starts = file
        .symbols()
        .chain(file.dynamic_symbols())
        .filter(|symbol| symbol.is_definition())
        .filter(|symbol| symbol.kind() == object::SymbolKind::Text)
        .map(|symbol| symbol.address())
        .filter(|address| *address >= text_segment_address && *address < text_segment_end)
        .collect::<Vec<_>>();
    starts.sort_unstable();
    starts.dedup();
    if starts.is_empty() {
        return None;
    }

    let ranges = build_ranges_from_starts(&starts, text_segment_end);
    if ranges.is_empty() {
        return None;
    }
    Some((AttributionSource::MachOTextSymbols, ranges))
}

fn function_starts_from_section(
    file: &object::File<'_>,
    text_segment_address: u64,
    text_segment_end: u64,
) -> Option<(AttributionSource, Vec<u64>)> {
    let section = file
        .section_by_name("__TEXT,__function_starts")
        .or_else(|| file.section_by_name("__TEXT,__func_starts"))?;
    let data = section.data().ok()?;
    let offsets = decode_macho_function_start_deltas(data);
    if offsets.is_empty() {
        return None;
    }

    let mut starts = offsets
        .into_iter()
        .map(|offset| text_segment_address.saturating_add(offset))
        .filter(|start| *start >= text_segment_address && *start < text_segment_end)
        .collect::<Vec<_>>();
    starts.sort_unstable();
    starts.dedup();
    if starts.is_empty() {
        return None;
    }

    Some((AttributionSource::MachOFunctionStarts, starts))
}

pub fn build_ranges_from_starts(starts: &[u64], end_address: u64) -> Vec<(u64, u64)> {
    if starts.is_empty() {
        return Vec::new();
    }
    let mut ranges = Vec::with_capacity(starts.len());
    for index in 0..starts.len() {
        let start = starts[index];
        let end = starts
            .get(index + 1)
            .copied()
            .unwrap_or(end_address)
            .min(end_address);
        if end > start {
            ranges.push((start, end));
        }
    }
    ranges
}

pub fn decode_macho_function_start_deltas(data: &[u8]) -> Vec<u64> {
    let mut cursor = 0usize;
    let mut current = 0u64;
    let mut out = Vec::new();

    loop {
        let (value, read) = decode_uleb128(&data[cursor..]);
        if read == 0 {
            break;
        }
        cursor += read;
        if value == 0 {
            break;
        }
        current = current.saturating_add(value);
        out.push(current);
        if cursor >= data.len() {
            break;
        }
    }
    out
}

pub fn decode_uleb128(data: &[u8]) -> (u64, usize) {
    let mut result = 0u64;
    let mut shift = 0u32;
    let mut index = 0usize;
    while index < data.len() {
        let byte = data[index];
        let low = (byte & 0x7f) as u64;
        if shift >= 64 {
            return (0, 0);
        }
        result |= low << shift;
        index += 1;
        if (byte & 0x80) == 0 {
            return (result, index);
        }
        shift = shift.saturating_add(7);
    }
    (0, 0)
}
