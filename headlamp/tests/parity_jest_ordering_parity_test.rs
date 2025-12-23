mod parity_support;

use parity_support::{
    assert_parity_with_args, mk_repo, parity_binaries, write_file, write_jest_config,
};

#[test]
fn parity_jest_directness_rank_and_http_augmentation_order_fixture() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let repo = mk_repo("jest-directness-rank-http-augment", &binaries.node_modules);
    write_file(
        &repo.join("server/routes.js"),
        "const express = require('express');\nconst router = express.Router();\nrouter.get('/hello', (_req, res) => res.send('ok'));\nmodule.exports = router;\n",
    );
    // Ensure fast-related matches by including the seed-ish path segment in test content.
    write_file(
        &repo.join("tests/related.test.js"),
        "const r = require('../server/routes');\n\ntest('related', () => { expect(typeof r).toBe('function'); });\n",
    );
    // Ensure route augmentation adds this test (it doesn't mention server/routes).
    write_file(
        &repo.join("tests/http.test.js"),
        "test('http', () => { expect('/hello').toContain('hello'); });\n",
    );
    write_jest_config(&repo, "**/tests/**/*.test.js");

    assert_parity_with_args(
        &repo,
        &binaries,
        &["server/routes.js"],
        &["server/routes.js"],
    );
}
