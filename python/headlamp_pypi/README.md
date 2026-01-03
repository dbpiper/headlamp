# Headlamp

Headlamp is a **Rust-powered test UX CLI**: smarter test selection, cleaner output, and a unified workflow across **jest**, **Rust tests (headlamp runner)**, **cargo test**, **cargo nextest**, and **pytest**.

Headlamp is useful when you want a consistent way to run tests across different projects and keep feedback fast as your repo grows. It can select tests based on what changed, surface failures in a readable format, and keep common defaults (like runner args and coverage settings) in a single config file so your team doesn’t have to remember a long list of flags.

## Why Headlamp

- **One CLI, many runners**: `--runner=headlamp|jest|cargo-nextest|cargo-test|pytest`
- **Selection that scales**: run what changed (`--changed`) and what’s related (dependency-graph driven)
- **Coverage-first UX**: coverage output you can actually read
- **Fast**: Rust core + caching

## Installation

### npm (Node.js projects)

Requirements:

- Node **>= 18**

Install:

```bash
npm i -D headlamp
```

Run:

```bash
npx headlamp --help
```

### Cargo (Rust projects)

Install from crates.io:

```bash
cargo install headlamp
```

### Python (pytest projects)

Install:

```bash
pip install headlamp
```

```bash
headlamp --runner=pytest
```

## Peer dependencies (system requirements)

Headlamp is a wrapper around your project’s runners. It does **not** vendor the runners themselves, so you need the runner executables available in your environment for the features you’re using.

### Common (all runners)

- **Git**: required for `--changed=...` modes (e.g. `--changed=branch`).

### Jest runner (`--runner=jest`)

- **Node.js**: required.
- **Jest installed in the repo**: Headlamp expects Jest to be runnable from your project (typically `./node_modules/.bin/jest`).
- **Coverage** (`--coverage`): requires Jest coverage support (standard Jest `--coverage` + reporters). Headlamp formats/prints coverage from generated reports.

### Pytest runner (`--runner=pytest`)

- **Python 3**: required.
- **pytest**: must be on `PATH` (`pytest` / `pytest.exe`).
- **Coverage** (`--coverage`): requires `pytest-cov` (Headlamp enables coverage and passes `--cov` flags; branch coverage uses `--cov-branch`).

### Headlamp (native Rust runner) (`--runner=headlamp`)

- **Rust toolchain**: `cargo` + `rustc`.
- **Per-test timings**: requires a preinstalled nightly toolchain (Headlamp enables libtest JSON + `--report-time` only when nightly is available).
  - Install via: `rustup toolchain install nightly`

### Cargo test runner (`--runner=cargo-test`)

- **Rust toolchain**: `cargo` + `rustc`.
- **Coverage** (`--coverage`): collected via LLVM tools from `rustup` (**no `cargo-llvm-cov` dependency**).
  - Install via: `rustup component add llvm-tools-preview`

### Cargo nextest runner (`--runner=cargo-nextest`)

- **Rust toolchain**: `cargo` + `rustc`.
- **nextest**: requires **`cargo-nextest`** (`cargo install cargo-nextest`).
- **Coverage** (`--coverage`): collected via LLVM tools from `rustup` (**no `cargo-llvm-cov` dependency**).
  - Install via: `rustup component add llvm-tools-preview`

## Quickstart

### Jest

```bash
npx headlamp --runner=jest
```

Forward runner args after `--` (unknown args are forwarded):

```bash
npx headlamp --runner=jest -- --runInBand
```

### Cargo nextest / cargo test

```bash
headlamp --runner=cargo-nextest
headlamp --runner=cargo-test
```

Requirements:

- `--runner=cargo-nextest`: requires `cargo-nextest` to be installed.
  - Install via: `cargo install cargo-nextest` (or your preferred installer)

## CLI

Run `headlamp --help` to see the up-to-date flags list.

Highlights:

- **runners**: `--runner=headlamp|jest|pytest|cargo-nextest|cargo-test`
- **changed selection**: `--changed=all|staged|unstaged|branch|lastCommit|lastRelease`
  - `lastRelease` selects changes since the previous stable SemVer release tag
- **coverage**: `--coverage` plus `--coverage-ui`, `--coverage-detail`, thresholds, etc.
- **artifacts** (default: none): `--keep-artifacts` to keep runner artifacts on disk

Legacy aliases (still accepted, but not recommended):

- `--keepArtifacts`
- `--coverage.detail`

## Configuration

Headlamp discovers config from your repo root. Supported file names:

- `headlamp.toml` (highest precedence)
- `headlamp.config.ts`
- `headlamp.config.js`
- `headlamp.config.mjs`
- `headlamp.config.cjs`
- `headlamp.config.json`
- `headlamp.config.json5`
- `headlamp.config.jsonc`
- `headlamp.config.yaml`
- `headlamp.config.yml`
- `.headlamprc` plus `.headlamprc.*` variants (`.json`, `.json5`, `.jsonc`, `.yaml`, `.yml`, `.js`, `.cjs`, `.mjs`, `.ts`)

Headlamp also supports embedded TOML config (lower precedence than explicit config files):

- `pyproject.toml` under `[tool.headlamp]`
- `Cargo.toml` under `[package.metadata.headlamp]`

### Example: `headlamp.toml` (recommended for Rust + Python)

```toml
# Run tests sequentially (useful for very heavy integration tests)
sequential = true

[coverage]
abort_on_failure = true
mode = "auto"
page_fit = true
keep_artifacts = false

[changed]
depth = 20
```

### Example: `headlamp.config.ts`

Rules:

- Must have a **default export**
- Only **relative imports** are supported inside the config file (`./` and `../`)

```ts
export default {
  // Runner defaults
  jestArgs: ["--runInBand"],

  // Run once before tests (npm script name or a shell command)
  bootstrapCommand: "test:jest:bootstrap",

  // Global toggles
  ci: false,
  verbose: false,
  noCache: false,
  keepArtifacts: false,

  // Coverage defaults
  coverage: true,
  coverageUi: "both",
  coverage: {
    abortOnFailure: true,
    mode: "auto",
    pageFit: true,
  },

  // Changed selection defaults
  changed: { depth: 2 },
};
```

## Artifacts (coverage, caches, temp files)

By default, headlamp runs **artifact-free**: it uses an ephemeral per-run workspace and **does not leave files behind** in your repo (e.g. `coverage/`, `.coverage`, `.pytest_cache`, `target/`) or OS temp.

If you need artifacts on disk (for example, to upload coverage reports in CI), opt out:

- CLI: `--keep-artifacts`
- Config: `keepArtifacts: true`

## Contributing

Pull requests are welcome. For large changes, open an issue first to align on direction.

## Support

- Bug reports and feature requests: GitHub Issues

## License

MIT — see `LICENSE`.
