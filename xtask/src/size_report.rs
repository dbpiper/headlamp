use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::Path;

use anyhow::{bail, Context};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrateSizeRow {
    pub crate_name: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SizeReport {
    pub crate_sizes: Vec<CrateSizeRow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapSymbol {
    pub address: u64,
    pub size_bytes: u64,
    pub object_index: u32,
    pub raw_symbol_name: String,
}

pub fn parse_map_file(map_path: &Path) -> anyhow::Result<SizeReport> {
    let object_index_to_path = parse_object_index_to_path(map_path)?;
    let object_index_to_symbol_bytes = parse_symbol_bytes_by_object(map_path)?;

    let mut crate_name_to_bytes: HashMap<String, u64> = HashMap::new();
    for (object_index, symbol_bytes) in object_index_to_symbol_bytes {
        let Some(object_path) = object_index_to_path.get(&object_index) else {
            continue;
        };
        let crate_name = crate_bucket_from_object_path(object_path);
        crate_name_to_bytes
            .entry(crate_name)
            .and_modify(|bytes| *bytes += symbol_bytes)
            .or_insert(symbol_bytes);
    }

    let mut crate_sizes = crate_name_to_bytes
        .into_iter()
        .map(|(crate_name, bytes)| CrateSizeRow { crate_name, bytes })
        .collect::<Vec<_>>();
    crate_sizes.sort_by(|left, right| right.bytes.cmp(&left.bytes));

    Ok(SizeReport { crate_sizes })
}

pub fn parse_map_symbols(map_path: &Path) -> anyhow::Result<Vec<MapSymbol>> {
    let file = std::fs::File::open(map_path)
        .with_context(|| format!("failed to open map file {}", map_path.display()))?;
    let mut reader = BufReader::new(file);

    let mut found_symbols_header = false;
    let mut line_buffer: Vec<u8> = Vec::new();
    let mut symbols: Vec<MapSymbol> = Vec::new();

    loop {
        line_buffer.clear();
        let bytes_read = reader
            .read_until(b'\n', &mut line_buffer)
            .context("failed to read map file bytes")?;
        if bytes_read == 0 {
            break;
        }

        let line_text = String::from_utf8_lossy(&line_buffer);
        let trimmed = line_text.trim();

        if !found_symbols_header {
            if trimmed == "# Symbols:" {
                found_symbols_header = true;
            }
            continue;
        }

        if !trimmed.starts_with("0x") {
            continue;
        }

        let Some(symbol) = parse_symbol_line(trimmed) else {
            continue;
        };
        symbols.push(symbol);
    }

    if !found_symbols_header {
        bail!("map file did not contain '# Symbols:' section");
    }

    Ok(symbols)
}

pub fn parse_object_index_to_path(map_path: &Path) -> anyhow::Result<HashMap<u32, String>> {
    parse_object_index_to_path_impl(map_path)
}

fn parse_object_index_to_path_impl(map_path: &Path) -> anyhow::Result<HashMap<u32, String>> {
    let file = std::fs::File::open(map_path)
        .with_context(|| format!("failed to open map file {}", map_path.display()))?;
    let mut reader = BufReader::new(file);

    let mut found_object_files_header = false;
    let mut object_index_to_path: HashMap<u32, String> = HashMap::new();

    let mut line_buffer: Vec<u8> = Vec::new();
    loop {
        line_buffer.clear();
        let bytes_read = reader
            .read_until(b'\n', &mut line_buffer)
            .context("failed to read map file bytes")?;
        if bytes_read == 0 {
            break;
        }
        let line_text = String::from_utf8_lossy(&line_buffer);
        let trimmed = line_text.trim();

        if !found_object_files_header {
            if trimmed == "# Object files:" {
                found_object_files_header = true;
            }
            continue;
        }

        if trimmed == "# Sections:" {
            break;
        }

        let Some((object_index, path_text)) = parse_object_table_line(trimmed) else {
            continue;
        };
        object_index_to_path.insert(object_index, path_text.to_string());
    }

    if !found_object_files_header {
        bail!("map file did not contain '# Object files:' section");
    }

    Ok(object_index_to_path)
}

fn parse_object_table_line(line: &str) -> Option<(u32, &str)> {
    let (object_index, right) = parse_bracketed_index_prefix(line)?;
    Some((object_index, right.trim_start()))
}

fn parse_symbol_bytes_by_object(map_path: &Path) -> anyhow::Result<HashMap<u32, u64>> {
    let file = std::fs::File::open(map_path)
        .with_context(|| format!("failed to open map file {}", map_path.display()))?;
    let mut reader = BufReader::new(file);

    let mut found_symbols_header = false;
    let mut object_index_to_symbol_bytes: HashMap<u32, u64> = HashMap::new();

    let mut line_buffer: Vec<u8> = Vec::new();
    loop {
        line_buffer.clear();
        let bytes_read = reader
            .read_until(b'\n', &mut line_buffer)
            .context("failed to read map file bytes")?;
        if bytes_read == 0 {
            break;
        }
        let line_text = String::from_utf8_lossy(&line_buffer);
        let trimmed = line_text.trim();

        if !found_symbols_header {
            if trimmed == "# Symbols:" {
                found_symbols_header = true;
            }
            continue;
        }

        if !trimmed.starts_with("0x") {
            continue;
        }

        let Some((object_index, symbol_bytes)) = parse_symbol_line_object_and_size(trimmed) else {
            continue;
        };

        object_index_to_symbol_bytes
            .entry(object_index)
            .and_modify(|bytes| *bytes += symbol_bytes)
            .or_insert(symbol_bytes);
    }

    if !found_symbols_header {
        bail!("map file did not contain '# Symbols:' section");
    }

    Ok(object_index_to_symbol_bytes)
}

fn parse_symbol_line_object_and_size(line: &str) -> Option<(u32, u64)> {
    let mut whitespace_split = line.split_whitespace();
    let _address_token = whitespace_split.next()?;
    let size_token = whitespace_split.next()?;
    let symbol_bytes = parse_0x_prefixed_hex_u64(size_token)?;

    let (object_index, _rest) = parse_bracketed_index_anywhere(line)?;
    Some((object_index, symbol_bytes))
}

fn parse_symbol_line(line: &str) -> Option<MapSymbol> {
    let mut whitespace_split = line.split_whitespace();
    let address_token = whitespace_split.next()?;
    let size_token = whitespace_split.next()?;

    let address = parse_0x_prefixed_hex_u64(address_token)?;
    let size_bytes = parse_0x_prefixed_hex_u64(size_token)?;
    let (object_index, remainder) = parse_bracketed_index_anywhere(line)?;
    let raw_symbol_name = remainder.trim().to_string();

    Some(MapSymbol {
        address,
        size_bytes,
        object_index,
        raw_symbol_name,
    })
}

fn parse_0x_prefixed_hex_u64(token: &str) -> Option<u64> {
    let hex = token.strip_prefix("0x")?;
    u64::from_str_radix(hex, 16).ok()
}

fn parse_bracketed_index_prefix(line: &str) -> Option<(u32, &str)> {
    let trimmed_start = line.trim_start();
    let open_bracket_offset = trimmed_start.find('[')?;
    if open_bracket_offset != 0 {
        return None;
    }

    let close_bracket_offset = trimmed_start.find(']')?;
    let inside = &trimmed_start[(open_bracket_offset + 1)..close_bracket_offset];
    let object_index = parse_digits_as_u32(inside)?;
    let remainder = &trimmed_start[(close_bracket_offset + 1)..];
    Some((object_index, remainder))
}

fn parse_bracketed_index_anywhere(line: &str) -> Option<(u32, &str)> {
    let open_bracket_offset = line.find('[')?;
    let close_bracket_offset = line[open_bracket_offset..].find(']')? + open_bracket_offset;
    let inside = &line[(open_bracket_offset + 1)..close_bracket_offset];
    let object_index = parse_digits_as_u32(inside)?;
    let remainder = &line[(close_bracket_offset + 1)..];
    Some((object_index, remainder))
}

fn parse_digits_as_u32(text: &str) -> Option<u32> {
    let digits = text
        .chars()
        .filter(|character| character.is_ascii_digit())
        .collect::<String>();
    if digits.is_empty() {
        return None;
    }
    digits.parse::<u32>().ok()
}

fn crate_bucket_from_object_path(object_path: &str) -> String {
    if object_path.contains("/rustlib/") {
        return "rust-stdlib".to_string();
    }
    if object_path == "linker synthesized" {
        return "linker".to_string();
    }

    if let Some(crate_name) = crate_name_from_rlib_reference(object_path) {
        return crate_name.to_string();
    }

    if object_path.contains("/target/")
        && object_path.contains("/deps/")
        && object_path.contains("headlamp-")
    {
        return "headlamp(bin)".to_string();
    }

    object_path.to_string()
}

fn crate_name_from_rlib_reference(object_path: &str) -> Option<&str> {
    let filename = object_path.rsplit('/').next().unwrap_or(object_path);
    let rlib_offset = filename.find(".rlib")?;
    let before_rlib = &filename[..rlib_offset];
    let after_lib_prefix = before_rlib.strip_prefix("lib")?;
    let hash_separator_offset = after_lib_prefix.rfind('-')?;
    Some(&after_lib_prefix[..hash_separator_offset])
}
