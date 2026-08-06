/// HenrikDev API client — player lookup + stored match history.
/// Mirrors backend/henrik_api.py.
use reqwest::Client;
use serde_json::Value;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use thiserror::Error;

use crate::riot::client::SharedLimiter;
use crate::riot::ratelimit::RateLimiter;

const BASE: &str = "https://api.henrikdev.xyz/valorant";

pub fn new_henrik_limiter(per_minute: u32) -> SharedLimiter {
    Arc::new(Mutex::new(RateLimiter::new(
        per_minute as usize,
        Duration::from_secs(60),
    )))
}

#[derive(Debug, Error)]
pub enum HenrikError {
    #[error("HTTP {0}: {1}")]
    Status(u16, String),
    #[error("Rate limited")]
    RateLimited,
    #[error("Not found")]
    NotFound,
    #[error("Request error: {0}")]
    Http(#[from] reqwest::Error),
}

pub struct HenrikClient {
    http: Client,
    key: String,
    limiter: SharedLimiter,
}

impl HenrikClient {
    pub fn new(http: Client, key: String, limiter: SharedLimiter) -> Self {
        Self { http, key, limiter }
    }

    async fn gate(&self) {
        loop {
            let wait = {
                let mut l = self.limiter.lock().unwrap();
                if l.available() > 0 { l.record(); 0.0 } else { l.time_until(1).max(0.05) }
            };
            if wait <= 0.0 { break; }
            tokio::time::sleep(Duration::from_secs_f64(wait.min(30.0))).await;
        }
    }

    async fn get(&self, url: &str) -> Result<Value, HenrikError> {
        self.gate().await;
        let resp = self.http.get(url)
            .header("Authorization", &self.key)
            .send()
            .await?;
        let status = resp.status().as_u16();
        match status {
            200 => {
                let v: Value = resp.json().await?;
                Ok(v)
            }
            429 => Err(HenrikError::RateLimited),
            404 => Err(HenrikError::NotFound),
            _ => {
                let msg = resp.text().await.unwrap_or_default();
                Err(HenrikError::Status(status, msg))
            }
        }
    }

    pub async fn account(&self, name: &str, tag: &str) -> Result<Value, HenrikError> {
        let url = format!("{BASE}/v1/account/{name}/{tag}");
        self.get(&url).await
    }

    pub async fn mmr(&self, region: &str, name: &str, tag: &str) -> Result<Value, HenrikError> {
        let url = format!("{BASE}/v2/mmr/{region}/{name}/{tag}");
        self.get(&url).await
    }

    pub async fn matches(&self, region: &str, name: &str, tag: &str) -> Result<Value, HenrikError> {
        let url = format!("{BASE}/v1/lifetime/matches/{region}/{name}/{tag}?mode=competitive&size=10");
        self.get(&url).await
    }

