### Headlamp

Headlamp is a **Rust-powered test runner CLI** that helps you run the right tests quickly and read the results easily across **jest**, **cargo test**, and **cargo nextest**.

Headlamp is useful when you want a consistent way to run tests across different projects and keep feedback fast as your repo grows. It can select tests based on what changed, surface failures in a readable format, and keep common defaults (like runner args and coverage settings) in a single config file so your team doesn’t have to remember a long list of flags.

### Why Headlamp

- **One CLI, many runners**: `--runner=jest|cargo-nextest|cargo-test`
- **Selection that scales**: run what changed (`--changed`) and what’s related (dependency-graph driven)
- **Coverage-first UX**: coverage output you can actually read
- **Fast**: Rust core + caching

### Installation

#### npm (recommended)

Requirements:

- Node **>= 18**
- A GitHub Release for your version tag (see “Releases” below)

Install:

```bash
npm i -D headlamp
```

Run:

```bash
npx headlamp --help
```

#### Cargo (from source)

```bash
cargo install --path headlamp
```

### Quickstart

#### Jest

```bash
npx headlamp --runner=jest
```

Forward runner args after `--` (unknown args are forwarded):

```bash
npx headlamp --runner=jest -- --runInBand
```

#### Cargo nextest / cargo test

```bash
headlamp --runner=cargo-nextest
headlamp --runner=cargo-test
```

### CLI

Current `--help` output:

```text
headlamp

Usage:
  headlamp [--runner=<jest|vitest|pytest|cargo-nextest|cargo-test>] [--coverage] [--changed[=<mode>]] [args...]

Flags:
  --runner <runner>              Select runner (default: jest)
  --coverage                     Enable coverage collection (runner-specific)
  --coverage-ui=jest|both         Coverage output mode
  --coverage.abortOnFailure       Exit on test failures without printing coverage
  --watch                        Re-run on file changes (runner-agnostic polling watch)
  --ci                           CI mode (disable interactive UI and set CI=1)
  --verbose                      More Headlamp diagnostics
  --no-cache                     Disable Headlamp caches (and runner caches when possible)
  --onlyFailures                 Show only failing tests during live output
  --showLogs                     Show full logs under failing tests
  --sequential                   Serialize execution (maps to jest --runInBand)
  --bootstrapCommand <cmd>       Run once before tests (npm script name or shell cmd)
  --changed[=all|staged|unstaged|branch|lastCommit]
  --changed.depth=<n>

Notes:
  Unknown args are forwarded to the runner.
```

### Configuration

Headlamp discovers config from your repo root. Supported file names:

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

#### Example: `headlamp.config.ts`

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

### Contributing

Pull requests are welcome. For large changes, open an issue first to align on direction.

Local dev:

```bash
cargo test
```

### Support

- Bug reports and feature requests: GitHub Issues

### License

MIT — see `LICENSE`.
