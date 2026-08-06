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

    /// Full match scoreboards (all players) for recent competitive games —
    /// used to compute match/team MVP. One call covers several matches.
    pub async fn matches_full(
        &self, region: &str, name: &str, tag: &str, size: u32,
    ) -> Result<Value, HenrikError> {
        let url = format!("{BASE}/v4/matches/{region}/pc/{name}/{tag}?mode=competitive&size={size}");
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
    m.insert("puuid".into(), d["puuid"].clone());
    // Player card art for the profile avatar. v1 account returns
    // card: { small, large, wide, id } (full URLs); some shapes give a bare id.
    let card = d.get("card");
    let card_url = card
        .and_then(|c| c.get("large").or_else(|| c.get("small")).or_else(|| c.get("wide")))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| card.and_then(|v| v.as_str())
            .map(|id| format!("https://media.valorant-api.com/playercards/{id}/largeart.png")));
    m.insert("card".into(), card_url.map(Value::from).unwrap_or(Value::Null));
    Some(m)
}

pub fn parse_mmr(data: &Value) -> serde_json::Map<String, Value> {
    let d = &data["data"];
    // v2 nests current standing under `current_data`; fall back to flat (v1).
    let cur = if d.get("current_data").is_some() { &d["current_data"] } else { d };
    let mut m = serde_json::Map::new();
    m.insert("current_tier_name".into(), cur["currenttierpatched"].clone());
    m.insert("current_rr".into(), cur["ranking_in_tier"].clone());
    // Numeric tier ids let the caller resolve tier color/icon from content.
    m.insert("current_tier".into(), cur["currenttier"].clone());
    m.insert("peak_tier".into(), d["highest_rank"]["tier"].clone());
    m.insert("peak_tier_name".into(),
        d["highest_rank"]["patched_tier"].clone());
    m.insert("peak_season".into(),
        d["highest_rank"]["season"].clone());
    m
}

