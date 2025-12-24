use std::path::Path;

use tempfile::TempDir;

use headlamp_core::config::{discover_config_path, load_headlamp_config_from_path};

fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, contents).unwrap();
}

#[test]
fn config_discovery_prefers_headlamp_config_over_rc_when_both_exist() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    write_file(&root.join(".headlamprc.json"), r#"{ "sequential": true }"#);
    write_file(
        &root.join("headlamp.config.json"),
        r#"{ "sequential": false }"#,
    );

    let discovered = discover_config_path(root).unwrap();
    assert!(discovered.ends_with("headlamp.config.json"));

    let loaded = load_headlamp_config_from_path(&discovered).unwrap();
    assert_eq!(loaded.sequential, Some(false));
}

#[test]
fn config_loads_yaml() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("headlamp.config.yml");
    write_file(
        &path,
        r#"
sequential: true
bootstrapCommand: test:bootstrap
"#,
    );
    let cfg = load_headlamp_config_from_path(&path).unwrap();
    assert_eq!(cfg.sequential, Some(true));
    assert_eq!(cfg.bootstrap_command.as_deref(), Some("test:bootstrap"));
}

#[test]
fn config_loads_json5_with_comments() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("headlamp.config.json5");
    write_file(
        &path,
        r#"
// comment
{
  sequential: true,
  jestArgs: ["--runInBand"],
}
"#,
    );
    let cfg = load_headlamp_config_from_path(&path).unwrap();
    assert_eq!(cfg.sequential, Some(true));
    assert_eq!(
        cfg.jest_args.clone().unwrap_or_default(),
        vec!["--runInBand".to_string()]
    );
}

#[test]
fn config_loads_js_when_node_is_available() {
    if which::which("node").is_err() {
        return;
    }
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("headlamp.config.js");
    write_file(
        &path,
        r#"
module.exports = {
  sequential: true,
  coverageUi: 'jest',
  ci: true,
  verbose: true,
  noCache: true,
};
"#,
    );
    let cfg = load_headlamp_config_from_path(&path).unwrap();
    assert_eq!(cfg.sequential, Some(true));
    assert_eq!(
        cfg.coverage_ui.map(|v| format!("{v:?}")),
        Some("Jest".to_string())
    );
    assert_eq!(cfg.ci, Some(true));
    assert_eq!(cfg.verbose, Some(true));
    assert_eq!(cfg.no_cache, Some(true));
}

#[test]
fn config_loads_ts_static_object_without_node_runtime() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("headlamp.config.ts");
    write_file(
        &path,
        r#"
// Project-wide headlamp defaults (TS)
const config = {
  bootstrapCommand: 'test:jest:bootstrap',
  sequential: true,
  jestArgs: ['--no-watchman'],
  coverage: {
    abortOnFailure: true,
    mode: 'auto' as const,
    pageFit: true,
  },
  changed: {
    depth: 20,
  } as const,
};

export default config;
"#,
    );

    let cfg = load_headlamp_config_from_path(&path).unwrap();
    assert_eq!(
        cfg.bootstrap_command.as_deref(),
        Some("test:jest:bootstrap")
    );
    assert_eq!(cfg.sequential, Some(true));
    assert_eq!(
        cfg.jest_args.clone().unwrap_or_default(),
        vec!["--no-watchman".to_string()]
    );
    match cfg.changed {
        Some(headlamp_core::config::ChangedConfig::Obj(section)) => {
            assert_eq!(section.depth, Some(20));
        }
        other => panic!("expected changed obj, got {other:?}"),
    }
}

#[test]
fn config_loads_ts_define_config_wrapper() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("headlamp.config.ts");
    write_file(
        &path,
        r#"
export default defineConfig({
  sequential: true,
  changed: { depth: 20 } as const,
});
"#,
    );

    let cfg = load_headlamp_config_from_path(&path).unwrap();
    assert_eq!(cfg.sequential, Some(true));
    match cfg.changed {
        Some(headlamp_core::config::ChangedConfig::Obj(section)) => {
            assert_eq!(section.depth, Some(20));
        }
        other => panic!("expected changed obj, got {other:?}"),
    }
}

#[test]
fn config_loads_ts_import_and_spread() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let shared_path = root.join("shared.ts");
    write_file(
        &shared_path,
        r#"
export const sharedCoverage = {
  coverage: {
    abortOnFailure: true,
  },
};
"#,
    );

    let config_path = root.join("headlamp.config.ts");
    write_file(
        &config_path,
        r#"
import { sharedCoverage } from './shared';

export default defineConfig({
  ...sharedCoverage,
  sequential: true,
});
"#,
    );

    let cfg = load_headlamp_config_from_path(&config_path).unwrap();
    assert_eq!(cfg.sequential, Some(true));
    match cfg.coverage {
        Some(headlamp_core::config::CoverageConfig::Obj(section)) => {
            assert_eq!(section.abort_on_failure, Some(true));
        }
        other => panic!("expected coverage obj, got {other:?}"),
    }
}

#[test]
fn config_load_ts_rejects_dynamic_expressions_with_clear_error() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("headlamp.config.ts");
    write_file(
        &path,
        r#"
export default defineConfig({
  sequential: Boolean(process.env.X),
});
"#,
    );

    let err = load_headlamp_config_from_path(&path).unwrap_err();
    let text = format!("{err}");
    assert!(
        text.to_ascii_lowercase().contains("unsupported"),
        "expected clear unsupported message, got: {text}"
    );
}
