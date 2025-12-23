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
};
"#,
    );
    let cfg = load_headlamp_config_from_path(&path).unwrap();
    assert_eq!(cfg.sequential, Some(true));
    assert_eq!(
        cfg.coverage_ui.map(|v| format!("{v:?}")),
        Some("Jest".to_string())
    );
}
