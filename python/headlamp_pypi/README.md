# Headlamp

Headlamp is a **Rust-powered test UX CLI**: smarter test selection, cleaner output, and a unified workflow across **jest**, **cargo test**, **cargo nextest**, and **pytest**.

Headlamp is useful when you want a consistent way to run tests across different projects and keep feedback fast as your repo grows. It can select tests based on what changed, surface failures in a readable format, and keep common defaults (like runner args and coverage settings) in a single config file so your team doesn’t have to remember a long list of flags.

## Why Headlamp

- **One CLI, many runners**: `--runner=jest|cargo-nextest|cargo-test|pytest`
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

## CLI

Run `headlamp --help` to see the up-to-date flags list.

Highlights:

- **runners**: `--runner=jest|pytest|cargo-nextest|cargo-test`
- **changed selection**: `--changed=all|staged|unstaged|branch|lastCommit|lastRelease`
  - `lastRelease` selects changes since the previous stable SemVer release tag
- **coverage**: `--coverage` plus `--coverage-ui`, `--coverage.detail`, thresholds, etc.

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

## Contributing

Pull requests are welcome. For large changes, open an issue first to align on direction.

## Support

- Bug reports and feature requests: GitHub Issues

## License

MIT — see `LICENSE`.
