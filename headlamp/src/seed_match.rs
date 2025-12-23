use std::path::Path;

use aho_corasick::AhoCorasick;

pub struct SeedMatcher {
    ac: Option<AhoCorasick>,
    fallback: Vec<Box<str>>,
}

impl SeedMatcher {
    pub fn new(seeds: &[String]) -> Option<Self> {
        let fallback = seeds
            .iter()
            .map(|s| s.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.into())
            .collect::<Vec<Box<str>>>();
        if fallback.is_empty() {
            return None;
        }
        let patterns = fallback.iter().map(|s| s.as_ref()).collect::<Vec<&str>>();
        let ac = AhoCorasick::new(patterns).ok();
        Some(Self { ac, fallback })
    }

    pub fn is_match_bytes(&self, bytes: &[u8]) -> bool {
        if let Some(ac) = self.ac.as_ref() {
            return ac.find(bytes).is_some();
        }
        self.fallback
            .iter()
            .any(|s| !s.is_empty() && bytes.windows(s.len()).any(|w| w == s.as_bytes()))
    }

    pub fn is_match_file_name_or_body(&self, path: &Path) -> bool {
        let name = path.file_name().and_then(|x| x.to_str()).unwrap_or("");
        if self.is_match_bytes(name.as_bytes()) {
            return true;
        }
        let body = std::fs::read_to_string(path).unwrap_or_default();
        self.is_match_bytes(body.as_bytes())
    }
}
