use std::collections::HashSet;

use rustc_lexer::TokenKind;

use super::lex::{is_trivia, lex_spans};
use super::types::TokenSpan;
use super::util::{is_ident_text, skip_trivia, skip_visibility, unescape_rust_string_literal};

pub(super) fn extract_import_specs_from_source(source: &str) -> Vec<String> {
    if should_use_fast_path(source) {
        return extract_import_specs_fast_path(source);
    }

    let token_spans = lex_spans(source);
    let mut out = collect_import_specs(source, &token_spans)
        .into_iter()
        .collect::<Vec<_>>();
    out.sort();
    out
}

fn should_use_fast_path(source: &str) -> bool {
    // Fast path is for "flat" files that only contain simple top-level:
    //   - `use foo::bar;`
    //   - `mod a;` / `pub mod a;`
    //
    // Anything involving braces (use groups), globs, path attributes, or renames falls back to
    // the lexer-based parser for correctness.
    !(source.contains('{')
        || source.contains('}')
        || source.contains('#')
        || source.contains('*')
        || source.contains(" as ")
        || source.contains("\tas ")
        || source.contains(" as\t"))
}

fn extract_import_specs_fast_path(source: &str) -> Vec<String> {
    let mut use_specs: HashSet<&str> = HashSet::new();
    let mut mod_specs: HashSet<&str> = HashSet::new();

    for line in source.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            continue;
        }

        if let Some(rest) = strip_simple_use_prefix(trimmed) {
            if let Some(spec) = rest
                .split(';')
                .next()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                let spec = spec.trim_start_matches("::").trim();
                if !spec.is_empty() {
                    use_specs.insert(spec);
                }
            }
            continue;
        }

        if let Some(mod_name) = strip_simple_mod_decl(trimmed) {
            mod_specs.insert(mod_name);
        }
    }

    let mut out: Vec<String> = Vec::with_capacity(use_specs.len() + mod_specs.len());
    use_specs
        .into_iter()
        .for_each(|spec| out.push(spec.to_string()));
    mod_specs
        .into_iter()
        .for_each(|name| out.push(format!("self::{name}")));

    out.sort();
    out
}

fn strip_simple_use_prefix(line: &str) -> Option<&str> {
    // Accept:
    // - `use ...;`
    // - `pub use ...;`
    // - `pub(crate) use ...;` etc (we just search for the first `use ` token at the start level)
    if let Some(rest) = line.strip_prefix("use ") {
        return Some(rest);
    }
    if let Some(rest) = line.strip_prefix("pub ") {
        let rest = rest.trim_start();
        return rest.strip_prefix("use ").or_else(|| {
            // `pub(...) use ...`
            let after_paren = rest.strip_prefix("pub")?;
            let after_paren = after_paren.trim_start();
            after_paren.strip_prefix("use ")
        });
    }
    // `pub(crate) use ...` doesn't start with "pub " after trim_start, it starts with "pub(".
    if let Some(rest) = line.strip_prefix("pub(") {
        // Find the closing ')' then look for `use `.
        let after = rest.split_once(')')?.1.trim_start();
        return after.strip_prefix("use ");
    }
    None
}

fn strip_simple_mod_decl(line: &str) -> Option<&str> {
    // Accept:
    // - `mod a;`
    // - `pub mod a;`
    // - `pub(crate) mod a;`
    // Ignore inline module bodies (`mod a { ... }`) via fast-path predicate.
    let after = if let Some(rest) = line.strip_prefix("mod ") {
        rest
    } else if let Some(rest) = line.strip_prefix("pub mod ") {
        rest
    } else if let Some(rest) = line.strip_prefix("pub(") {
        let after = rest.split_once(')')?.1.trim_start();
        after.strip_prefix("mod ")?
    } else {
        return None;
    };
    let name = after.split(';').next().map(str::trim).unwrap_or("");
    (!name.is_empty()).then_some(name)
}