    pub async fn stored_matches_by_puuid(
        &self, region: &str, puuid: &str, page: u32, size: u32,
    ) -> Result<Value, HenrikError> {
        let url = format!(
            "{BASE}/v1/by-puuid/stored-matches/{region}/{puuid}?size={size}&page={page}"
        );
        self.get(&url).await
    }
}

// ── Parsers ────────────────────────────────────────────────────────────────

pub fn parse_account(data: &Value) -> Option<serde_json::Map<String, Value>> {
    let d = data.get("data")?;
    let mut m = serde_json::Map::new();
    m.insert("name".into(), d["name"].clone());
    m.insert("tag".into(), d["tag"].clone());
    m.insert("region".into(), d["region"].clone());
    m.insert("level".into(), d["account_level"].clone());
    Some(m)
}

pub fn parse_mmr(data: &Value) -> serde_json::Map<String, Value> {
    let d = &data["data"];
    // v2 nests current standing under `current_data`; fall back to flat (v1).
    let cur = if d.get("current_data").is_some() { &d["current_data"] } else { d };
    let mut m = serde_json::Map::new();
    m.insert("current_tier_name".into(), cur["currenttierpatched"].clone());
    m.insert("current_rr".into(), cur["ranking_in_tier"].clone());
    m.insert("peak_tier_name".into(),
        d["highest_rank"]["patched_tier"].clone());
    m.insert("peak_season".into(),
        d["highest_rank"]["season"].clone());
    m
}

pub fn parse_matches(data: &Value, name: &str, tag: &str) -> Vec<serde_json::Map<String, Value>> {
    let arr = match data.get("data").and_then(|d| d.as_array()) {
        Some(a) => a,
        None => return vec![],
    };
    arr.iter().filter_map(|g| {
        // The lifetime endpoint puts the queried player's stats directly in
        // `stats`. Fall back to scanning a `players` array (match-detail shape).
        let me: &Value = if g.get("stats").is_some() {
            g
        } else {
            g.get("players")?.as_array()?.iter().find(|p| {
                p.get("name").and_then(|v| v.as_str()).unwrap_or("").to_lowercase() == name.to_lowercase()
                && p.get("tag").and_then(|v| v.as_str()).unwrap_or("").to_lowercase() == tag.to_lowercase()
            })?
        };
        let stats = me.get("stats")?;
        let kills = stats["kills"].as_i64().unwrap_or(0);
        let deaths = stats["deaths"].as_i64().unwrap_or(0);
        let assists = stats["assists"].as_i64().unwrap_or(0);
        let kd = if deaths > 0 { format!("{:.2}", kills as f64 / deaths as f64) } else { kills.to_string() };

        // Headshot % from shot counts (may live under stats.shots or stats directly).
        let shots = stats.get("shots").unwrap_or(stats);
        let hs = shots.get("head").and_then(|v| v.as_i64()).unwrap_or(0);
        let bs = shots.get("body").and_then(|v| v.as_i64()).unwrap_or(0);
        let ls = shots.get("leg").and_then(|v| v.as_i64()).unwrap_or(0);
        let total = hs + bs + ls;
        let hs_pct = if total > 0 { ((hs as f64 / total as f64) * 100.0).round() as i64 } else { 0 };

        // API uses "Red"/"Blue" or "red"/"blue" — look up case-insensitively.
        let team_raw = me.get("stats").and_then(|s| s.get("team"))
            .or_else(|| me.get("team"))
            .and_then(|v| v.as_str()).unwrap_or("").to_lowercase();

        // Returns a cloned value for a key matched case-insensitively.
        fn ci_get(obj: &Value, name: &str) -> Option<Value> {
            let lo = name.to_lowercase();
            obj.as_object()?.iter()
                .find(|(k, _)| k.to_lowercase() == lo)
                .map(|(_, v)| v.clone())
        }

        let won = me.get("won").and_then(|v| v.as_bool())
            .or_else(|| {
                let teams = g.get("teams")?;
                let my_t = ci_get(teams, &team_raw)?;
                if let Some(b) = my_t.get("has_won").and_then(|v| v.as_bool()) { return Some(b); }
                let other = if team_raw == "red" { "blue" } else { "red" };
                let my_rw = my_t.get("rounds_won").and_then(|v| v.as_i64())?;
                let other_rw = ci_get(teams, other).and_then(|t| t.get("rounds_won").and_then(|v| v.as_i64()))?;
                Some(my_rw > other_rw)
            });

        let score = stats.get("score").and_then(|v| v.as_i64()).unwrap_or(0);
        let rounds = g.get("teams").and_then(|t| {
            let r = ci_get(t, "red").and_then(|x| x.get("rounds_won").and_then(|v| v.as_i64()));
            let b = ci_get(t, "blue").and_then(|x| x.get("rounds_won").and_then(|v| v.as_i64()));
            if let (Some(r), Some(b)) = (r, b) { return Some(r + b); }
            let my_team = ci_get(t, &team_raw)?;
            let rw = my_team.get("rounds_won").and_then(|v| v.as_i64())?;
            let rl = my_team.get("rounds_lost").and_then(|v| v.as_i64())?;
            Some(rw + rl)
        }).or_else(|| g.get("metadata").and_then(|m| m.get("rounds_played")).and_then(|v| v.as_i64()))
          .unwrap_or(0);

        let map = g.get("meta").and_then(|m| m.get("map"))
            .and_then(|m| m.get("name")).and_then(|v| v.as_str())
            .unwrap_or("").to_string();
        let agent = stats.get("character").and_then(|c| c.get("name")).and_then(|v| v.as_str())
            .or_else(|| me.get("character").and_then(|c| c.get("name")).and_then(|v| v.as_str()))
            .or_else(|| stats.get("agent").and_then(|a| a.get("name")).and_then(|v| v.as_str()))
            .unwrap_or("").to_string();
        let mut row = serde_json::Map::new();
        row.insert("agent".into(), agent.into());
        row.insert("map".into(), map.into());
        row.insert("kills".into(), kills.into());
        row.insert("deaths".into(), deaths.into());
        row.insert("assists".into(), assists.into());
        row.insert("kd".into(), kd.into());
        row.insert("hs".into(), hs_pct.into());
        row.insert("score".into(), score.into());
        row.insert("rounds".into(), rounds.into());
        if let Some(w) = won { row.insert("won".into(), w.into()); }
        Some(row)
    }).collect()
}

/// Parse HenrikDev v1 by-puuid stored-matches. Each entry is shaped
/// `{ meta, stats, teams }` (same family as the lifetime endpoint), where
/// `stats` is already the queried player's own line. `results.after` drives
/// pagination. Returns rows keyed for the profile store / aggregate_stats.
pub fn parse_stored_matches(data: &Value, _puuid: &str) -> (Vec<serde_json::Map<String, Value>>, i64) {
    let after = data.get("results").and_then(|r| r.get("after")).and_then(|v| v.as_i64()).unwrap_or(0);
    let matches = match data.get("data").and_then(|d| d.as_array()) {
        Some(a) => a,
        None => return (vec![], after),
    };
    let games: Vec<serde_json::Map<String, Value>> = matches.iter().filter_map(|g| {
        let meta = g.get("meta")?;
        let match_id = meta.get("id")?.as_str()?.to_string();
        let stats = g.get("stats")?;

        let shots = stats.get("shots");
        let hs = shots.and_then(|s| s.get("head")).and_then(|v| v.as_i64()).unwrap_or(0);
        let bs = shots.and_then(|s| s.get("body")).and_then(|v| v.as_i64()).unwrap_or(0);
        let ls = shots.and_then(|s| s.get("leg")).and_then(|v| v.as_i64()).unwrap_or(0);
        let damage = stats.get("damage").and_then(|d| d.get("made")).and_then(|v| v.as_f64()).unwrap_or(0.0);

        // Round counts per team → total rounds + win/loss for the queried player.
        let red = g.get("teams").and_then(|t| t.get("red")).and_then(|t| t.get("rounds_won")).and_then(|v| v.as_i64()).unwrap_or(0);
        let blue = g.get("teams").and_then(|t| t.get("blue")).and_then(|t| t.get("rounds_won")).and_then(|v| v.as_i64()).unwrap_or(0);
        let team = stats.get("team").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
        let won = match team.as_str() { "red" => red > blue, "blue" => blue > red, _ => false };

        // Real per-game season UUID — matches content's current_season for act filtering.
        let season = meta.get("season").and_then(|s| s.get("id")).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let agent = stats.get("character").and_then(|c| c.get("name")).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let started_at = meta.get("started_at").and_then(|v| v.as_str()).unwrap_or("").to_string();

        let mut row = serde_json::Map::new();
        row.insert("match_id".into(), match_id.into());
        row.insert("kills".into(), stats.get("kills").cloned().unwrap_or_else(|| 0.into()));
        row.insert("deaths".into(), stats.get("deaths").cloned().unwrap_or_else(|| 0.into()));
        row.insert("assists".into(), stats.get("assists").cloned().unwrap_or_else(|| 0.into()));
        row.insert("score".into(), stats.get("score").cloned().unwrap_or_else(|| 0.into()));
        row.insert("damage".into(), damage.into());
        row.insert("rounds".into(), (red + blue).into());
        row.insert("headshots".into(), hs.into());
        row.insert("bodyshots".into(), bs.into());
        row.insert("legshots".into(), ls.into());
        row.insert("won".into(), won.into());
        row.insert("agent".into(), agent.into());
        row.insert("season".into(), season.into());
        row.insert("started_at".into(), started_at.into());
        row.insert("party".into(), "".into());
        Some(row)
    }).collect();
    (games, after)
}
