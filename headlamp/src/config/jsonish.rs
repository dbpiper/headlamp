use super::HeadlampConfig;
use serde::de::DeserializeOwned;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContextKind {
    Object,
    Array,
}

pub(super) fn parse_jsonish_config(raw: &str) -> Result<HeadlampConfig, serde_json::Error> {
    parse_jsonish::<HeadlampConfig>(raw)
}

pub(crate) fn parse_jsonish_value(raw: &str) -> Result<serde_json::Value, serde_json::Error> {
    parse_jsonish::<serde_json::Value>(raw)
}

pub(crate) fn parse_jsonish<T: DeserializeOwned>(raw: &str) -> Result<T, serde_json::Error> {
    let normalized = normalize_jsonish_to_json(raw);
    serde_json::from_str::<T>(&normalized)
}

fn normalize_jsonish_to_json(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = String::with_capacity(raw.len());
    let mut context_stack: Vec<ContextKind> = vec![];
    let mut expecting_object_key = false;

    let mut index = 0usize;
    while index < bytes.len() {
        let byte = bytes[index];

        if let Some(next_index) = try_skip_comment(bytes, index) {
            index = next_index;
            continue;
        }
        if byte == b'"' || byte == b'\'' {
            let (literal, next_index) = take_string_literal(raw, index, byte);
            out.push_str(&literal);
            index = next_index;
            continue;
        }
        if byte.is_ascii_whitespace() {
            out.push(byte as char);
            index += 1;
            continue;
        }
        if let Some(next_index) =
            try_emit_quoted_bare_key(bytes, raw, index, expecting_object_key, &mut out)
        {
            index = next_index;
            continue;
        }
        if byte == b',' && is_trailing_comma(bytes, index) {
            index += 1;
            continue;
        }
        if handle_context_punct(
            byte,
            &mut context_stack,
            &mut expecting_object_key,
            &mut out,
        ) {
            index += 1;
            continue;
        }

        out.push(byte as char);
        index += 1;
    }

    out
}

fn try_skip_comment(bytes: &[u8], index: usize) -> Option<usize> {
    if bytes.get(index).copied() != Some(b'/') {
        return None;
    }
    match (bytes.get(index + 1).copied(), bytes.get(index + 2).copied()) {
        (Some(b'/'), _) => Some(skip_line_comment(bytes, index + 2)),
        (Some(b'*'), _) => Some(skip_block_comment(bytes, index + 2)),
        _ => None,
    }
}

fn try_emit_quoted_bare_key(
    bytes: &[u8],
    raw: &str,
    index: usize,
    expecting_object_key: bool,
    out: &mut String,
) -> Option<usize> {
    if !expecting_object_key {
        return None;
    }
    let byte = *bytes.get(index)?;
    if !is_ident_start(byte) {
        return None;
    }
    let (key_text, after_key) = take_identifier(raw, index)?;
    let lookahead = skip_ws(bytes, after_key);
    if bytes.get(lookahead).copied()? != b':' {
        return None;
    }
    out.push('"');
    out.push_str(key_text);
    out.push('"');
    Some(after_key)
}

fn is_trailing_comma(bytes: &[u8], comma_index: usize) -> bool {
    let lookahead = skip_ws_and_comments(bytes, comma_index + 1);
    matches!(bytes.get(lookahead).copied(), Some(b'}' | b']'))
}

fn skip_ws(bytes: &[u8], mut index: usize) -> usize {
    while index < bytes.len() && bytes[index].is_ascii_whitespace() {
        index += 1;
    }
    index
}

fn skip_ws_and_comments(bytes: &[u8], mut index: usize) -> usize {
    loop {
        index = skip_ws(bytes, index);
        if let Some(next) = try_skip_comment(bytes, index) {
            index = next;
            continue;
        }
        return index;
    }
}

fn handle_context_punct(
    byte: u8,
    context_stack: &mut Vec<ContextKind>,
    expecting_object_key: &mut bool,
    out: &mut String,
) -> bool {
    match byte {
        b'{' => {
            context_stack.push(ContextKind::Object);
            *expecting_object_key = true;
            out.push('{');
            true
        }
        b'}' => {
            let _ = context_stack.pop();
            *expecting_object_key = false;
            out.push('}');
            true
        }
        b'[' => {
            context_stack.push(ContextKind::Array);
            *expecting_object_key = false;
            out.push('[');
            true
        }
        b']' => {
            let _ = context_stack.pop();
            *expecting_object_key = false;
            out.push(']');
            true
        }
        b':' => {
            *expecting_object_key = false;
            out.push(':');
            true
        }
        b',' => {
            *expecting_object_key = matches!(context_stack.last(), Some(ContextKind::Object));
            out.push(',');
            true
        }
        _ => false,
    }
}

fn skip_line_comment(bytes: &[u8], mut index: usize) -> usize {
    while index < bytes.len() {
        if bytes[index] == b'\n' {
            return index;
        }
        index += 1;
    }
    index
}

fn skip_block_comment(bytes: &[u8], mut index: usize) -> usize {
    while index + 1 < bytes.len() {
        if bytes[index] == b'*' && bytes[index + 1] == b'/' {
            return index + 2;
        }
        index += 1;
    }
    bytes.len()
}

fn take_string_literal(source: &str, start: usize, quote: u8) -> (String, usize) {
    if quote == b'\'' {
        return take_single_quoted_string_as_json_double(source, start);
    }
    take_double_quoted_string(source, start)
}

fn take_double_quoted_string(source: &str, start: usize) -> (String, usize) {
    let bytes = source.as_bytes();
    let mut out = String::new();
    out.push('"');
    let mut index = start + 1;
    while index < bytes.len() {
        let byte = bytes[index];
        out.push(byte as char);
        index += 1;
        if byte == b'\\' && index < bytes.len() {
            out.push(bytes[index] as char);
            index += 1;
            continue;
        }
        if byte == b'"' {
            break;
        }
    }
    (out, index)
}

fn take_single_quoted_string_as_json_double(source: &str, start: usize) -> (String, usize) {
    let bytes = source.as_bytes();
    let mut out = String::new();
    out.push('"');
    let mut index = start + 1;
    while index < bytes.len() {
        let byte = bytes[index];
        index += 1;
        if byte == b'\\' && index < bytes.len() {
            let escaped = bytes[index];
            index += 1;
            if escaped == b'\'' {
                out.push('\'');
                continue;
            }
            out.push('\\');
            out.push(escaped as char);
            continue;
        }
        if byte == b'\'' {
            out.push('"');
            break;
        }
        if byte == b'"' {
            out.push('\\');
            out.push('"');
            continue;
        }
        out.push(byte as char);
    }
    (out, index)
}

fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte == b'$'
}

fn is_ident_continue(byte: u8) -> bool {
    is_ident_start(byte) || byte.is_ascii_digit()
}

fn take_identifier(source: &str, start: usize) -> Option<(&str, usize)> {
    let bytes = source.as_bytes();
    if start >= bytes.len() || !is_ident_start(bytes[start]) {
        return None;
    }
    let mut index = start + 1;
    while index < bytes.len() && is_ident_continue(bytes[index]) {
        index += 1;
    }
    Some((&source[start..index], index))
}
