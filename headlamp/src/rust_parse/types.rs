use rustc_lexer::TokenKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RustFileMarkers {
    pub has_test_attr: bool,
    pub has_cfg_test: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TokenSpan {
    pub(super) kind: TokenKind,
    pub(super) start: usize,
    pub(super) end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ParsedOuterAttr {
    pub(super) is_test_marker: bool,
    pub(super) is_cfg_test: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RustItemKind {
    Fn,
    Mod,
    Impl,
    Other,
}
