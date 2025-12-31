#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TreemapNode {
    pub name: String,
    pub bytes: u64,
    pub children: Vec<TreemapNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedLocation {
    pub function_name: String,
    pub file_path: Option<String>,
    pub line_number: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolRecord {
    pub address: u64,
    pub size_bytes: u64,
    pub raw_symbol_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributionSource {
    Dwarf,
    MachOFunctionStarts,
    MachOTextSymbols,
    ElfSymbols,
    SectionsOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FunctionRange {
    pub start: u64,
    pub end: u64,
}

impl FunctionRange {
    pub fn size_bytes(self) -> u64 {
        self.end.saturating_sub(self.start)
    }
}
