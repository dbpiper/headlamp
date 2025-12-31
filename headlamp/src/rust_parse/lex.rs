use rustc_lexer::TokenKind;

use super::types::TokenSpan;

pub(super) fn lex_spans(source: &str) -> Vec<TokenSpan> {
    let mut spans: Vec<TokenSpan> = Vec::new();
    let mut offset = 0usize;
    for token in rustc_lexer::tokenize(source) {
        let start = offset;
        let end = offset + token.len;
        spans.push(TokenSpan {
            kind: token.kind,
            start,
            end,
        });
        offset = end;
    }
    spans
}

pub(super) fn is_trivia(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Whitespace | TokenKind::LineComment | TokenKind::BlockComment { .. }
    )
}