/// From the v4 full-match response, work out for each match whether the queried
/// player was match MVP (top combat score overall) or team MVP (top on their
/// team). Because every player shares the same round count, comparing raw combat
/// score is equivalent to comparing ACS. Returns match_id -> "match" | "team".
pub fn parse_mvp_map(data: &Value, puuid: &str, name: &str, tag: &str)
    -> std::collections::HashMap<String, String> {
    use std::collections::HashMap;
    let mut out: HashMap<String, String> = HashMap::new();
    let arr = match data.get("data").and_then(|d| d.as_array()) { Some(a) => a, None => return out };
    let score = |p: &Value| p.get("stats").and_then(|s| s.get("score")).and_then(|v| v.as_f64()).unwrap_or(0.0);
    let team = |p: &Value| p.get("team_id").and_then(|v| v.as_str())
        .or_else(|| p.get("team").and_then(|v| v.as_str())).unwrap_or("").to_string();
    for m in arr {
        let mid = m.get("metadata").and_then(|md| md.get("match_id")).and_then(|v| v.as_str()).unwrap_or("");
        if mid.is_empty() { continue; }
        // v4: flat `players`; older shapes nest under players.all_players.
        let players = match m.get("players").and_then(|p| p.as_array())
            .or_else(|| m.get("players").and_then(|p| p.get("all_players")).and_then(|a| a.as_array())) {
            Some(p) => p, None => continue,
        };
        let is_me = |p: &Value| {
            if !puuid.is_empty() && p.get("puuid").and_then(|v| v.as_str()) == Some(puuid) { return true; }
            let pn = p.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let pt = p.get("tag").and_then(|v| v.as_str()).unwrap_or("");
            pn.eq_ignore_ascii_case(name) && pt.eq_ignore_ascii_case(tag)
        };
        let me = match players.iter().find(|p| is_me(p)) { Some(p) => p, None => continue };
        let my_score = score(me);
        let my_team = team(me);
        let mut match_best = f64::MIN;
        let mut team_best = f64::MIN;
        for p in players {
            let s = score(p);
            if s > match_best { match_best = s; }
            if team(p) == my_team && s > team_best { team_best = s; }
        }
        let label = if my_score >= match_best { "match" }
            else if my_score >= team_best { "team" } else { "" };
        if !label.is_empty() { out.insert(mid.to_string(), label.to_string()); }
    }
    out
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
        // A team's round count. The lifetime/stored endpoint returns teams as
        // integers: {"red": 13, "blue": 5}. Match-detail nests {rounds_won: 13}.
        // Handle both.
        fn team_rounds(teams: &Value, name: &str) -> Option<i64> {
            let v = ci_get(teams, name)?;
            v.as_i64().or_else(|| v.get("rounds_won").and_then(|x| x.as_i64()))
        }

        let other_raw = if team_raw == "red" { "blue" } else { "red" };
        let my_rounds = g.get("teams").and_then(|t| team_rounds(t, &team_raw));
        let enemy_rounds = g.get("teams").and_then(|t| team_rounds(t, other_raw));

        let score = stats.get("score").and_then(|v| v.as_i64()).unwrap_or(0);
        let rounds = match (my_rounds, enemy_rounds) {
            (Some(a), Some(b)) => a + b,
            _ => g.get("metadata").and_then(|m| m.get("rounds_played"))
                .and_then(|v| v.as_i64()).unwrap_or(0),
        };
        let won = me.get("won").and_then(|v| v.as_bool())
            .or_else(|| {
                // Explicit has_won flag on my team object, if the endpoint provides it.
                if let Some(b) = g.get("teams").and_then(|t| ci_get(t, &team_raw))
                    .and_then(|mt| mt.get("has_won").and_then(|v| v.as_bool())) {
                    return Some(b);
                }
                match (my_rounds, enemy_rounds) {
                    (Some(a), Some(b)) => Some(a > b),
                    _ => None,
                }
            });

        let match_id = g.get("meta").and_then(|m| m.get("id")).and_then(|v| v.as_str())
            .unwrap_or("").to_string();
        let map = g.get("meta").and_then(|m| m.get("map"))
            .and_then(|m| m.get("name")).and_then(|v| v.as_str())
            .unwrap_or("").to_string();
        let agent = stats.get("character").and_then(|c| c.get("name")).and_then(|v| v.as_str())
            .or_else(|| me.get("character").and_then(|c| c.get("name")).and_then(|v| v.as_str()))
            .or_else(|| stats.get("agent").and_then(|a| a.get("name")).and_then(|v| v.as_str()))
            .unwrap_or("").to_string();
        // Agent portrait from the character UUID via the valorant-api media CDN.
        let agent_id = stats.get("character").and_then(|c| c.get("id")).and_then(|v| v.as_str())
            .or_else(|| me.get("character").and_then(|c| c.get("id")).and_then(|v| v.as_str()))
            .or_else(|| stats.get("agent").and_then(|a| a.get("id")).and_then(|v| v.as_str()))
            .unwrap_or("");
        let agent_icon = if agent_id.is_empty() { String::new() }
            else { format!("https://media.valorant-api.com/agents/{agent_id}/displayicon.png") };
        let mut row = serde_json::Map::new();
        row.insert("match_id".into(), match_id.into());
        row.insert("agent".into(), agent.into());
        row.insert("agent_icon".into(), agent_icon.into());
        row.insert("map".into(), map.into());
        row.insert("kills".into(), kills.into());
        row.insert("deaths".into(), deaths.into());
        row.insert("assists".into(), assists.into());
        row.insert("kd".into(), kd.into());
        row.insert("hs".into(), hs_pct.into());
        let damage = stats.get("damage").and_then(|d| d.get("made")).and_then(|v| v.as_f64())
            .or_else(|| stats.get("damage").and_then(|v| v.as_f64()))
            .unwrap_or(0.0);
        row.insert("score".into(), score.into());
        row.insert("rounds".into(), rounds.into());
        row.insert("my_rounds".into(), my_rounds.unwrap_or(0).into());
        row.insert("enemy_rounds".into(), enemy_rounds.unwrap_or(0).into());
        row.insert("damage".into(), damage.into());
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_matches_integer_teams() {
        // Lifetime/stored endpoint shape: teams are integer round counts.
        let data = json!({
            "data": [{
                "meta": { "id": "m1", "map": { "name": "Split" } },
                "stats": {
                    "team": "Red",
                    "score": 6000,
                    "kills": 20, "deaths": 15, "assists": 5,
                    "character": { "name": "Jett" },
                    "shots": { "head": 20, "body": 60, "leg": 20 }
                },
                "teams": { "red": 13, "blue": 11 }
            }]
        });
        let rows = parse_matches(&data, "Someone", "0000");
        assert_eq!(rows.len(), 1);
        let r = &rows[0];
        assert_eq!(r["rounds"].as_i64(), Some(24));
        assert_eq!(r["my_rounds"].as_i64(), Some(13));
        assert_eq!(r["enemy_rounds"].as_i64(), Some(11));
        assert_eq!(r["won"].as_bool(), Some(true));
        assert_eq!(r["agent"].as_str(), Some("Jett"));
    }

    #[test]
    fn parse_mvp_map_match_and_team() {
        let data = json!({ "data": [{
            "metadata": { "match_id": "M1" },
            "players": [
                { "puuid": "me",  "team_id": "Red",  "stats": { "score": 5000 } },
                { "puuid": "x",   "team_id": "Red",  "stats": { "score": 4000 } },
                { "puuid": "y",   "team_id": "Blue", "stats": { "score": 6000 } },
            ]
        }, {
            "metadata": { "match_id": "M2" },
            "players": [
                { "puuid": "me",  "team_id": "Blue", "stats": { "score": 7000 } },
                { "puuid": "z",   "team_id": "Blue", "stats": { "score": 3000 } },
                { "puuid": "w",   "team_id": "Red",  "stats": { "score": 6500 } },
            ]
        }]});
        let m = parse_mvp_map(&data, "me", "", "");
        assert_eq!(m.get("M1").map(|s| s.as_str()), Some("team"));  // top on Red, not overall
        assert_eq!(m.get("M2").map(|s| s.as_str()), Some("match")); // top overall
    }

    #[test]
    fn parse_matches_nested_teams() {
        // Match-detail shape: teams nest rounds_won.
        let data = json!({
            "data": [{
                "meta": { "id": "m2", "map": { "name": "Ascent" } },
                "stats": {
                    "team": "Blue",
                    "score": 3000,
                    "kills": 10, "deaths": 18, "assists": 4,
                    "character": { "name": "Sova" },
                    "shots": { "head": 5, "body": 40, "leg": 5 }
                },
                "teams": {
                    "red": { "rounds_won": 13 },
                    "blue": { "rounds_won": 8 }
                }
            }]
        });
        let rows = parse_matches(&data, "Someone", "0000");
        let r = &rows[0];
        assert_eq!(r["rounds"].as_i64(), Some(21));
        assert_eq!(r["my_rounds"].as_i64(), Some(8));
        assert_eq!(r["won"].as_bool(), Some(false));
    }
}
