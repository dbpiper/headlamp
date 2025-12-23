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
