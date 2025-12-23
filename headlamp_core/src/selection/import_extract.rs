use std::path::Path;

use duct::cmd as duct_cmd;
use which::which;

const RG_IMPORT_FROM: &str = r#"import\s+[^'"\n]*from\s+['"]([^'"]+)['"]"#;
const RG_REQUIRE: &str = r#"require\(\s*['"]([^'"]+)['"]\s*\)"#;
const RG_EXPORT_FROM: &str = r#"export\s+(?:\*|\{[^}]*\})\s*from\s*['"]([^'"]+)['"]"#;
const RG_PATTERNS: [&str; 3] = [RG_IMPORT_FROM, RG_REQUIRE, RG_EXPORT_FROM];

pub fn extract_import_specs(abs_path: &Path) -> Vec<String> {
    let Ok(rg) = which("rg") else {
        return vec![];
    };
    if !abs_path.exists() {
        return vec![];
    }

    // NOTE: ripgrep's `--replace $1` only reliably captures for a single `-e` at a time.
    // Run once per pattern and union results to match headlamp-original behavior.
    let mut out: Vec<String> = vec![];
    for pattern in RG_PATTERNS {
        let args: Vec<String> = vec![
            "--pcre2",
            "--no-filename",
            "--no-line-number",
            "--max-columns=200",
            "--max-columns-preview",
            "--no-messages",
            "-o",
            "--replace",
            "$1",
            "-e",
            pattern,
            abs_path.to_string_lossy().as_ref(),
        ]
        .into_iter()
        .map(str::to_string)
        .collect();

        let Ok(result) = duct_cmd(&rg, args)
            .env("CI", "1")
            .stderr_capture()
            .stdout_capture()
            .unchecked()
            .run()
        else {
            continue;
        };
        let text = String::from_utf8_lossy(&result.stdout);
        out.extend(
            text.lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_string),
        );
    }

    out.sort();
    out.dedup();
    out
}
