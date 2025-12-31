use std::collections::HashMap;
use std::path::Path;

use serde::de::{DeserializeSeed, IgnoredAny, MapAccess, SeqAccess, Visitor};

use super::statement_id::statement_id_from_line_col;

pub fn read_repo_llvm_cov_json_statement_hits(
    repo_root: &Path,
) -> Option<HashMap<String, HashMap<u64, u32>>> {
    read_llvm_cov_json_statement_hits_from_path(
        repo_root,
        &repo_root.join("coverage").join("coverage.json"),
    )
}

pub fn read_llvm_cov_json_statement_hits_from_path(
    repo_root: &Path,
    json_path: &Path,
) -> Option<HashMap<String, HashMap<u64, u32>>> {
    let raw = std::fs::read(json_path).ok()?;
    parse_llvm_cov_json_statement_hits_bytes(&raw, repo_root).ok()
}

pub fn parse_llvm_cov_json_statement_hits(
    text: &str,
    repo_root: &Path,
) -> Result<HashMap<String, HashMap<u64, u32>>, String> {
    parse_llvm_cov_json_statement_hits_serde(text.as_bytes(), repo_root)
}

fn parse_llvm_cov_json_statement_hits_bytes(
    bytes: &[u8],
    repo_root: &Path,
) -> Result<HashMap<String, HashMap<u64, u32>>, String> {
    parse_llvm_cov_json_statement_hits_serde(bytes, repo_root)
}

pub fn read_repo_llvm_cov_json_statement_totals(
    repo_root: &Path,
) -> Option<HashMap<String, (u32, u32)>> {
    let hits = read_repo_llvm_cov_json_statement_hits(repo_root)?;
    Some(
        hits.iter()
            .map(|(path, by_id)| {
                let total = (by_id.len() as u64).min(u64::from(u32::MAX)) as u32;
                let covered = (by_id.values().filter(|h| **h > 0).count() as u64)
                    .min(u64::from(u32::MAX)) as u32;
                (path.to_string(), (total, covered))
            })
            .collect::<HashMap<_, _>>(),
    )
}

pub fn read_llvm_cov_json_statement_totals_from_path(
    repo_root: &Path,
    json_path: &Path,
) -> Option<HashMap<String, (u32, u32)>> {
    let hits = read_llvm_cov_json_statement_hits_from_path(repo_root, json_path)?;
    Some(
        hits.iter()
            .map(|(path, by_id)| {
                let total = (by_id.len() as u64).min(u64::from(u32::MAX)) as u32;
                let covered = (by_id.values().filter(|h| **h > 0).count() as u64)
                    .min(u64::from(u32::MAX)) as u32;
                (path.to_string(), (total, covered))
            })
            .collect::<HashMap<_, _>>(),
    )
}

pub fn parse_llvm_cov_json_statement_totals(
    text: &str,
    repo_root: &Path,
) -> Result<HashMap<String, (u32, u32)>, String> {
    let hits = parse_llvm_cov_json_statement_hits(text, repo_root)?;
    Ok(hits
        .into_iter()
        .map(|(path, by_id)| {
            let total = (by_id.len() as u64).min(u64::from(u32::MAX)) as u32;
            let covered =
                (by_id.values().filter(|h| **h > 0).count() as u64).min(u64::from(u32::MAX)) as u32;
            (path, (total, covered))
        })
        .collect::<HashMap<_, _>>())
}

