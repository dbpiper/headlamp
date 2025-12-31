use std::path::Path;

use object::Object;
use object::ObjectSymbol;

use crate::model::SymbolRecord;

pub struct ParsedBinary<'data> {
    pub object_file: object::File<'data>,
}

pub fn parse_binary(binary_path: &Path) -> Result<ParsedBinary<'static>, String> {
    let data = std::fs::read(binary_path)
        .map_err(|error| format!("failed to read {}: {error}", binary_path.display()))?;

    let leaked: &'static [u8] = Box::leak(data.into_boxed_slice());
    let object_file = object::File::parse(leaked)
        .map_err(|error| format!("failed to parse object {}: {error}", binary_path.display()))?;

    Ok(ParsedBinary { object_file })
}

pub fn read_symbols_from_object(file: &object::File<'_>) -> Vec<SymbolRecord> {
    let mut records = file
        .symbols()
        .chain(file.dynamic_symbols())
        .filter(|symbol| symbol.is_definition())
        .filter(|symbol| symbol.address() != 0)
        .filter_map(|symbol| {
            let raw_name = match symbol.name() {
                Ok(name) => name.to_string(),
                Err(_) => String::from_utf8_lossy(symbol.name_bytes().ok()?).to_string(),
            };
            Some(SymbolRecord {
                address: symbol.address(),
                size_bytes: symbol.size(),
                raw_symbol_name: raw_name,
            })
        })
        .collect::<Vec<_>>();

    records.sort_by_key(|record| record.address);
    records
}

pub fn fill_missing_symbol_sizes(records: &mut [SymbolRecord]) {
    for index in 0..records.len() {
        if records[index].size_bytes != 0 {
            continue;
        }
        let Some(next) = records.get(index + 1) else {
            continue;
        };
        let current_address = records[index].address;
        let next_address = next.address;
        if next_address > current_address {
            records[index].size_bytes = next_address - current_address;
        }
    }
}

pub fn label_address_from_symbols(symbols: &[SymbolRecord], address: u64) -> String {
    if symbols.is_empty() {
        return format!("function@0x{address:x}");
    }

    match symbols.binary_search_by_key(&address, |record| record.address) {
        Ok(index) => super::tree::demangle_symbol(&symbols[index].raw_symbol_name),
        Err(0) => format!("function@0x{address:x}"),
        Err(insert_index) => {
            let base = &symbols[insert_index - 1];
            let base_name = super::tree::demangle_symbol(&base.raw_symbol_name);
            let delta = address.saturating_sub(base.address);
            if delta == 0 {
                base_name
            } else {
                format!("{base_name}+0x{delta:x}")
            }
        }
    }
}
