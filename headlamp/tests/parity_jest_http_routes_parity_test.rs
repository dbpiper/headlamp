mod parity_support;

use parity_support::{
    assert_parity_with_args, mk_repo, parity_binaries, write_file, write_jest_config,
};

#[test]
fn parity_jest_http_route_augmentation_fixture() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let repo = mk_repo("jest-http-routes", &binaries.node_modules);
    write_file(
        &repo.join("server/routes.js"),
        "const express = require('express');\nconst router = express.Router();\nrouter.get('/hello', (_req, res) => res.send('ok'));\nmodule.exports = router;\n",
    );
    write_file(
        &repo.join("tests/routes.test.js"),
        "test('routes', () => { expect('/hello').toContain('hello'); });\n",
    );
    write_jest_config(&repo, "**/tests/**/*.test.js");

    assert_parity_with_args(
        &repo,
        &binaries,
        &["server/routes.js"],
        &["server/routes.js"],
    );
}

#[test]
fn parity_jest_http_route_nested_use_fixture() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let repo = mk_repo("jest-http-nested-use", &binaries.node_modules);
    write_file(
        &repo.join("server/api.js"),
        "const express = require('express');\nconst router = express.Router();\nrouter.get('/hello', (_req, res) => res.send('ok'));\nmodule.exports = router;\n",
    );
    write_file(
        &repo.join("server/app.js"),
        "const express = require('express');\nconst app = express();\nconst api = require('./api');\napp.use('/api', api);\nmodule.exports = app;\n",
    );
    write_file(
        &repo.join("tests/http.test.js"),
        "test('http', () => { expect('/api/hello').toContain('hello'); });\n",
    );
    write_file(
        &repo.join("tests/unrelated.test.js"),
        "test('unrelated', () => { expect(1).toBe(1); });\n",
    );
    write_jest_config(&repo, "**/tests/**/*.test.js");

    assert_parity_with_args(&repo, &binaries, &["server/api.js"], &["server/api.js"]);
}

#[test]
fn parity_jest_http_route_use_without_base_path_fixture() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let repo = mk_repo("jest-http-use-no-path", &binaries.node_modules);
    write_file(
        &repo.join("server/api.js"),
        "const express = require('express');\nconst router = express.Router();\nrouter.get('/hello', (_req, res) => res.send('ok'));\nmodule.exports = router;\n",
    );
    write_file(
        &repo.join("server/app.js"),
        "const express = require('express');\nconst app = express();\nconst api = require('./api');\napp.use(api);\nmodule.exports = app;\n",
    );
    write_file(
        &repo.join("tests/http.test.js"),
        "test('http', () => { expect('/hello').toContain('hello'); });\n",
    );
    write_file(
        &repo.join("tests/unrelated.test.js"),
        "test('unrelated', () => { expect(1).toBe(1); });\n",
    );
    write_jest_config(&repo, "**/tests/**/*.test.js");

    assert_parity_with_args(&repo, &binaries, &["server/app.js"], &["server/app.js"]);
}

#[test]
fn parity_jest_http_route_inline_require_mount_fixture() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let repo = mk_repo("jest-http-inline-require-mount", &binaries.node_modules);
    write_file(
        &repo.join("server/api.js"),
        "const express = require('express');\nconst router = express.Router();\nrouter.get('/hello', (_req, res) => res.send('ok'));\nmodule.exports = router;\n",
    );
    write_file(
        &repo.join("server/app.js"),
        "const express = require('express');\nconst app = express();\napp.use('/api', require('./api'));\nmodule.exports = app;\n",
    );
    write_file(
        &repo.join("tests/http.test.js"),
        "test('http', () => { expect('/api/hello').toContain('hello'); });\n",
    );
    write_file(
        &repo.join("tests/unrelated.test.js"),
        "test('unrelated', () => { expect(1).toBe(1); });\n",
    );
    write_jest_config(&repo, "**/tests/**/*.test.js");

    assert_parity_with_args(&repo, &binaries, &["server/app.js"], &["server/app.js"]);
}

#[test]
fn parity_jest_http_route_deep_chain_fixture() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let repo = mk_repo("jest-http-deep-chain", &binaries.node_modules);
    write_file(
        &repo.join("server/v1.js"),
        "const express = require('express');\nconst router = express.Router();\nrouter.get('/hello', (_req, res) => res.send('ok'));\nmodule.exports = router;\n",
    );
    write_file(
        &repo.join("server/api.js"),
        "const express = require('express');\nconst router = express.Router();\nrouter.use('/v1', require('./v1'));\nmodule.exports = router;\n",
    );
    write_file(
        &repo.join("server/app.js"),
        "const express = require('express');\nconst app = express();\nconst api = require('./api');\napp.use('/api', api);\nmodule.exports = app;\n",
    );
    write_file(
        &repo.join("tests/http.test.js"),
        "test('http', () => { expect('/api/v1/hello').toContain('hello'); });\n",
    );
    write_file(
        &repo.join("tests/unrelated.test.js"),
        "test('unrelated', () => { expect(1).toBe(1); });\n",
    );
    write_jest_config(&repo, "**/tests/**/*.test.js");

    assert_parity_with_args(&repo, &binaries, &["server/app.js"], &["server/app.js"]);
}

