use serde::Serialize;
use sha1::{Digest, Sha1};
use std::collections::BTreeMap;
use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum TokenKind {
    AnsiEscape,
    Osc8Link,
    Text,
    Whitespace,
    Newline,
}

#[derive(Debug, Clone, Serialize)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Range<usize>,
    pub visible_width: usize,
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TokenStats {
    pub token_count: usize,
    pub counts_by_kind: BTreeMap<TokenKind, usize>,
    pub visible_width_total: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct TokenStream {
    pub tokens: Vec<Token>,
    pub stats: TokenStats,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum BlockKind {
    Blank,
    BoxTable,
    PipeTable,
    Rule,
    Text,
}

#[derive(Debug, Clone, Serialize)]
pub struct LineNode {
    pub token_range: Range<usize>,
    pub visible_width: usize,
    pub stripped_preview: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BlockNode {
    pub kind: BlockKind,
    pub line_range: Range<usize>,
    pub hash: String,
    pub line_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct DocumentAst {
    pub lines: Vec<LineNode>,
    pub blocks: Vec<BlockNode>,
}

pub fn build_token_stream(text: &str) -> TokenStream {
    let tokens = tokenize(text);
    let stats = token_stats(&tokens);
    TokenStream { tokens, stats }
}

pub fn build_document_ast(text: &str) -> DocumentAst {
    let lines = build_lines(text);
    let blocks = build_blocks(&lines);
    DocumentAst { lines, blocks }
}

fn tokenize(text: &str) -> Vec<Token> {
    let bytes = text.as_bytes();
    let mut tokens: Vec<Token> = vec![];
    let mut index = 0usize;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'\n' {
            tokens.push(Token {
                kind: TokenKind::Newline,
                span: index..index + 1,
                visible_width: 0,
                snippet: "\\n".to_string(),
            });
            index += 1;
            continue;
        }
        if byte == 0x1b {
            let (kind, end) = parse_escape(bytes, index);
            tokens.push(Token {
                kind,
                span: index..end,
                visible_width: 0,
                snippet: snippet(text, index..end),
            });
            index = end;
            continue;
        }
        if is_ws(byte) {
            let end = scan_while(bytes, index, is_ws);
            tokens.push(Token {
                kind: TokenKind::Whitespace,
                span: index..end,
                visible_width: end.saturating_sub(index),
                snippet: snippet(text, index..end),
            });
            index = end;
            continue;
        }
        let end = scan_while(bytes, index, |b| b != b'\n' && b != 0x1b && !is_ws(b));
        let visible_width = text[index..end].chars().count();
        tokens.push(Token {
            kind: TokenKind::Text,
            span: index..end,
            visible_width,
            snippet: snippet(text, index..end),
        });
        index = end;
    }
    tokens
}

fn parse_escape(bytes: &[u8], start: usize) -> (TokenKind, usize) {
    let next = bytes.get(start + 1).copied().unwrap_or_default();
    match next {
        b'[' => (TokenKind::AnsiEscape, consume_csi(bytes, start)),
        b']' => consume_osc(bytes, start),
        _ => (TokenKind::AnsiEscape, start.saturating_add(1)),
    }
}

fn consume_csi(bytes: &[u8], start: usize) -> usize {
    let mut i = start.saturating_add(2);
    while i < bytes.len() {
        let b = bytes[i];
        if (0x40..=0x7e).contains(&b) {
            return i + 1;
        }
        i += 1;
    }
    bytes.len()
}

fn consume_osc(bytes: &[u8], start: usize) -> (TokenKind, usize) {
    let mut i = start.saturating_add(2);
    while i < bytes.len() {
        match bytes[i] {
            0x07 => return (osc_kind(bytes, start, i + 1), i + 1),
            0x1b if bytes.get(i + 1) == Some(&b'\\') => {
                return (osc_kind(bytes, start, i + 2), i + 2);
            }
            _ => i += 1,
        }
    }
    (TokenKind::AnsiEscape, bytes.len())
}

fn osc_kind(bytes: &[u8], start: usize, end: usize) -> TokenKind {
    let slice = bytes.get(start..end).unwrap_or(&[]);
    let is_osc8 = slice.windows(3).any(|w| w == [b']', b'8', b';']);
    if is_osc8 {
        TokenKind::Osc8Link
    } else {
        TokenKind::AnsiEscape
    }
}

fn scan_while(bytes: &[u8], start: usize, keep: impl Fn(u8) -> bool) -> usize {
    let mut i = start;
    while i < bytes.len() && keep(bytes[i]) {
        i += 1;
    }
    i
}

fn is_ws(byte: u8) -> bool {
    byte == b' ' || byte == b'\t' || byte == b'\r'
}

fn snippet(text: &str, span: Range<usize>) -> String {
    const LIMIT: usize = 80;
    let raw = text.get(span).unwrap_or("");
    raw.chars().take(LIMIT).collect::<String>()
}

fn token_stats(tokens: &[Token]) -> TokenStats {
    let counts_by_kind = tokens.iter().fold(BTreeMap::new(), |mut acc, token| {
        *acc.entry(token.kind).or_insert(0) += 1;
        acc
    });
    let visible_width_total = tokens.iter().map(|t| t.visible_width).sum();
    TokenStats {
        token_count: tokens.len(),
        counts_by_kind,
        visible_width_total,
    }
}

fn build_lines(text: &str) -> Vec<LineNode> {
    text.lines()
        .map(|line| {
            let stripped = headlamp_core::format::stacks::strip_ansi_simple(line);
            LineNode {
                token_range: 0..0,
                visible_width: stripped.chars().count(),
                stripped_preview: stripped.chars().take(120).collect(),
            }
        })
        .collect()
}

fn build_blocks(lines: &[LineNode]) -> Vec<BlockNode> {
    let mut blocks: Vec<BlockNode> = vec![];
    let mut start = 0usize;
    while start < lines.len() {
        let next = next_block_end(lines, start);
        let slice = &lines[start..next];
        let kind = classify_block(slice);
        blocks.push(BlockNode {
            kind,
            line_range: start..next,
            hash: hash_block(kind, slice),
            line_count: slice.len(),
        });
        start = next;
    }
    blocks
}

fn next_block_end(lines: &[LineNode], start: usize) -> usize {
    let is_blank = |line: &LineNode| line.stripped_preview.trim().is_empty();
    let start_is_blank = lines.get(start).is_some_and(is_blank);
    let end = (start..lines.len()).find(|&i| {
        let blank = lines.get(i).is_some_and(is_blank);
        blank != start_is_blank
    });
    end.unwrap_or(lines.len())
}

fn classify_block(lines: &[LineNode]) -> BlockKind {
    if lines.iter().all(|l| l.stripped_preview.trim().is_empty()) {
        return BlockKind::Blank;
    }
    if lines.iter().any(|l| is_box_table_line(&l.stripped_preview)) {
        return BlockKind::BoxTable;
    }
    if lines
        .iter()
        .any(|l| is_pipe_table_line(&l.stripped_preview))
    {
        return BlockKind::PipeTable;
    }
    if lines.iter().any(|l| is_rule_line(&l.stripped_preview)) {
        return BlockKind::Rule;
    }
    BlockKind::Text
}

fn is_box_table_line(line: &str) -> bool {
    line.contains('┌')
        || line.contains('└')
        || line.contains('┬')
        || line.contains('┴')
        || line.contains('│')
}

fn is_pipe_table_line(line: &str) -> bool {
    let t = line.trim();
    t.contains("|---------|")
        || (t.contains('|') && t.chars().all(|c| c == '|' || c == '-' || c == ' '))
}

fn is_rule_line(line: &str) -> bool {
    let t = line.trim();
    t.len() >= 20 && t.chars().all(|c| c == '─' || c == '=' || c == '-')
}

fn hash_block(kind: BlockKind, lines: &[LineNode]) -> String {
    let mut h = Sha1::new();
    h.update(format!("{kind:?}\n").as_bytes());
    lines
        .iter()
        .for_each(|line| h.update(format!("{}\n", line.stripped_preview).as_bytes()));
    let hex = hex::encode(h.finalize());
    hex.chars().take(12).collect()
}
