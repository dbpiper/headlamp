use crate::format::ansi;
use crate::format::stacks::strip_ansi_simple;

#[derive(Debug, Clone)]
pub struct ConsoleEntry {
    pub type_name: Option<String>,
    pub message: Option<String>,
    pub origin: Option<String>,
    pub test_path: Option<String>,
    pub current_test_name: Option<String>,
}

pub fn build_console_section(entries: &[ConsoleEntry], full: bool) -> Vec<String> {
    if entries.is_empty() {
        return vec![];
    }

    if full {
        let lines = entries
            .iter()
            .map(|e| {
                let type_text = e.type_name.clone().unwrap_or_default().to_lowercase();
                let msg = e.message.clone().unwrap_or_default();
                let origin = e.origin.clone().unwrap_or_default();
                let type_fmt = if type_text.is_empty() {
                    String::new()
                } else {
                    format!("{}: ", ansi::white(&type_text))
                };
                let origin_fmt = if origin.is_empty() {
                    String::new()
                } else {
                    format!(" {}", ansi::dim(&format!("({origin})")))
                };
                format!("      {} {}{}{}", ansi::dim("•"), type_fmt, msg, origin_fmt)
            })
            .filter(|ln| !strip_ansi_simple(ln).trim().is_empty())
            .rev()
            .take(150)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>();

        if lines.is_empty() {
            return vec![];
        }

        vec![ansi::dim("    Logs:")]
            .into_iter()
            .chain(lines)
            .chain([String::new()])
            .collect()
    } else {
        let mut scored = entries
            .iter()
            .filter(|e| e.type_name.as_deref().unwrap_or_default().to_lowercase() == "error")
            .map(|e| {
                let msg = e.message.clone().unwrap_or_default();
                let score = msg.len();
                (msg, score)
            })
            .filter(|(msg, _)| !msg.trim().is_empty())
            .collect::<Vec<_>>();

        scored.sort_by(|(_, s1), (_, s2)| s2.cmp(s1));
        let top = scored.into_iter().take(3).collect::<Vec<_>>();
        if top.is_empty() {
            return vec![];
        }

        let mut out = vec![ansi::dim("    Console errors:")];
        for (msg, _) in top {
            out.push(format!("      {} {}", ansi::dim("•"), msg));
        }
        out.push(String::new());
        out
    }
}
