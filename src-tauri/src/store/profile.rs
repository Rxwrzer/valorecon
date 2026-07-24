/// Persistent per-account lifetime stats store.
/// Mirrors backend/profile_store.py — stores one JSON record per match.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::models::PlayerStats;

#[derive(Debug, Serialize, Deserialize, Default)]
struct StoreData {
    #[serde(default)]
    matches: HashMap<String, Value>,   // match_id -> statline
    #[serde(default)]
    skipped: HashSet<String>,
    #[serde(default)]
    scanned: i32,
}

pub struct ProfileStore {
    pub puuid: String,
    path: PathBuf,
    data: StoreData,
}

impl ProfileStore {
    pub fn open(dir: &Path, puuid: &str) -> Self {
        let path = dir.join(format!("{puuid}.json"));
        let data = if let Ok(text) = std::fs::read_to_string(&path) {
            serde_json::from_str(&text).unwrap_or_default()
        } else {
            StoreData::default()
        };
        Self { puuid: puuid.to_string(), path, data }
    }

    pub fn save(&self) {
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(text) = serde_json::to_string(&self.data) {
            let _ = std::fs::write(&self.path, text);
        }
    }

    pub fn add(&mut self, match_id: &str, rec: Value) {
        self.data.matches.insert(match_id.to_string(), rec);
    }

    pub fn known_ids(&self) -> HashSet<String> {
        self.data.matches.keys().cloned().collect()
    }

    pub fn record(&self, match_id: &str) -> Option<&Value> {
        self.data.matches.get(match_id)
    }

    pub fn skipped_ids(&self) -> HashSet<String> {
        self.data.skipped.clone()
    }

    pub fn mark_skipped(&mut self, match_id: &str) {
        self.data.skipped.insert(match_id.to_string());
    }

    pub fn count(&self) -> i32 {
        self.data.matches.len() as i32
    }

    pub fn scanned(&self) -> i32 { self.data.scanned }
    pub fn set_scanned(&mut self, n: i32) { self.data.scanned = n; }

    pub fn delete_oldest(&mut self, n: usize) -> i32 {
        // Sort by start_ms and remove the oldest n
        let mut entries: Vec<(u64, String)> = self.data.matches.iter()
            .map(|(id, v)| {
                let ms = v.get("start_ms").and_then(|v| v.as_u64()).unwrap_or(0);
                (ms, id.clone())
            })
            .collect();
        entries.sort_by_key(|(ms, _)| *ms);
        let to_delete: Vec<String> = entries.into_iter().take(n).map(|(_, id)| id).collect();
        let removed = to_delete.len() as i32;
        for id in to_delete { self.data.matches.remove(&id); }
        removed
    }

    pub fn oldest_ms(&self) -> u64 {
        self.data.matches.values()
            .filter_map(|v| v.get("start_ms").and_then(|v| v.as_u64()))
            .min()
            .unwrap_or(0)
    }

    pub fn newest_ms(&self) -> u64 {
        self.data.matches.values()
            .filter_map(|v| v.get("start_ms").and_then(|v| v.as_u64()))
            .max()
            .unwrap_or(0)
    }

    /// Aggregate stats over all stored games (season="" = lifetime).
    pub fn aggregate(&self, season: &str) -> AggResult {
        let rows: Vec<&Value> = self.data.matches.values()
            .filter(|v| {
                if season.is_empty() { return true; }
                v.get("season").and_then(|s| s.as_str()).unwrap_or("") == season
            })
            .collect();

        let n = rows.len();
        if n == 0 {
            return AggResult { agg: None, games: 0 };
        }

        let agg_vals: Vec<Option<Value>> = rows.iter().map(|r| Some((*r).clone())).collect();
        let refs: Vec<Option<Value>> = agg_vals;
        let ref_refs: Vec<Option<serde_json::Value>> = refs;

        // Use the same aggregation logic as parse.rs
        use crate::riot::parse::aggregate_stats;
        let agg = aggregate_stats(&ref_refs);

        AggResult { agg, games: n as i32 }
    }
}

pub struct AggResult {
    pub agg: Option<PlayerStats>,
    pub games: i32,
}

pub fn profile_store_dir() -> PathBuf {
    let base = std::env::var("APPDATA").map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    base.join("ValoRecon").join("profile")
}
