mod parity_support;

use parity_support::{
    mk_repo, normalize, parity_binaries, run_parity_fixture_with_args, write_file,
    write_jest_config,
};

#[test]
fn parity_jest_only_failures_and_show_logs_fixture() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let repo = mk_repo("jest-only-failures-show-logs", &binaries.node_modules);
    write_file(
        &repo.join("tests/mixed.test.js"),
        "test('pass', () => { console.log('log-pass'); expect(1).toBe(1); });\n\ntest('fail', () => { console.error('err-fail'); expect(1).toBe(2); });\n",
    );
    write_jest_config(&repo, "**/tests/**/*.test.js");

    let (_spec, code_ts, out_ts, code_rs, out_rs) = run_parity_fixture_with_args(
        &repo,
        &binaries.ts_cli,
        &binaries.rust_bin,
        &["--showLogs", "--onlyFailures"],
        &["--showLogs", "--onlyFailures"],
        "jest",
    );
    assert_eq!(code_ts, code_rs);
    let out_ts_raw = out_ts.clone();
    let debug_counts = out_ts
        .lines()
        .find(|ln| ln.contains("debug: bridge events total="))
        .unwrap_or("")
        .to_string();

    let n_ts = normalize(out_ts, &repo);
    let n_rs = normalize(out_rs, &repo);
    assert_eq!(n_ts, n_rs);
    assert!(
        n_ts.contains("Logs:"),
        "expected Logs section in TS output:\n{n_ts}\n\nraw-ts-debug:\n{debug_counts}"
    );
    assert!(
        out_ts_raw.contains("Stack:"),
        "expected Stack section in raw TS output:\n{out_ts_raw}\n\nraw-ts-debug:\n{debug_counts}"
    );
    assert!(
        n_ts.contains("log-pass"),
        "expected log-pass in TS output:\n{n_ts}"
    );
    assert!(
        n_ts.contains("err-fail"),
        "expected err-fail in TS output:\n{n_ts}"
    );
}

#[test]
fn parity_jest_http_card_fixture() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let repo = mk_repo("jest-http-card", &binaries.node_modules);
    write_file(
        &repo.join("tests/http-card.test.js"),
        "const http = require('node:http');\n\
\n\
test('GET /hello', async () => {\n\
  const server = http.createServer((_req, res) => {\n\
    res.statusCode = 500;\n\
    res.setHeader('Content-Type', 'text/plain');\n\
    res.end('nope');\n\
  });\n\
\n\
  await new Promise((resolve) => server.listen(0, resolve));\n\
  const port = server.address().port;\n\
\n\
  const statusCode = await new Promise((resolve, reject) => {\n\
    http.get(`http://127.0.0.1:${port}/hello`, (res) => {\n\
      res.resume();\n\
      res.on('end', () => resolve(res.statusCode));\n\
    }).on('error', reject);\n\
  });\n\
\n\
  try {\n\
    expect(statusCode).toBe(200);\n\
  } finally {\n\
    await new Promise((resolve) => server.close(resolve));\n\
  }\n\
});\n",
    );
    write_jest_config(&repo, "**/tests/**/*.test.js");

    let (_spec, code_ts, out_ts, code_rs, out_rs) = run_parity_fixture_with_args(
        &repo,
        &binaries.ts_cli,
        &binaries.rust_bin,
        &[],
        &[],
        "jest",
    );
    assert_eq!(code_ts, code_rs);

    let n_ts = normalize(out_ts, &repo);
    let n_rs = normalize(out_rs, &repo);
    assert_eq!(n_ts, n_rs);
    assert!(
        n_ts.contains("HTTP:"),
        "expected HTTP section in TS output:\n{n_ts}"
    );
}

#[test]
fn parity_jest_http_abort_card_fixture() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let repo = mk_repo("jest-http-abort-card", &binaries.node_modules);
    write_file(
        &repo.join("tests/http-abort.test.js"),
        "const http = require('node:http');\n\
\n\
test('GET /abort', async () => {\n\
  const server = http.createServer((_req, _res) => {\n\
    // Intentionally never respond so the client can abort.\n\
  });\n\
\n\
  await new Promise((resolve) => server.listen(0, resolve));\n\
  const port = server.address().port;\n\
\n\
  try {\n\
    const req = http.get(`http://127.0.0.1:${port}/abort`, (_res) => {\n\
      // keep connection open\n\
    });\n\
    await new Promise((resolve) => setTimeout(resolve, 10));\n\
    req.destroy();\n\
    await new Promise((resolve) => setTimeout(resolve, 10));\n\
\n\
    expect(1).toBe(2);\n\
  } finally {\n\
    await new Promise((resolve) => server.close(resolve));\n\
  }\n\
});\n",
    );
    write_jest_config(&repo, "**/tests/**/*.test.js");

    let (_spec, code_ts, out_ts, code_rs, out_rs) = run_parity_fixture_with_args(
        &repo,
        &binaries.ts_cli,
        &binaries.rust_bin,
        &[],
        &[],
        "jest",
    );
    assert_eq!(code_ts, code_rs);

    let n_ts = normalize(out_ts, &repo);
    let n_rs = normalize(out_rs, &repo);
    assert_eq!(n_ts, n_rs);
    assert!(
        n_ts.contains("connection aborted"),
        "expected connection aborted in TS output:\n{n_ts}"
    );
}

#[test]
fn parity_jest_unhandled_rejection_live_fixture() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let repo = mk_repo("jest-unhandled-rejection-live", &binaries.node_modules);
    write_file(
        &repo.join("tests/rej.test.js"),
        "test('rej', async () => {\n\
  Promise.reject(new Error('boom-rej'));\n\
  await new Promise((r) => setTimeout(r, 10));\n\
  expect(1).toBe(1);\n\
});\n",
    );
    write_jest_config(&repo, "**/tests/**/*.test.js");

    let (_spec, code_ts, out_ts, code_rs, out_rs) = run_parity_fixture_with_args(
        &repo,
        &binaries.ts_cli,
        &binaries.rust_bin,
        &[],
        &[],
        "jest",
    );
    assert_eq!(code_ts, code_rs);

    let n_ts = normalize(out_ts, &repo);
    let n_rs = normalize(out_rs, &repo);
    assert_eq!(n_ts, n_rs);
}

#[test]
fn parity_jest_no_live_progress_frames_when_not_tty_fixture() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let repo = mk_repo(
        "jest-no-live-progress-frames-not-tty",
        &binaries.node_modules,
    );
    write_file(
        &repo.join("tests/pass.test.js"),
        "test('pass', () => { expect(1).toBe(1); });\n",
    );
    write_jest_config(&repo, "**/tests/**/*.test.js");

    let (_spec, code_ts, out_ts, code_rs, out_rs) = run_parity_fixture_with_args(
        &repo,
        &binaries.ts_cli,
        &binaries.rust_bin,
        &[],
        &[],
        "jest",
    );
    assert_eq!(code_ts, code_rs);
    assert!(
        !out_ts.contains("\u{1b}[2K\rRUN "),
        "TS output unexpectedly contained live progress frames (not a TTY):\n{out_ts}"
    );
    assert!(
        !out_rs.contains("\u{1b}[2K\rRUN "),
        "Rust output unexpectedly contained live progress frames (not a TTY):\n{out_rs}"
    );
}
