use std::path::{Path, PathBuf};

fn mk_temp_dir(name: &str) -> PathBuf {
    let base = std::env::temp_dir().join("headlamp-core-tests").join(name);
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    base
}

fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, contents).unwrap();
}

#[test]
fn ts_js_resolution_relative_prefers_ts_over_js_and_supports_index() {
    let repo = mk_temp_dir("tsjs-resolve-relative-priority");
    write_file(&repo.join("src/entry.ts"), "export const entry = 1;\n");
    write_file(&repo.join("src/foo.ts"), "export const foo = 1;\n");
    write_file(&repo.join("src/foo.js"), "exports.foo = 1;\n");
    write_file(&repo.join("src/bar/index.tsx"), "export const bar = 1;\n");

    let from = repo.join("src/entry.ts");

    let foo = headlamp::selection::dependency_language::resolve_import_with_root(
        headlamp::selection::dependency_language::DependencyLanguageId::TsJs,
        &from,
        "./foo",
        &repo,
    )
    .unwrap();
    assert!(foo.to_string_lossy().ends_with("src/foo.ts"));

    let bar = headlamp::selection::dependency_language::resolve_import_with_root(
        headlamp::selection::dependency_language::DependencyLanguageId::TsJs,
        &from,
        "./bar",
        &repo,
    )
    .unwrap();
    assert!(bar.to_string_lossy().ends_with("src/bar/index.tsx"));
}

#[test]
fn ts_js_resolution_repo_root_slash_is_repo_relative() {
    let repo = mk_temp_dir("tsjs-resolve-repo-root-slash");
    write_file(&repo.join("src/entry.ts"), "export const entry = 1;\n");
    write_file(&repo.join("src/x.ts"), "export const x = 1;\n");

    let from = repo.join("src/entry.ts");
    let resolved = headlamp::selection::dependency_language::resolve_import_with_root(
        headlamp::selection::dependency_language::DependencyLanguageId::TsJs,
        &from,
        "/src/x",
        &repo,
    )
    .unwrap();
    assert!(resolved.to_string_lossy().ends_with("src/x.ts"));
}

#[test]
fn ts_js_resolution_tsconfig_paths_resolves_aliases() {
    let repo = mk_temp_dir("tsjs-resolve-tsconfig-paths");
    write_file(
        &repo.join("tsconfig.json"),
        r#"
        // comment + trailing commas should parse (json5)
        {
          "compilerOptions": {
            "baseUrl": ".",
            "paths": {
              "@app/*": ["src/app/*"],
            },
          },
        }
        "#,
    );
    write_file(&repo.join("src/entry.ts"), "import { x } from '@app/x';\n");
    write_file(&repo.join("src/app/x.ts"), "export const x = 1;\n");

    let from = repo.join("src/entry.ts");
    let resolved = headlamp::selection::dependency_language::resolve_import_with_root(
        headlamp::selection::dependency_language::DependencyLanguageId::TsJs,
        &from,
        "@app/x",
        &repo,
    );
    assert!(resolved.is_some());
    assert!(
        resolved
            .unwrap()
            .to_string_lossy()
            .ends_with("src/app/x.ts")
    );
}

#[test]
fn ts_js_resolution_tsconfig_root_dirs_falls_back_to_alternate_root() {
    let repo = mk_temp_dir("tsjs-resolve-tsconfig-rootdirs");
    write_file(
        &repo.join("tsconfig.json"),
        r#"
        {
          "compilerOptions": {
            "rootDirs": ["src", "generated"],
          },
        }
        "#,
    );
    write_file(
        &repo.join("src/app/main.ts"),
        "import { util } from './util';\nexport const main = util;\n",
    );
    write_file(
        &repo.join("generated/app/util.ts"),
        "export const util = 1;\n",
    );

    let from = repo.join("src/app/main.ts");
    let resolved = headlamp::selection::dependency_language::resolve_import_with_root(
        headlamp::selection::dependency_language::DependencyLanguageId::TsJs,
        &from,
        "./util",
        &repo,
    );
    assert!(resolved.is_some());
    assert!(
        resolved
            .unwrap()
            .to_string_lossy()
            .ends_with("generated/app/util.ts")
    );
}

#[test]
fn ts_js_resolution_bare_specifiers_never_traverse_node_modules() {
    let repo = mk_temp_dir("tsjs-resolve-no-node-modules");
    write_file(
        &repo.join("src/entry.ts"),
        "import react from 'react';\nexport const x = react;\n",
    );
    write_file(
        &repo.join("node_modules/react/index.js"),
        "module.exports = {};\n",
    );

    let from = repo.join("src/entry.ts");
    let resolved = headlamp::selection::dependency_language::resolve_import_with_root(
        headlamp::selection::dependency_language::DependencyLanguageId::TsJs,
        &from,
        "react",
        &repo,
    );
    assert!(resolved.is_none());
}
