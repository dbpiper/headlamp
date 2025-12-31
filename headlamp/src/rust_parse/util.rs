use rustc_lexer::TokenKind;

use super::lex::is_trivia;
use super::types::TokenSpan;

pub(super) fn is_ident_text(
    source: &str,
    token_spans: &[TokenSpan],
    index: usize,
    text: &str,
) -> bool {
    let Some(span) = token_spans.get(index) else {
        return false;
    };
    matches!(span.kind, TokenKind::Ident | TokenKind::RawIdent)
        && source.get(span.start..span.end).is_some_and(|s| s == text)
}

pub(super) fn skip_visibility(source: &str, token_spans: &[TokenSpan], index: usize) -> usize {
    if !is_ident_text(source, token_spans, index, "pub") {
        return index;
    }
    let after_pub = skip_trivia(token_spans, index + 1);
    if token_spans
        .get(after_pub)
        .is_some_and(|t| t.kind == TokenKind::OpenParen)
    {
        return skip_balanced(
            token_spans,
            after_pub,
            TokenKind::OpenParen,
            TokenKind::CloseParen,
        )
        .unwrap_or(after_pub + 1);
    }
    after_pub
}

pub(super) fn skip_trivia(token_spans: &[TokenSpan], mut index: usize) -> usize {
    while token_spans.get(index).is_some_and(|t| is_trivia(t.kind)) {
        index += 1;
    }
    index
}

pub(super) fn skip_balanced(
    token_spans: &[TokenSpan],
    open_index: usize,
    open_kind: TokenKind,
    close_kind: TokenKind,
) -> Option<usize> {
    if token_spans
        .get(open_index)
        .is_none_or(|t| t.kind != open_kind)
    {
        return None;
    }
    let mut depth = 0usize;
    let mut index = open_index;
    while index < token_spans.len() {
        let kind = token_spans[index].kind;
        if kind == open_kind {
            depth += 1;
        } else if kind == close_kind {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return Some(index + 1);
            }
        }
        index += 1;
    }
    None
}

pub(super) fn unescape_rust_string_literal(literal_text: &str) -> Option<String> {
    let text = literal_text.trim();
    if text.starts_with('\"') && text.ends_with('\"') && text.len() >= 2 {
        return Some(unescape_basic(&text[1..text.len() - 1]));
    }
    if let Some(stripped) = text
        .strip_prefix("r#\"")
        .and_then(|s| s.strip_suffix("\"#"))
    {
        return Some(stripped.to_string());
    }
    if let Some(stripped) = text.strip_prefix("r\"").and_then(|s| s.strip_suffix('\"')) {
        return Some(stripped.to_string());
    }
    None
}

fn unescape_basic(inner: &str) -> String {
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('\\') => out.push('\\'),
            Some('\"') => out.push('\"'),
            Some(other) => out.push(other),
            None => break,
        }
    }
    out
}
