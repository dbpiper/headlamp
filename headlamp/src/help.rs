pub fn help_text() -> &'static str {
    r#"headlamp

Usage:
  headlamp [--runner=<jest|pytest|cargo-nextest|cargo-test>] [--coverage] [--changed[=<mode>]] [args...]

Flags:
  --runner <runner>                         Select runner (default: jest)
  --coverage                                Enable coverage collection (runner-specific)
  --coverage-ui=jest|both                   Coverage output mode
  --coverage.abortOnFailure                 Exit on test failures without printing coverage
  --coverage.detail=<all|auto|n>            Coverage detail level
  --coverage.showCode[=true|false]          Show code under failing lines (default: true in TTY)
  --coverage.mode=<auto|full|compact>       Coverage UI mode
  --coverage.compact                        Shorthand for --coverage.mode=compact
  --coverage.maxFiles=<n>                   Max files shown in coverage output
  --coverage.maxHotspots=<n>                Max hotspots shown in coverage output
  --coverage.thresholds.lines=<n>           Minimum line coverage threshold (0.0-1.0)
  --coverage.thresholds.functions=<n>       Minimum function coverage threshold (0.0-1.0)
  --coverage.thresholds.branches=<n>        Minimum branch coverage threshold (0.0-1.0)
  --coverage.thresholds.statements=<n>      Minimum statement coverage threshold (0.0-1.0)
  --coverage.pageFit[=true|false]           Fit coverage output to terminal width (default: true in TTY)
  --coverage.include=<glob,...>             Include globs for coverage (comma-separated)
  --coverage.exclude=<glob,...>             Exclude globs for coverage (comma-separated)
  --coverage.editor=<cmd>                   Editor command for file links
  --coverage.root=<path>                    Workspace root override
  --onlyFailures[=true|false]               Show only failing tests during live output
  --showLogs[=true|false]                   Show full logs under failing tests
  --sequential[=true|false]                 Serialize execution (e.g. jest --runInBand)
  --watch[=true|false]                      Re-run on file changes (polling watch)
  --watchAll[=true|false]                   Watch everything (runner-specific)
  --ci[=true|false]                         CI mode (disable interactive UI and set CI=1)
  --verbose[=true|false]                    More Headlamp diagnostics
  --no-cache[=true|false]                   Disable Headlamp caches (and runner caches when possible)
  --bootstrapCommand <cmd>                  Run once before tests (npm script name or shell cmd)
  --changed[=all|staged|unstaged|branch|lastCommit|lastRelease]
  --changed.depth=<n>                       Max dependency depth for changed selection
  --dependency-language=<tsjs|rust>         Dependency language for selection (where applicable)
  --dependencyLanguage=<tsjs|rust>          Alias for --dependency-language

Notes:
  Unknown args are forwarded to the runner.
"#
}
