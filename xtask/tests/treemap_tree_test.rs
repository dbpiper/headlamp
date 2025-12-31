use xtask::dwarf_addr2line::ResolvedLocation;
use xtask::size_report::MapSymbol;
use xtask::treemap::{build_treemap, SymbolWithLocation};

#[test]
fn builds_crate_file_function_hierarchy_and_sums_bytes() {
    let symbols = vec![
        SymbolWithLocation {
            symbol: MapSymbol {
                address: 0x1000,
                size_bytes: 10,
                object_index: 1,
                raw_symbol_name: "_one".to_string(),
            },
            object_path:
                "/tmp/libheadlamp-1234567890abcdef.rlib(headlamp-1234567890abcdef.cgu.0.o)"
                    .to_string(),
            resolved: ResolvedLocation {
                function_name: "headlamp::foo".to_string(),
                file_path: Some("/repo/headlamp/src/foo.rs".to_string()),
                line_number: Some(10),
            },
        },
        SymbolWithLocation {
            symbol: MapSymbol {
                address: 0x2000,
                size_bytes: 5,
                object_index: 1,
                raw_symbol_name: "_two".to_string(),
            },
            object_path:
                "/tmp/libheadlamp-1234567890abcdef.rlib(headlamp-1234567890abcdef.cgu.0.o)"
                    .to_string(),
            resolved: ResolvedLocation {
                function_name: "headlamp::foo".to_string(),
                file_path: Some("/repo/headlamp/src/foo.rs".to_string()),
                line_number: Some(11),
            },
        },
        SymbolWithLocation {
            symbol: MapSymbol {
                address: 0x3000,
                size_bytes: 7,
                object_index: 2,
                raw_symbol_name: "_three".to_string(),
            },
            object_path: "/tmp/libsyn-1234567890abcdef.rlib(syn-1234567890abcdef.cgu.0.o)"
                .to_string(),
            resolved: ResolvedLocation {
                function_name: "syn::parse".to_string(),
                file_path: Some("/deps/syn/src/lib.rs".to_string()),
                line_number: Some(1),
            },
        },
    ];

    let tree = build_treemap(&symbols);
    assert_eq!(tree.bytes, 22);
    assert_eq!(tree.children.len(), 2);
    assert_eq!(tree.children[0].name, "headlamp");
    assert_eq!(tree.children[0].bytes, 15);

    let headlamp_file = &tree.children[0].children[0];
    assert_eq!(headlamp_file.name, "/repo/headlamp/src/foo.rs");
    assert_eq!(headlamp_file.bytes, 15);
    assert_eq!(headlamp_file.children.len(), 1);
    assert_eq!(headlamp_file.children[0].name, "headlamp::foo");
    assert_eq!(headlamp_file.children[0].bytes, 15);
}