fn collect_import_specs(source: &str, token_spans: &[TokenSpan]) -> HashSet<String> {
    let mut import_specs: HashSet<String> = HashSet::new();
    let mut index = 0usize;
    let mut block_depth = 0usize;
    let mut pending_mod_path: Option<String> = None;

    while index < token_spans.len() {
        let token = token_spans[index];

        if is_trivia(token.kind) {
            index += 1;
            continue;
        }

        if update_block_depth(token.kind, &mut block_depth) {
            index += 1;
            continue;
        }

        if block_depth != 0 {
            index += 1;
            continue;
        }

        if token.kind == TokenKind::Pound {
            let parsed = parse_path_attribute(source, token_spans, index);
            if let Some((path_value, next_index)) = parsed {
                pending_mod_path = Some(path_value);
                index = next_index;
                continue;
            }
        }

        let after_vis = skip_visibility(source, token_spans, index);
        if let Some(next_index) = handle_top_level_mod_decl(
            source,
            token_spans,
            after_vis,
            &mut pending_mod_path,
            &mut import_specs,
        ) {
            index = next_index;
            continue;
        }

        if is_ident_text(source, token_spans, after_vis, "use") {
            if let Some(next_index) =
                parse_use_stmt_into(source, token_spans, after_vis, &mut import_specs)
            {
                pending_mod_path = None;
                index = next_index;
                continue;
            }
        }

        pending_mod_path = None;
        index += 1;
    }

    import_specs
}

fn update_block_depth(token_kind: TokenKind, block_depth: &mut usize) -> bool {
    match token_kind {
        TokenKind::OpenBrace => {
            *block_depth += 1;
            true
        }
        TokenKind::CloseBrace => {
            *block_depth = block_depth.saturating_sub(1);
            true
        }
        _ => false,
    }
}

fn handle_top_level_mod_decl(
    source: &str,
    token_spans: &[TokenSpan],
    mod_keyword_index: usize,
    pending_mod_path: &mut Option<String>,
    import_specs: &mut HashSet<String>,
) -> Option<usize> {
    if !is_ident_text(source, token_spans, mod_keyword_index, "mod") {
        return None;
    }
    let (mod_name, is_external, next_index) =
        parse_mod_decl(source, token_spans, mod_keyword_index)?;
    if !is_external {
        *pending_mod_path = None;
        return Some(next_index);
    }
    if let Some(path_value) = pending_mod_path.take() {
        import_specs.insert(format!("path:{path_value}"));
        return Some(next_index);
    }
    import_specs.insert(format!("self::{mod_name}"));
    Some(next_index)
}

fn parse_path_attribute(
    source: &str,
    token_spans: &[TokenSpan],
    pound_index: usize,
) -> Option<(String, usize)> {
    if token_spans
        .get(pound_index)
        .is_none_or(|t| t.kind != TokenKind::Pound)
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

    let after_bracket = skip_trivia(token_spans, bracket_index + 1);
    if !is_ident_text(source, token_spans, after_bracket, "path") {
        return None;
    }

    let after_path = skip_trivia(token_spans, after_bracket + 1);
    if token_spans
        .get(after_path)
        .is_none_or(|t| t.kind != TokenKind::Eq)
    {
        return None;
    }
    let after_eq = skip_trivia(token_spans, after_path + 1);
    let literal_span = token_spans.get(after_eq)?;
    if !matches!(literal_span.kind, TokenKind::Literal { .. }) {
        return None;
    }

    let literal_text = &source[literal_span.start..literal_span.end];
    let path_value = unescape_rust_string_literal(literal_text)?;

    let after_literal = skip_trivia(token_spans, after_eq + 1);
    let close_index = skip_trivia(token_spans, after_literal);
    if token_spans
        .get(close_index)
        .is_none_or(|t| t.kind != TokenKind::CloseBracket)
    {
        return None;
    }

    Some((path_value, close_index + 1))
}

fn parse_mod_decl<'a>(
    source: &'a str,
    token_spans: &[TokenSpan],
    mod_index: usize,
) -> Option<(&'a str, bool, usize)> {
    if !is_ident_text(source, token_spans, mod_index, "mod") {
        return None;
    }
    let name_index = skip_trivia(token_spans, mod_index + 1);
    let name_span = token_spans.get(name_index)?;
    if !matches!(name_span.kind, TokenKind::Ident | TokenKind::RawIdent) {
        return None;
    }
    let name = source.get(name_span.start..name_span.end)?;

    let mut index = name_index + 1;
    while index < token_spans.len() {
        let kind = token_spans[index].kind;
        if is_trivia(kind) {
            index += 1;
            continue;
        }
        if kind == TokenKind::Semi {
            return Some((name, true, index + 1));
        }
        if kind == TokenKind::OpenBrace {
            return Some((name, false, index + 1));
        }
        index += 1;
    }
    None
}

