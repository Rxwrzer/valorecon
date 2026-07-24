use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RankInfo {
    pub tier: i32,
    pub tier_name: String,
    pub rr: i32,
    pub tier_color: String,
    pub tier_icon: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlayerStats {
    pub kda: Option<f64>,
    pub acs: Option<i32>,
    pub adr: Option<i32>,
    pub hs: Option<i32>,
    pub winrate: Option<i32>,
    pub games: i32,
    pub avg_k: f64,
    pub avg_d: f64,
    pub avg_a: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchPlayer {
    pub puuid: String,
    pub team: String,
    pub name: String,
    pub incognito: bool,
    pub agent_id: String,
    pub agent_name: String,
    pub agent_icon: String,
    pub agent_color: String,
    pub account_level: i32,
    pub hide_level: bool,
    pub current: RankInfo,
    pub peak: RankInfo,
    pub wins: i32,
    pub is_self: bool,
    pub pending: bool,
    pub stats: Option<PlayerStats>,
    pub stats_pending: bool,
    pub party_group: i32,
    pub party_confirmed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppState {
    pub connected: bool,
    pub phase: String,   // "offline" | "menus" | "pregame" | "ingame"
    pub map: String,
    pub map_image: String,
    pub players: Vec<MatchPlayer>,
    pub self_name: String,
    pub error: String,
    pub updated: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RRHistoryPoint {
    pub match_id: String,
    pub map_name: String,
    pub tier: i32,
    pub tier_name: String,
    pub rr_after: i32,
    pub rr_change: i32,
    pub elo: i32,
    pub date_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProfileDeep {
    pub games_total: i32,
    pub games_act: i32,
    pub lifetime: Option<PlayerStats>,
    pub act: Option<PlayerStats>,
    pub oldest_ms: u64,
    pub newest_ms: u64,
    pub scanned: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub puuid: String,
    pub name: String,
    pub current: RankInfo,
    pub peak: RankInfo,
    pub wins: i32,
    pub history: Vec<RRHistoryPoint>,
    pub deep: Option<ProfileDeep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullStatus {
    pub running: bool,
    pub added: i32,
    pub target: i32,
    pub want_max: bool,
    pub bucket_full_in: i32,
    pub done: bool,
    pub error: String,
    pub total_games: i32,
    pub history_seen: i32,
    pub history_end: bool,
}

impl Default for PullStatus {
    fn default() -> Self {
        Self {
            running: false, added: 0, target: 0, want_max: false,
            bucket_full_in: 0, done: false, error: String::new(),
            total_games: 0, history_seen: 0, history_end: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub region_override: String,
    pub poll_seconds: u64,
    pub has_key: bool,
    pub key_hint: String,
    pub profile_pull_target: i32,
    pub pull_source: String,
    pub always_on_top: bool,
    pub henrik_rate_limit: u32,
    pub live_use_henrik: bool,
    pub version: String,
}