#[test]
fn parity_jest_http_route_inline_require_mount_with_middleware_fixture() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let repo = mk_repo(
        "jest-http-inline-require-middleware",
        &binaries.node_modules,
    );
    write_file(
        &repo.join("server/v1.js"),
        "const express = require('express');\nconst router = express.Router();\nrouter.get('/hello', (_req, res) => res.send('ok'));\nmodule.exports = router;\n",
    );
    write_file(
        &repo.join("server/api.js"),
        "const express = require('express');\nconst router = express.Router();\nrouter.use('/v1', require('./v1'), (_req, _res, next) => next());\nmodule.exports = router;\n",
    );
    write_file(
        &repo.join("server/app.js"),
        "const express = require('express');\nconst app = express();\nconst api = require('./api');\napp.use('/api', api);\nmodule.exports = app;\n",
    );
    write_file(
        &repo.join("tests/http.test.js"),
        "test('http', () => { expect('/api/v1/hello').toContain('hello'); });\n",
    );
    write_file(
        &repo.join("tests/unrelated.test.js"),
        "test('unrelated', () => { expect(1).toBe(1); });\n",
    );
    write_jest_config(&repo, "**/tests/**/*.test.js");

    assert_parity_with_args(&repo, &binaries, &["server/app.js"], &["server/app.js"]);
}

#[test]
fn parity_jest_http_route_router_factory_export_fixture() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let repo = mk_repo("jest-http-router-factory-export", &binaries.node_modules);
    write_file(
        &repo.join("server/api.js"),
        "const express = require('express');\nconst router = express.Router();\nrouter.get('/hello', (_req, res) => res.send('ok'));\nmodule.exports = () => router;\n",
    );
    write_file(
        &repo.join("server/app.js"),
        "const express = require('express');\nconst app = express();\nconst apiFactory = require('./api');\napp.use('/api', apiFactory());\nmodule.exports = app;\n",
    );
    write_file(
        &repo.join("tests/http.test.js"),
        "test('http', () => { expect('/api/hello').toContain('hello'); });\n",
    );
    write_file(
        &repo.join("tests/unrelated.test.js"),
        "test('unrelated', () => { expect(1).toBe(1); });\n",
    );
    write_jest_config(&repo, "**/tests/**/*.test.js");

    assert_parity_with_args(&repo, &binaries, &["server/app.js"], &["server/app.js"]);
}

#[test]
fn parity_jest_http_route_exports_property_fixture() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let repo = mk_repo("jest-http-exports-property", &binaries.node_modules);
    write_file(
        &repo.join("server/api.js"),
        "const express = require('express');\nconst router = express.Router();\nrouter.get('/hello', (_req, res) => res.send('ok'));\nexports.router = router;\n",
    );
    write_file(
        &repo.join("server/app.js"),
        "const express = require('express');\nconst app = express();\nconst api = require('./api');\napp.use('/api', api.router);\nmodule.exports = app;\n",
    );
    write_file(
        &repo.join("tests/http.test.js"),
        "test('http', () => { expect('/api/hello').toContain('hello'); });\n",
    );
    write_file(
        &repo.join("tests/unrelated.test.js"),
        "test('unrelated', () => { expect(1).toBe(1); });\n",
    );
    write_jest_config(&repo, "**/tests/**/*.test.js");

    assert_parity_with_args(&repo, &binaries, &["server/app.js"], &["server/app.js"]);
}

#[test]
fn parity_jest_http_route_require_destructure_fixture() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let repo = mk_repo("jest-http-require-destructure", &binaries.node_modules);
    write_file(
        &repo.join("server/api.js"),
        "const express = require('express');\nconst router = express.Router();\nrouter.get('/hello', (_req, res) => res.send('ok'));\nexports.router = router;\n",
    );
    write_file(
        &repo.join("server/app.js"),
        "const express = require('express');\nconst app = express();\nconst { router: apiRouter } = require('./api');\napp.use('/api', apiRouter);\nmodule.exports = app;\n",
    );
    write_file(
        &repo.join("tests/http.test.js"),
        "test('http', () => { expect('/api/hello').toContain('hello'); });\n",
    );
    write_file(
        &repo.join("tests/unrelated.test.js"),
        "test('unrelated', () => { expect(1).toBe(1); });\n",
    );
    write_jest_config(&repo, "**/tests/**/*.test.js");

    assert_parity_with_args(&repo, &binaries, &["server/app.js"], &["server/app.js"]);
}
