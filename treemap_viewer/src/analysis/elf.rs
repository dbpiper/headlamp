use object::Object;
use object::ObjectSymbol;

pub fn function_ranges(file: &object::File<'_>) -> Vec<(u64, u64)> {
    let mut functions = file
        .symbols()
        .chain(file.dynamic_symbols())
        .filter(|symbol| symbol.is_definition())
        .filter(|symbol| symbol.kind() == object::SymbolKind::Text)
        .filter(|symbol| symbol.address() != 0)
        .map(|symbol| (symbol.address(), symbol.size()))
        .collect::<Vec<_>>();

    functions.sort_by_key(|(address, _)| *address);
    let mut ranges = Vec::with_capacity(functions.len());
    for index in 0..functions.len() {
        let (start, size) = functions[index];
        let end = if size != 0 {
            start.saturating_add(size)
        } else {
            functions
                .get(index + 1)
                .map(|(next_start, _)| *next_start)
                .unwrap_or(start)
        };
        if end > start {
            ranges.push((start, end));
        }
    }
    ranges
}
