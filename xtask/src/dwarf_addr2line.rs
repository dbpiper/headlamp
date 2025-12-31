use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedLocation {
    pub function_name: String,
    pub file_path: Option<String>,
    pub line_number: Option<u32>,
}

pub fn resolve_locations_with_llvm_addr2line(
    dwarf_executable_path: &Path,
    addresses: &[u64],
) -> anyhow::Result<Vec<ResolvedLocation>> {
    if addresses.is_empty() {
        return Ok(Vec::new());
    }

    let mut child = Command::new("xcrun")
        .arg("llvm-addr2line")
        .arg("-f")
        .arg("-p")
        .arg("-C")
        .arg("-e")
        .arg(dwarf_executable_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| {
            anyhow::anyhow!(
                "failed to spawn xcrun llvm-addr2line for {}: {error}",
                dwarf_executable_path.display()
            )
        })?;

    {
        let Some(mut stdin) = child.stdin.take() else {
            anyhow::bail!("failed to capture llvm-addr2line stdin");
        };
        for address in addresses {
            writeln!(stdin, "0x{address:016x}")?;
        }
    }

    let Some(stdout) = child.stdout.take() else {
        anyhow::bail!("failed to capture llvm-addr2line stdout");
    };
    let mut stdout_reader = BufReader::new(stdout);

    let mut resolved: Vec<ResolvedLocation> = Vec::with_capacity(addresses.len());
    let mut line_buffer = String::new();
    while resolved.len() < addresses.len() {
        line_buffer.clear();
        let bytes_read = stdout_reader.read_line(&mut line_buffer)?;
        if bytes_read == 0 {
            break;
        }
        let Some(parsed) = parse_addr2line_output_line(&line_buffer) else {
            continue;
        };
        resolved.push(parsed);
    }

    let status = child.wait()?;
    if !status.success() {
        anyhow::bail!("llvm-addr2line exited with status {status}");
    }

    if resolved.len() != addresses.len() {
        anyhow::bail!(
            "llvm-addr2line returned {} lines for {} addresses",
            resolved.len(),
            addresses.len()
        );
    }

    Ok(resolved)
}

pub fn parse_addr2line_output_line(line: &str) -> Option<ResolvedLocation> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    let at_separator = " at ";
    let at_offset = trimmed.rfind(at_separator)?;
    let function_name = trimmed[..at_offset].trim().to_string();
    let location_text = trimmed[(at_offset + at_separator.len())..].trim();

    let (file_path, line_number) = parse_location_text(location_text);
    Some(ResolvedLocation {
        function_name,
        file_path,
        line_number,
    })
}

fn parse_location_text(location_text: &str) -> (Option<String>, Option<u32>) {
    if location_text == "??:0" || location_text == "??:?" {
        return (None, None);
    }

    let last_colon_offset = match location_text.rfind(':') {
        Some(offset) => offset,
        None => return (Some(location_text.to_string()), None),
    };

    let file_path = location_text[..last_colon_offset].to_string();
    let line_text = location_text[(last_colon_offset + 1)..].trim();
    let line_number = line_text.parse::<u32>().ok();
    (Some(file_path), line_number)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DwarfExecutablePaths {
    pub binary_path: PathBuf,
    pub dwarf_path: PathBuf,
}
