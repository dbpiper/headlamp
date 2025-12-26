use std::path::Path;

use crate::project::classify::FileKind;

pub fn classify_by_content(abs_path: &Path) -> FileKind {
    let Ok(body) = std::fs::read_to_string(abs_path) else {
        return FileKind::Unknown;
    };
    let Ok(file) = syn::parse_file(&body) else {
        return FileKind::Unknown;
    };

    let has_test_attr = file.items.iter().any(item_has_test_marker);
    let has_cfg_test = file
        .items
        .iter()
        .any(|item| attrs_contain_cfg_test(item_attrs(item)));

    match (has_test_attr, has_cfg_test) {
        (true, true) => FileKind::Mixed,
        (true, false) => FileKind::Test,
        (false, true) => FileKind::Production,
        (false, false) => FileKind::Production,
    }
}

fn item_has_test_marker(item: &syn::Item) -> bool {
    match item {
        syn::Item::Fn(item_fn) => attrs_contain_test_like(&item_fn.attrs),
        syn::Item::Mod(item_mod) => attrs_contain_test_like(&item_mod.attrs),
        syn::Item::Impl(item_impl) => attrs_contain_test_like(&item_impl.attrs),
        _ => false,
    }
}

fn attrs_contain_test_like(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(attr_is_test_marker) || attrs_contain_cfg_test(attrs)
}

fn attrs_contain_cfg_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if !attr.path().is_ident("cfg") {
            return false;
        }
        let mut found = false;
        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("test") {
                found = true;
            }
            Ok(())
        });
        found
    })
}

fn attr_is_test_marker(attr: &syn::Attribute) -> bool {
    attr.path()
        .segments
        .last()
        .map(|seg| seg.ident.to_string())
        .is_some_and(|ident| matches!(ident.as_str(), "test" | "rstest"))
}

fn item_attrs(item: &syn::Item) -> &[syn::Attribute] {
    match item {
        syn::Item::Const(i) => &i.attrs,
        syn::Item::Enum(i) => &i.attrs,
        syn::Item::ExternCrate(i) => &i.attrs,
        syn::Item::Fn(i) => &i.attrs,
        syn::Item::ForeignMod(i) => &i.attrs,
        syn::Item::Impl(i) => &i.attrs,
        syn::Item::Macro(i) => &i.attrs,
        syn::Item::Mod(i) => &i.attrs,
        syn::Item::Static(i) => &i.attrs,
        syn::Item::Struct(i) => &i.attrs,
        syn::Item::Trait(i) => &i.attrs,
        syn::Item::TraitAlias(i) => &i.attrs,
        syn::Item::Type(i) => &i.attrs,
        syn::Item::Union(i) => &i.attrs,
        syn::Item::Use(i) => &i.attrs,
        _ => &[],
    }
}
