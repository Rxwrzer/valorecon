/// Valorant-api.com content service: tiers, agents, maps, seasons.
/// Fetches once and caches to disk. Mirrors backend/content.py.
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

const VALORANT_API: &str = "https://valorant-api.com/v1";

#[derive(Debug, Clone, Default)]
pub struct TierInfo {
    pub name: String,
    pub color: String,
    pub icon: String,
}

#[derive(Debug, Clone, Default)]
pub struct AgentInfo {
    pub name: String,
    pub icon: String,
    pub color: String,
}

#[derive(Debug, Clone, Default)]
pub struct MapInfo {
    pub name: String,
    pub icon: String,
}

#[derive(Debug, Default)]
pub struct Content {
    pub tiers: HashMap<i32, TierInfo>,
    pub agents: HashMap<String, AgentInfo>,  // uuid lowercase -> info
    pub maps: HashMap<String, MapInfo>,      // path lowercase -> info
    pub current_season: String,
    pub client_version: String,
}

impl Content {
    pub fn tier(&self, tier: i32) -> TierInfo {
        self.tiers.get(&tier).cloned().unwrap_or_else(|| TierInfo {
            name: "Unranked".into(), color: "#8b90a0".into(), icon: String::new(),
        })
    }

    pub fn agent(&self, id: &str) -> AgentInfo {
        self.agents.get(&id.to_lowercase()).cloned().unwrap_or_default()
    }

    pub fn map(&self, path: &str) -> MapInfo {
        // path may be full e.g. "/Game/Maps/Ascent/Ascent" — match by last segment
        let key = path.to_lowercase();
        if let Some(v) = self.maps.get(&key) { return v.clone(); }
        let short = path.split('/').last().unwrap_or("").to_lowercase();
        self.maps.values().find(|m| m.name.to_lowercase() == short).cloned().unwrap_or_default()
    }
}

pub type SharedContent = Arc<RwLock<Option<Content>>>;

pub fn cache_path() -> PathBuf {
    let base = std::env::var("APPDATA").map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    base.join("ValoRecon").join("content_cache.json")
}

pub async fn load_content(http: &reqwest::Client) -> anyhow::Result<Content> {
    // Try disk cache first
    let cached = load_from_disk();

    // Fetch in parallel
    let (tiers_r, agents_r, maps_r, version_r, seasons_r) = tokio::join!(
        http.get(&format!("{VALORANT_API}/competitivetiers")).send(),
        http.get(&format!("{VALORANT_API}/agents?isPlayableCharacter=true")).send(),
        http.get(&format!("{VALORANT_API}/maps")).send(),
        http.get(&format!("{VALORANT_API}/version")).send(),
        http.get(&format!("{VALORANT_API}/seasons")).send(),
    );

    let tiers_json: Value = tiers_r?.json().await?;
    let agents_json: Value = agents_r?.json().await?;
    let maps_json: Value = maps_r?.json().await?;
    let version_json: Value = version_r?.json().await?;
    let seasons_json: Value = seasons_r?.json().await?;

    let client_version = version_json["data"]["riotClientVersion"]
        .as_str().unwrap_or("").to_string();

    // Parse competitive tiers (latest episode)
    let mut tiers = HashMap::new();
    if let Some(episodes) = tiers_json["data"].as_array() {
        if let Some(latest) = episodes.last() {
            if let Some(tier_arr) = latest["tiers"].as_array() {
                for t in tier_arr {
                    let id = t["tier"].as_i64().unwrap_or(0) as i32;
                    let name = t["tierName"].as_str().unwrap_or("Unranked").to_string();
                    let color_raw = t["color"].as_str().unwrap_or("ff8b90a0").to_string();
                    // Format: AARRGGBB -> #RRGGBB
                    let color = if color_raw.len() == 8 {
                        format!("#{}", &color_raw[2..])
                    } else {
                        "#8b90a0".into()
                    };
                    let icon = t["largeIcon"].as_str()
                        .or_else(|| t["smallIcon"].as_str())
                        .unwrap_or("").to_string();
                    tiers.insert(id, TierInfo { name, color, icon });
                }
            }
        }
    }

    // Parse agents
    let mut agents = HashMap::new();
    if let Some(arr) = agents_json["data"].as_array() {
        for a in arr {
            let id = a["uuid"].as_str().unwrap_or("").to_lowercase();
            let name = a["displayName"].as_str().unwrap_or("").to_string();
            let icon = a["displayIcon"].as_str().unwrap_or("").to_string();
            // Agent background color from role accent
            let color = a["role"]["displayIcon"].as_str()
                .map(|_| "#4b5160".to_string())
                .unwrap_or_else(|| "#4b5160".into());
            agents.insert(id, AgentInfo { name, icon, color });
        }
    }

    // Parse maps
    let mut maps = HashMap::new();
    if let Some(arr) = maps_json["data"].as_array() {
        for m in arr {
            let path = m["mapUrl"].as_str().unwrap_or("").to_lowercase();
            let name = m["displayName"].as_str().unwrap_or("").to_string();
            let icon = m["splash"].as_str()
                .or_else(|| m["displayIcon"].as_str())
                .unwrap_or("").to_string();
            maps.insert(path, MapInfo { name, icon });
        }
    }

    // Determine the active competitive Act UUID (matches SeasonID in
    // competitiveupdates). Acts have a parentUuid (episode) and a time window;
    // pick the one whose [startTime, endTime) contains now.
    let current_season = active_act_uuid(&seasons_json);

    let content = Content { tiers, agents, maps, current_season, client_version };

    // Cache to disk
    let _ = save_to_disk(&content);
    let _ = cached; // unused if fetch succeeded

    Ok(content)
}

