use rustc_lexer::TokenKind;

use super::lex::{is_trivia, lex_spans};
use super::types::{ParsedOuterAttr, RustFileMarkers, RustItemKind, TokenSpan};
use super::util::{is_ident_text, skip_trivia, skip_visibility};

pub(super) fn classify_rust_file_markers(source: &str) -> RustFileMarkers {
    let token_spans = lex_spans(source);
    classify_rust_file_markers_from_tokens(source, &token_spans)
}

fn classify_rust_file_markers_from_tokens(
    source: &str,
    token_spans: &[TokenSpan],
) -> RustFileMarkers {
    let mut block_depth = 0usize;
    let mut index = 0usize;

    let mut has_test_attr = false;
    let mut has_cfg_test = false;

    let mut pending_test_marker_attr = false;
    let mut pending_cfg_test = false;

    while index < token_spans.len() {
        let token = token_spans[index];
        if is_trivia(token.kind) {
            index += 1;
            continue;
        }

        if token.kind == TokenKind::OpenBrace {
            block_depth += 1;
            index += 1;
            continue;
        }
        if token.kind == TokenKind::CloseBrace {
            block_depth = block_depth.saturating_sub(1);
            index += 1;
            continue;
        }

        if block_depth != 0 {
            index += 1;
            continue;
        }

        if token.kind == TokenKind::Pound {
            if let Some((attr, next_index)) = parse_outer_attribute(source, token_spans, index) {
                pending_test_marker_attr |= attr.is_test_marker;
                pending_cfg_test |= attr.is_cfg_test;
                index = next_index;
                continue;
            }
        }

        let after_vis = skip_visibility(source, token_spans, index);
        if let Some(item_kind) = peek_item_kind(source, token_spans, after_vis) {
            if pending_cfg_test {
                has_cfg_test = true;
            }
            if matches!(
                item_kind,
                RustItemKind::Fn | RustItemKind::Mod | RustItemKind::Impl
            ) {
                has_test_attr |= pending_test_marker_attr | pending_cfg_test;
            }
            pending_test_marker_attr = false;
            pending_cfg_test = false;
        }

        index += 1;
    }

    RustFileMarkers {
        has_test_attr,
        has_cfg_test,
    }
}

fn peek_item_kind(source: &str, token_spans: &[TokenSpan], index: usize) -> Option<RustItemKind> {
    if is_ident_text(source, token_spans, index, "fn") {
        return Some(RustItemKind::Fn);
    }
    if is_ident_text(source, token_spans, index, "mod") {
        return Some(RustItemKind::Mod);
    }
    if is_ident_text(source, token_spans, index, "impl") {
        return Some(RustItemKind::Impl);
    }
    token_spans
        .get(index)
        .is_some()
        .then_some(RustItemKind::Other)
}

fn parse_outer_attribute(
    source: &str,
    token_spans: &[TokenSpan],
    pound_index: usize,
) -> Option<(ParsedOuterAttr, usize)> {
    if token_spans
        .get(pound_index)
        .is_none_or(|t| t.kind != TokenKind::Pound)
    {
        return None;
    }
    let not_index = skip_trivia(token_spans, pound_index + 1);
    if token_spans
        .get(not_index)
        .is_some_and(|t| t.kind == TokenKind::Not)
    {
        return None;
    }
    let bracket_index = skip_trivia(token_spans, pound_index + 1);
    if token_spans
        .get(bracket_index)
        .is_none_or(|t| t.kind != TokenKind::OpenBracket)
    {
        return None;
    }

    let mut index = skip_trivia(token_spans, bracket_index + 1);
    let mut last_path_segment: Option<&str> = None;
    let mut first_path_segment: Option<&str> = None;
    while index < token_spans.len() {
        let token = token_spans[index];
        if is_trivia(token.kind) {
            index += 1;
            continue;
        }
        if token.kind == TokenKind::CloseBracket {
            break;
        }
        if matches!(token.kind, TokenKind::Ident | TokenKind::RawIdent) {
            let seg = source.get(token.start..token.end)?;
            if first_path_segment.is_none() {
                first_path_segment = Some(seg);
            }
            last_path_segment = Some(seg);
            let after_ident = skip_trivia(token_spans, index + 1);
            if token_spans
                .get(after_ident)
                .is_some_and(|t| t.kind == TokenKind::Colon)
                && token_spans
                    .get(skip_trivia(token_spans, after_ident + 1))
                    .is_some_and(|t| t.kind == TokenKind::Colon)
            {
                index = skip_trivia(token_spans, after_ident + 2);
                continue;
            }
        }
        break;
    }

    let close_index = skip_to_matching_close_bracket(token_spans, bracket_index)?;
    let attr_name = last_path_segment.unwrap_or("");
    let is_test_marker = matches!(attr_name, "test" | "rstest");

    let is_cfg_test = first_path_segment.is_some_and(|s| s == "cfg")
        && attribute_contains_cfg_test(source, token_spans, bracket_index + 1, close_index);

    Some((
        ParsedOuterAttr {
            is_test_marker,
            is_cfg_test,
        },
        close_index + 1,
    ))
}

fn skip_to_matching_close_bracket(
    token_spans: &[TokenSpan],
    open_bracket_index: usize,
) -> Option<usize> {
    if token_spans
        .get(open_bracket_index)
        .is_none_or(|t| t.kind != TokenKind::OpenBracket)
    {
        return None;
    }
    let mut depth = 0usize;
    for (index, token) in token_spans.iter().enumerate().skip(open_bracket_index) {
        if token.kind == TokenKind::OpenBracket {
            depth += 1;
        } else if token.kind == TokenKind::CloseBracket {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return Some(index);
            }
        }
    }
    None
}

fn attribute_contains_cfg_test(
    source: &str,
    token_spans: &[TokenSpan],
    start_index: usize,
    close_bracket_index: usize,
) -> bool {
    let mut index = start_index;
    while index < close_bracket_index {
        let token = token_spans[index];
        if matches!(token.kind, TokenKind::Literal { .. }) {
            index += 1;
            continue;
        }
        if matches!(token.kind, TokenKind::Ident | TokenKind::RawIdent)
            && source
                .get(token.start..token.end)
                .is_some_and(|s| s == "test")
        {
            return true;
        }
        index += 1;
    }
    false
}
