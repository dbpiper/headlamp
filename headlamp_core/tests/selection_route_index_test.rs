#[test]
fn selection_route_index_extracts_basic_http_routes() {
    if !should_run_rg_tests() {
        return;
    }
    let repo = mk_temp_dir("route-index-basic-routes");
    write_file(
        &repo.join("server/api.js"),
        "const express = require('express');\nconst router = express.Router();\nrouter.get('/hello', () => {});\nrouter.post(\"/v1/items/:id\", () => {});\nmodule.exports = router;\n",
    );
    write_file(
        &repo.join("server/app.js"),
        "const express = require('express');\nconst app = express();\nconst api = require('./api');\napp.use('/api', api);\nmodule.exports = app;\n",
    );

    let index = headlamp_core::selection::route_index::get_route_index(&repo);
    let sources_hello = index.sources_for_http_route("/api/hello");
    assert!(
        sources_hello
            .iter()
            .any(|s| s.ends_with("/server/api.js") || s.ends_with("\\server\\api.js"))
    );
    let sources_items = index.sources_for_http_route("/api/v1/items/:id");
    assert!(
        sources_items
            .iter()
            .any(|s| s.ends_with("/server/api.js") || s.ends_with("\\server\\api.js"))
    );
}

fn should_run_rg_tests() -> bool {
    which::which("rg").is_ok()
}

fn mk_temp_dir(name: &str) -> std::path::PathBuf {
    let base = std::env::temp_dir().join("headlamp-core-tests").join(name);
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    base
}

fn write_file(path: &std::path::Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, contents).unwrap();
}

#[test]
fn selection_route_index_tracks_nested_router_use_prefixes() {
    if !should_run_rg_tests() {
        return;
    }
    let repo = mk_temp_dir("route-index-nested-use");
    write_file(
        &repo.join("server/api.js"),
        "const express = require('express');\nconst router = express.Router();\nrouter.get('/hello', (_req, res) => res.send('ok'));\nmodule.exports = router;\n",
    );
    write_file(
        &repo.join("server/app.js"),
        "const express = require('express');\nconst app = express();\nconst api = require('./api');\napp.use('/api', api);\nmodule.exports = app;\n",
    );

    let index = headlamp_core::selection::route_index::get_route_index(&repo);
    let sources = index.sources_for_http_route("/api/hello");
    assert!(
        sources
            .iter()
            .any(|s| s.ends_with("/server/api.js") || s.ends_with("\\server\\api.js"))
    );
}

#[test]
fn selection_route_index_tracks_use_without_base_path() {
    if !should_run_rg_tests() {
        return;
    }
    let repo = mk_temp_dir("route-index-use-no-path");
    write_file(
        &repo.join("server/api.js"),
        "const express = require('express');\nconst router = express.Router();\nrouter.get('/hello', (_req, res) => res.send('ok'));\nmodule.exports = router;\n",
    );
    write_file(
        &repo.join("server/app.js"),
        "const express = require('express');\nconst app = express();\nconst api = require('./api');\napp.use(api);\nmodule.exports = app;\n",
    );

    let index = headlamp_core::selection::route_index::get_route_index(&repo);
    let sources = index.sources_for_http_route("/hello");
    assert!(
        sources
            .iter()
            .any(|s| s.ends_with("/server/api.js") || s.ends_with("\\server\\api.js"))
    );
}

#[test]
fn selection_route_index_supports_router_dot_route_chain() {
    if !should_run_rg_tests() {
        return;
    }
    let repo = mk_temp_dir("route-index-router-dot-route");
    write_file(
        &repo.join("server/api.js"),
        "const express = require('express');\nconst router = express.Router();\nrouter.route('/hello').get((_req, res) => res.send('ok'));\nmodule.exports = router;\n",
    );
    write_file(
        &repo.join("server/app.js"),
        "const express = require('express');\nconst app = express();\nconst api = require('./api');\napp.use('/api', api);\nmodule.exports = app;\n",
    );

    let index = headlamp_core::selection::route_index::get_route_index(&repo);
    let sources = index.sources_for_http_route("/api/hello");
    assert!(
        sources
            .iter()
            .any(|s| s.ends_with("/server/api.js") || s.ends_with("\\server\\api.js"))
    );
}

#[test]
fn selection_route_index_supports_app_method_routes() {
    if !should_run_rg_tests() {
        return;
    }
    let repo = mk_temp_dir("route-index-app-method");
    write_file(
        &repo.join("server/app.js"),
        "const express = require('express');\nconst app = express();\napp.get('/health', (_req, res) => res.send('ok'));\nmodule.exports = app;\n",
    );

    let index = headlamp_core::selection::route_index::get_route_index(&repo);
    let sources = index.sources_for_http_route("/health");
    assert!(
        sources
            .iter()
            .any(|s| s.ends_with("/server/app.js") || s.ends_with("\\server\\app.js"))
    );
}

#[test]
fn selection_route_index_resolves_named_import_mounts() {
    if !should_run_rg_tests() {
        return;
    }
    let repo = mk_temp_dir("route-index-named-import-mount");
    write_file(
        &repo.join("server/api.js"),
        "const express = require('express');\nconst router = express.Router();\nrouter.get('/hello', (_req, res) => res.send('ok'));\nexport { router };\n",
    );
    write_file(
        &repo.join("server/app.js"),
        "import express from 'express';\nimport { router as api } from './api';\nconst app = express();\napp.use('/api', api);\nexport default app;\n",
    );

    let index = headlamp_core::selection::route_index::get_route_index(&repo);
    let sources = index.sources_for_http_route("/api/hello");
    assert!(
        sources
            .iter()
            .any(|s| s.ends_with("/server/api.js") || s.ends_with("\\server\\api.js"))
    );
}

#[test]
fn selection_route_index_is_deterministic_and_monotone_under_extra_files() {
    if !should_run_rg_tests() {
        return;
    }
    let repo = mk_temp_dir("route-index-deterministic-monotone");
    write_file(
        &repo.join("server/api.js"),
        "const express = require('express');\nconst router = express.Router();\nrouter.get('/hello', () => {});\nmodule.exports = router;\n",
    );
    write_file(
        &repo.join("server/app.js"),
        "const express = require('express');\nconst app = express();\nconst api = require('./api');\napp.use('/api', api);\nmodule.exports = app;\n",
    );

    let index_before = headlamp_core::selection::route_index::get_route_index(&repo);
    let sources_before_1 = index_before.sources_for_http_route("/api/hello");
    let sources_before_2 = index_before.sources_for_http_route("/api/hello");
    assert_eq!(sources_before_1, sources_before_2);

    write_file(
        &repo.join("server/extra.js"),
        "const express = require('express');\nconst app = express();\napp.get('/extra', () => {});\nmodule.exports = app;\n",
    );
    let index_after = headlamp_core::selection::route_index::get_route_index(&repo);
    let sources_after = index_after.sources_for_http_route("/api/hello");
    sources_before_1.iter().for_each(|source| {
        assert!(sources_after.contains(source));
    });
}
