use oxc_resolver::{ResolveOptions, Resolver};

pub(super) fn build_resolver() -> Resolver {
    let extensions = [
        ".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".mts", ".cts", ".json",
    ]
    .into_iter()
    .map(|ext| ext.to_string())
    .collect::<Vec<_>>();
    Resolver::new(ResolveOptions {
        extensions,
        ..Default::default()
    })
}