fn parse_llvm_cov_json_statement_hits_serde(
    bytes: &[u8],
    repo_root: &Path,
) -> Result<HashMap<String, HashMap<u64, u32>>, String> {
    struct RootSeed<'a> {
        repo_root: &'a Path,
        hits_by_path: &'a mut HashMap<String, HashMap<u64, u32>>,
    }

    impl<'de> DeserializeSeed<'de> for RootSeed<'_> {
        type Value = ();
        fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            deserializer.deserialize_any(AnyValueVisitor {
                repo_root: self.repo_root,
                hits_by_path: self.hits_by_path,
            })
        }
    }

    struct AnyValueVisitor<'a> {
        repo_root: &'a Path,
        hits_by_path: &'a mut HashMap<String, HashMap<u64, u32>>,
    }

    impl<'de> Visitor<'de> for AnyValueVisitor<'_> {
        type Value = ();
        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("any JSON value")
        }

        fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
        where
            A: MapAccess<'de>,
        {
            while let Some(key) = map.next_key::<std::borrow::Cow<'de, str>>()? {
                match key.as_ref() {
                    "files" => {
                        map.next_value_seed(FilesSeed {
                            repo_root: self.repo_root,
                            hits_by_path: self.hits_by_path,
                        })?;
                    }
                    _ => {
                        map.next_value_seed(RootSeed {
                            repo_root: self.repo_root,
                            hits_by_path: self.hits_by_path,
                        })?;
                    }
                }
            }
            Ok(())
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            while (seq.next_element_seed(RootSeed {
                repo_root: self.repo_root,
                hits_by_path: self.hits_by_path,
            })?)
            .is_some()
            {}
            Ok(())
        }

        fn visit_bool<E>(self, _v: bool) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(())
        }
        fn visit_i64<E>(self, _v: i64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(())
        }
        fn visit_u64<E>(self, _v: u64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(())
        }
        fn visit_f64<E>(self, _v: f64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(())
        }
        fn visit_str<E>(self, _v: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(())
        }
        fn visit_string<E>(self, _v: String) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(())
        }
        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(())
        }
        fn visit_unit<E>(self) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(())
        }
    }

    struct FilesSeed<'a> {
        repo_root: &'a Path,
        hits_by_path: &'a mut HashMap<String, HashMap<u64, u32>>,
    }

    impl<'de> DeserializeSeed<'de> for FilesSeed<'_> {
        type Value = ();
        fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            deserializer.deserialize_any(FilesVisitor {
                repo_root: self.repo_root,
                hits_by_path: self.hits_by_path,
            })
        }
    }

    struct FilesVisitor<'a> {
        repo_root: &'a Path,
        hits_by_path: &'a mut HashMap<String, HashMap<u64, u32>>,
    }

    impl<'de> Visitor<'de> for FilesVisitor<'_> {
        type Value = ();
        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("files array")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            while (seq.next_element_seed(FileSeed {
                repo_root: self.repo_root,
                hits_by_path: self.hits_by_path,
            })?)
            .is_some()
            {}
            Ok(())
        }
    }

    struct FileSeed<'a> {
        repo_root: &'a Path,
        hits_by_path: &'a mut HashMap<String, HashMap<u64, u32>>,
    }

    impl<'de> DeserializeSeed<'de> for FileSeed<'_> {
        type Value = ();
        fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            deserializer.deserialize_any(FileVisitor {
                repo_root: self.repo_root,
                hits_by_path: self.hits_by_path,
            })
        }
    }

    struct FileVisitor<'a> {
        repo_root: &'a Path,
        hits_by_path: &'a mut HashMap<String, HashMap<u64, u32>>,
    }

    impl<'de> Visitor<'de> for FileVisitor<'_> {
        type Value = ();
        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("file object")
        }

        fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
        where
            A: MapAccess<'de>,
        {
            let mut filename: Option<String> = None;
            let mut pending: Option<HashMap<u64, u32>> = None;
            while let Some(key) = map.next_key::<std::borrow::Cow<'de, str>>()? {
                match key.as_ref() {
                    "filename" => {
                        filename = Some(map.next_value::<String>()?);
                    }
                    "segments" => {
                        if let Some(name) = filename.as_deref() {
                            let normalized =
                                crate::coverage::lcov::normalize_lcov_path(name, self.repo_root);
                            let entry = self.hits_by_path.entry(normalized).or_default();
                            map.next_value_seed(SegmentsSeed { target: entry })?;
                        } else {
                            let mut tmp: HashMap<u64, u32> = HashMap::new();
                            map.next_value_seed(SegmentsSeed { target: &mut tmp })?;
                            pending = Some(tmp);
                        }
                    }
                    _ => {
                        map.next_value::<IgnoredAny>()?;
                    }
                }
            }
            if let (Some(name), Some(mut tmp)) = (filename, pending) {
                let normalized = crate::coverage::lcov::normalize_lcov_path(&name, self.repo_root);
                let entry = self.hits_by_path.entry(normalized).or_default();
                tmp.drain().for_each(|(statement_id, hit)| {
                    insert_max(entry, statement_id, hit);
                });
            }
            Ok(())
        }
    }

    struct SegmentsSeed<'a> {
        target: &'a mut HashMap<u64, u32>,
    }

    impl<'de> DeserializeSeed<'de> for SegmentsSeed<'_> {
        type Value = ();
        fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            deserializer.deserialize_any(SegmentsVisitor {
                target: self.target,
            })
        }
    }

    struct SegmentsVisitor<'a> {
        target: &'a mut HashMap<u64, u32>,
    }

    impl<'de> Visitor<'de> for SegmentsVisitor<'_> {
        type Value = ();
        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("segments array")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            while (seq.next_element_seed(SegmentEntrySeed {
                target: self.target,
            })?)
            .is_some()
            {}
            Ok(())
        }
    }

    struct SegmentEntrySeed<'a> {
        target: &'a mut HashMap<u64, u32>,
    }

    impl<'de> DeserializeSeed<'de> for SegmentEntrySeed<'_> {
        type Value = ();
        fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            deserializer.deserialize_any(SegmentEntryVisitor {
                target: self.target,
            })
        }
    }

    struct SegmentEntryVisitor<'a> {
        target: &'a mut HashMap<u64, u32>,
    }

    impl<'de> Visitor<'de> for SegmentEntryVisitor<'_> {
        type Value = ();
        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("segment entry array")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let Some(line) = seq.next_element::<u64>()? else {
                return Ok(());
            };
            let Some(col) = seq.next_element::<u64>()? else {
                return Ok(());
            };
            let Some(hit) = seq.next_element::<u64>()? else {
                return Ok(());
            };
            let Some(has_count) = seq.next_element::<bool>()? else {
                return Ok(());
            };
            let Some(is_region_entry) = seq.next_element::<bool>()? else {
                return Ok(());
            };
            let Some(is_gap_region) = seq.next_element::<bool>()? else {
                return Ok(());
            };

            while seq.next_element::<IgnoredAny>()?.is_some() {}

            if has_count && is_region_entry && !is_gap_region && line > 0 {
                let line_u32 = (line.min(u64::from(u32::MAX))) as u32;
                let col_u32 = (col.min(u64::from(u32::MAX))) as u32;
                let hit_u32 = (hit.min(u64::from(u32::MAX))) as u32;
                let statement_id = statement_id_from_line_col(line_u32, col_u32);
                insert_max(self.target, statement_id, hit_u32);
            }
            Ok(())
        }
    }

    fn insert_max(target: &mut HashMap<u64, u32>, statement_id: u64, hit: u32) {
        match target.get(&statement_id).copied() {
            Some(prev) if prev >= hit => return,
            _ => {}
        }
        target.insert(statement_id, hit);
    }

    let mut hits_by_path: HashMap<String, HashMap<u64, u32>> = HashMap::new();
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    RootSeed {
        repo_root,
        hits_by_path: &mut hits_by_path,
    }
    .deserialize(&mut deserializer)
    .map_err(|e| e.to_string())?;
    Ok(hits_by_path)
}