/// Find the currently-active Act's UUID from valorant-api /seasons.
/// Acts carry a `parentUuid` (their episode) and an ISO8601 start/end window.
fn active_act_uuid(seasons_json: &Value) -> String {
    let arr = match seasons_json["data"].as_array() {
        Some(a) => a,
        None => return String::new(),
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let mut fallback = String::new();
    let mut fallback_end = i64::MIN;
    for s in arr {
        // Acts have a non-null parentUuid; episodes don't.
        if s["parentUuid"].is_null() { continue; }
        let uuid = s["uuid"].as_str().unwrap_or("");
        if uuid.is_empty() { continue; }
        let start = s["startTime"].as_str().and_then(iso8601_to_epoch);
        let end = s["endTime"].as_str().and_then(iso8601_to_epoch);
        if let (Some(st), Some(en)) = (start, end) {
            if now >= st && now < en {
                return uuid.to_string();
            }
            // Track the latest-ending act as a fallback (in case clocks/windows drift).
            if en > fallback_end {
                fallback_end = en;
                fallback = uuid.to_string();
            }
        }
    }
    fallback
}

/// Minimal parser for `YYYY-MM-DDTHH:MM:SS(.fff)?(Z|+HH:MM)?` → Unix epoch seconds (UTC).
/// valorant-api emits UTC "Z" timestamps; timezone offsets are ignored (treated as UTC).
fn iso8601_to_epoch(s: &str) -> Option<i64> {
    let bytes = s.as_bytes();
    if bytes.len() < 19 { return None; }
    let num = |a: usize, b: usize| -> Option<i64> { s.get(a..b)?.parse::<i64>().ok() };
    let year = num(0, 4)?;
    let month = num(5, 7)?;
    let day = num(8, 10)?;
    let hour = num(11, 13)?;
    let min = num(14, 16)?;
    let sec = num(17, 19)?;
    // Days-from-civil (Howard Hinnant's algorithm), valid for the proleptic Gregorian calendar.
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    Some(days * 86400 + hour * 3600 + min * 60 + sec)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn iso8601_known_epochs() {
        assert_eq!(iso8601_to_epoch("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(iso8601_to_epoch("2000-01-01T00:00:00Z"), Some(946684800));
        // With fractional seconds + offset suffix (ignored, treated as UTC).
        assert_eq!(iso8601_to_epoch("2000-01-01T00:00:00.000Z"), Some(946684800));
        assert_eq!(iso8601_to_epoch("bad"), None);
    }

    #[test]
    fn active_act_picks_current_window() {
        // Two acts; "now" (SystemTime) falls in the far-future one.
        let seasons = json!({ "data": [
            { "uuid": "episode", "parentUuid": Value::Null,
              "startTime": "2000-01-01T00:00:00Z", "endTime": "2999-01-01T00:00:00Z" },
            { "uuid": "old-act", "parentUuid": "episode",
              "startTime": "2000-01-01T00:00:00Z", "endTime": "2001-01-01T00:00:00Z" },
            { "uuid": "current-act", "parentUuid": "episode",
              "startTime": "2020-01-01T00:00:00Z", "endTime": "2999-01-01T00:00:00Z" },
        ]});
        assert_eq!(active_act_uuid(&seasons), "current-act");
    }

    #[test]
    fn active_act_ignores_episodes() {
        // Episode window contains now but has no parentUuid, so it must be skipped.
        let seasons = json!({ "data": [
            { "uuid": "episode", "parentUuid": Value::Null,
              "startTime": "2020-01-01T00:00:00Z", "endTime": "2999-01-01T00:00:00Z" },
        ]});
        assert_eq!(active_act_uuid(&seasons), "");
    }
}

fn load_from_disk() -> Option<Content> {
    // For Phase A this is a no-op placeholder; full implementation in Phase C
    None
}

fn save_to_disk(_content: &Content) {
    // placeholder; full implementation in Phase C
}