fn parse_use_stmt_into<'a>(
    source: &'a str,
    token_spans: &[TokenSpan],
    use_index: usize,
    out: &mut HashSet<String>,
) -> Option<usize> {
    if !is_ident_text(source, token_spans, use_index, "use") {
        return None;
    }

    let mut index = skip_trivia(token_spans, use_index + 1);
    if token_spans
        .get(index)
        .is_some_and(|t| t.kind == TokenKind::Colon)
        && token_spans
            .get(skip_trivia(token_spans, index + 1))
            .is_some_and(|t| t.kind == TokenKind::Colon)
    {
        index = skip_trivia(token_spans, index + 2);
    }

    let mut prefix: Vec<&'a str> = Vec::new();
    let next_index = parse_use_tree(source, token_spans, index, &mut prefix, out)?;
    let semi_index = skip_trivia(token_spans, next_index);
    if token_spans
        .get(semi_index)
        .is_none_or(|t| t.kind != TokenKind::Semi)
    {
        return None;
    }
    Some(semi_index + 1)
}

fn parse_use_tree<'a>(
    source: &'a str,
    token_spans: &[TokenSpan],
    start_index: usize,
    prefix: &mut Vec<&'a str>,
    out: &mut HashSet<String>,
) -> Option<usize> {
    let index = skip_trivia(token_spans, start_index);
    let token = token_spans.get(index)?;

    if token.kind == TokenKind::Star {
        insert_joined_path(prefix, out);
        return Some(index + 1);
    }

    if token.kind == TokenKind::OpenBrace {
        return parse_use_group(source, token_spans, index, prefix, out);
    }

    if !matches!(token.kind, TokenKind::Ident | TokenKind::RawIdent) {
        return None;
    }

    let ident = source.get(token.start..token.end)?;
    let after_ident = skip_trivia(token_spans, index + 1);

    if is_ident_text(source, token_spans, after_ident, "as") {
        prefix.push(ident);
        insert_joined_path(prefix, out);
        prefix.pop();
        let alias_ident_index = skip_trivia(token_spans, after_ident + 1);
        let next_index = if token_spans
            .get(alias_ident_index)
            .is_some_and(|t| matches!(t.kind, TokenKind::Ident | TokenKind::RawIdent))
        {
            alias_ident_index + 1
        } else {
            alias_ident_index
        };
        return Some(next_index);
    }

    if token_spans
        .get(after_ident)
        .is_some_and(|t| t.kind == TokenKind::Colon)
        && token_spans
            .get(skip_trivia(token_spans, after_ident + 1))
            .is_some_and(|t| t.kind == TokenKind::Colon)
    {
        prefix.push(ident);
        let next_index = parse_use_tree(
            source,
            token_spans,
            skip_trivia(token_spans, after_ident + 2),
            prefix,
            out,
        )?;
        prefix.pop();
        return Some(next_index);
    }

    prefix.push(ident);
    insert_joined_path(prefix, out);
    prefix.pop();
    Some(after_ident)
}

fn parse_use_group<'a>(
    source: &'a str,
    token_spans: &[TokenSpan],
    open_brace_index: usize,
    prefix: &mut Vec<&'a str>,
    out: &mut HashSet<String>,
) -> Option<usize> {
    let after_open = skip_trivia(token_spans, open_brace_index + 1);
    if token_spans
        .get(after_open)
        .is_some_and(|t| t.kind == TokenKind::CloseBrace)
    {
        return Some(after_open + 1);
    }

    let mut index = after_open;
    loop {
        index = parse_use_tree(source, token_spans, index, prefix, out)?;
        let after_tree = skip_trivia(token_spans, index);
        let next = token_spans.get(after_tree)?;
        if next.kind == TokenKind::Comma {
            index = skip_trivia(token_spans, after_tree + 1);
            continue;
        }
        if next.kind == TokenKind::CloseBrace {
            return Some(after_tree + 1);
        }
        return None;
    }
}

fn insert_joined_path(prefix: &[&str], out: &mut HashSet<String>) {
    if prefix.is_empty() {
        return;
    }
    out.insert(joined_path_text(prefix));
}

fn joined_path_text(segments: &[&str]) -> String {
    let separators_len = segments.len().saturating_sub(1) * 2;
    let segments_len = segments.iter().map(|s| s.len()).sum::<usize>();
    let mut text = String::with_capacity(separators_len + segments_len);
    for (index, segment) in segments.iter().enumerate() {
        if index != 0 {
            text.push_str("::");
        }
        text.push_str(segment);
    }
    text
}
