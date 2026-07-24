/// Orchestration: poll loop, live-match assembly, deep-pull, auto-archive.
/// The tracker runs as a background tokio task and emits Tauri events.
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;
use tokio::time::sleep;

use crate::models::{AppState, MatchPlayer, PlayerStats, PullStatus, RankInfo};
use crate::riot::lockfile::{read_lockfile, LockfileError};
use crate::riot::local::{get_credentials, Credentials};
use crate::riot::client::{RiotClient, SharedLimiter, new_limiter};
use crate::riot::content::{load_content, Content};
use crate::henrik::{HenrikClient, parse_stored_matches, new_henrik_limiter};
use crate::store::profile::{ProfileStore, profile_store_dir};
use tauri::Emitter;
use crate::riot::parse::{
    parse_rank_from_updates, parse_competitive_updates, parse_name_service,
    parse_pregame_match, parse_coregame_match, extract_all_match_stats, aggregate_stats,
};
use crate::settings::Settings;

// ── Cache TTLs (mirrors tracker.py constants) ──────────────────────────────
const MMR_TTL: Duration = Duration::from_secs(240);
const STATS_TTL: Duration = Duration::from_secs(600);
// Max recent games to fetch match-details for per player.
// At 45 req/60s with 9 parallel players: 9×10 = 90 fetches ≈ 2 minutes.
const STATS_MAX: usize = 10;

/// Per-player rank resolution: current + peak + recent match ids for stats.
#[derive(Clone, Default)]
struct PlayerResolve {
    current: RankInfo,
    peak: RankInfo,
    recent_ids: Vec<String>,
    pending: bool,
}

pub struct Tracker {
    pub settings: Settings,
    pub state: AppState,
    pub pull_status: PullStatus,

    // Riot connection (set after first successful connect)
    pub creds: Option<Credentials>,
    pub content: Option<Content>,

    // Shared across every RiotClient so all API calls draw from one budget.
    pub limiter: SharedLimiter,
    // Shared across every HenrikClient, ceiling set by settings.henrik_rate_limit.
    pub henrik_limiter: SharedLimiter,
    // True while a background stats pass is running (prevents overlapping passes).
    stats_busy: Arc<AtomicBool>,

    // In-memory caches
    mmr_cache: HashMap<String, (std::time::Instant, PlayerResolve)>,
    stats_cache: HashMap<String, (std::time::Instant, Option<PlayerStats>)>,
    // match_id -> per-puuid statline map (avoids refetching a match shared by
    // several players in the lobby).
    match_stats_cache: HashMap<String, serde_json::Map<String, serde_json::Value>>,
    name_cache: HashMap<String, String>,
}

impl Tracker {
    pub fn new() -> Self {
        let settings = Settings::load();
        let henrik_limiter = new_henrik_limiter(settings.henrik_rate_limit);
        Self {
            settings,
            state: AppState { phase: "offline".into(), ..Default::default() },
            pull_status: PullStatus::default(),
            creds: None,
            content: None,
            limiter: new_limiter(),
            henrik_limiter,
            stats_busy: Arc::new(AtomicBool::new(false)),
            mmr_cache: HashMap::new(),
            stats_cache: HashMap::new(),
            match_stats_cache: HashMap::new(),
            name_cache: HashMap::new(),
        }
    }
}

/// Background loop entry point — runs forever, emitting state-updated events.
pub async fn run_loop(tracker: Arc<Mutex<Tracker>>, app: tauri::AppHandle) {
    // Load content on startup (non-blocking relative to first paint)
    let http = make_remote_client();
    match load_content(&http).await {
        Ok(content) => {
            let mut t = tracker.lock().await;
            t.content = Some(content);
        }
        Err(e) => {
            tracing::warn!("Content load failed: {e}");
        }
    }

    loop {
        let interval = {
            let t = tracker.lock().await;
            Duration::from_secs(t.settings.poll_seconds.max(3))
        };

        poll_once(tracker.clone(), &app, &http).await;

        // Sleep interruptibly (wake event will be added in Phase D)
        sleep(interval).await;
    }
}

/// Run a single poll immediately (used by the manual Refresh button).
pub async fn force_poll(tracker: Arc<Mutex<Tracker>>, app: tauri::AppHandle) {
    let http = make_remote_client();
    poll_once(tracker, &app, &http).await;
}

async fn poll_once(tracker: Arc<Mutex<Tracker>>, app: &tauri::AppHandle, http: &reqwest::Client) {
    let result = do_poll(tracker.clone(), http).await;

    let stats_needed;
    {
        let mut t = tracker.lock().await;
        match result {
            Ok(mut state) => {
                // Carry forward already-resolved stats so re-polls don't blank them out.
                for p in &mut state.players {
                    if let Some(prev) = t.state.players.iter().find(|pp| pp.puuid == p.puuid) {
                        if !prev.stats_pending {
                            p.stats = prev.stats.clone();
                            p.stats_pending = false;
                        }
                    }
                }
                stats_needed = matches!(state.phase.as_str(), "pregame" | "ingame")
                    && !state.players.is_empty();
                t.state = state.clone();
                let _ = app.emit("state-updated", &state);
            }
            Err(PollError::Lockfile(e)) => {
                stats_needed = false;
                t.state = AppState {
                    connected: false,
                    phase: "offline".into(),
                    error: e.to_string(),
                    updated: unix_now(),
                    ..Default::default()
                };
                let _ = app.emit("state-updated", &t.state);
            }
            Err(e) => {
                stats_needed = false;
                t.state.error = e.to_string();
                t.state.updated = unix_now();
                let _ = app.emit("state-updated", &t.state);
            }
        }
    }

    // Second pass: fill in per-player recent-game stats. Run it DETACHED so the
    // poll loop keeps ticking (~7s) and can detect a new game while stats — which
    // can take minutes for a fresh lobby — resolve in the background. A busy flag
    // prevents overlapping passes; the running pass self-cancels per player once
    // the lobby changes (see resolve_stats' membership check).
    if stats_needed {
        let (creds, busy) = {
            let t = tracker.lock().await;
            (t.creds.clone(), t.stats_busy.clone())
        };
        if let Some(creds) = creds {
            if !busy.swap(true, Ordering::SeqCst) {
                let tracker2 = tracker.clone();
                let app2 = app.clone();
                let http2 = http.clone();
                tauri::async_runtime::spawn(async move {
                    resolve_stats(tracker2.clone(), &app2, &http2, &creds).await;
                    tracker2.lock().await.stats_busy.store(false, Ordering::SeqCst);
                });
            }
        }
    }
}

#[derive(Debug)]
enum PollError {
    Lockfile(LockfileError),
    Api(String),
}

impl std::fmt::Display for PollError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PollError::Lockfile(e) => write!(f, "{e}"),
            PollError::Api(s) => write!(f, "{s}"),
        }
    }
}

impl From<LockfileError> for PollError {
    fn from(e: LockfileError) -> Self { PollError::Lockfile(e) }
}
impl From<crate::riot::client::RiotApiError> for PollError {
    fn from(e: crate::riot::client::RiotApiError) -> Self { PollError::Api(e.to_string()) }
}

async fn do_poll(tracker: Arc<Mutex<Tracker>>, http: &reqwest::Client) -> Result<AppState, PollError> {
    // Read lockfile (cheap, proves game is running)
    let lockfile = read_lockfile()?;

    // Get/refresh credentials
    let (creds, _has_content, client_version) = {
        let t = tracker.lock().await;
        let c = t.creds.clone();
        let content_version = t.content.as_ref().map(|c| c.client_version.clone()).unwrap_or_default();
        (c, t.content.is_some(), content_version)
    };

    let local_http = make_local_client();
    let creds = match creds {
        Some(c) => c,
        None => {
            let c = get_credentials(&local_http, &lockfile, &client_version).await
                .map_err(|e| PollError::Api(e.to_string()))?;
            let mut t = tracker.lock().await;
            t.creds = Some(c.clone());
            c
        }
    };

    let limiter = { tracker.lock().await.limiter.clone() };
    let mut riot = RiotClient::new(http.clone(), creds.clone(), limiter);

    // Check game phase
    let coregame_id = riot.coregame_match_id().await.unwrap_or(None);
    let pregame_id = if coregame_id.is_none() {
        riot.pregame_match_id().await.unwrap_or(None)
    } else {
        None
    };

    if coregame_id.is_none() && pregame_id.is_none() {
        return Ok(AppState {
            connected: true,
            phase: "menus".into(),
            updated: unix_now(),
            ..Default::default()
        });
    }

    let (phase, parsed) = if let Some(mid) = coregame_id {
        let raw = riot.coregame_match(&mid).await?;
        ("ingame".to_string(), parse_coregame_match(&raw))
    } else {
        let mid = pregame_id.unwrap();
        let raw = riot.pregame_match(&mid).await?;
        ("pregame".to_string(), parse_pregame_match(&raw))
    };

    let puuids: Vec<String> = parsed.players.iter()
        .map(|p| p.puuid.clone())
        .filter(|p| !p.is_empty())
        .collect();

    // Name resolution
    let names = {
        let t = tracker.lock().await;
        let missing: Vec<String> = puuids.iter()
            .filter(|p| !t.name_cache.contains_key(*p))
            .cloned().collect();
        drop(t);
        if !missing.is_empty() {
            if let Ok(raw) = riot.name_service(&missing).await {
                let mut t = tracker.lock().await;
                for (puuid, name) in parse_name_service(&raw) {
                    t.name_cache.insert(puuid, name);
                }
            }
        }
        let t = tracker.lock().await;
        puuids.iter().map(|p| (p.clone(), t.name_cache.get(p).cloned().unwrap_or_default())).collect::<HashMap<_,_>>()
    };

    // MMR resolution (one per player, cached)
    let mmr_map = resolve_mmr(tracker.clone(), &mut riot, &puuids, &creds).await;

    // Map info
    let (map_name, map_image) = {
        let t = tracker.lock().await;
        if let Some(ref c) = t.content {
            let info = c.map(&parsed.map);
            (info.name, info.icon)
        } else {
            (String::new(), String::new())
        }
    };

    // Build players
    let self_puuid = creds.puuid.clone();
    let mut players = build_players(&parsed.players, &names, &mmr_map, &self_puuid, tracker.clone()).await;

    // Party detection from prior cached match data (instant, no extra requests).
    {
        let t = tracker.lock().await;
        detect_parties_in_place(&mut players, &t.match_stats_cache);
    }

    // Sort: self team first, self pinned top, then by rank
    let self_team = players.iter().find(|p| p.is_self).map(|p| p.team.clone()).unwrap_or_default();
    players.sort_by(|a, b| {
        let ta = if a.team == self_team { 0 } else { 1 };
        let tb = if b.team == self_team { 0 } else { 1 };
        ta.cmp(&tb)
            .then(if a.is_self { 0 } else { 1 }.cmp(&if b.is_self { 0 } else { 1 }))
            .then(b.current.tier.cmp(&a.current.tier))
            .then(b.current.rr.cmp(&a.current.rr))
    });

    // Self name
    let self_name = names.get(&self_puuid).cloned().unwrap_or_default();

    Ok(AppState {
        connected: true,
        phase,
        map: map_name,
        map_image,
        players,
        self_name,
        error: String::new(),
        updated: unix_now(),
    })
}

async fn resolve_mmr(
    tracker: Arc<Mutex<Tracker>>,
    riot: &mut RiotClient,
    puuids: &[String],
    _creds: &Credentials,
) -> HashMap<String, PlayerResolve> {
    let mut out = HashMap::new();
    let now = std::time::Instant::now();

    for puuid in puuids {
        // Check cache
        let cached = {
            let t = tracker.lock().await;
            t.mmr_cache.get(puuid).filter(|(ts, _)| now.duration_since(*ts) < MMR_TTL).map(|(_, r)| r.clone())
        };
        if let Some(res) = cached {
            out.insert(puuid.clone(), res);
            continue;
        }

        // Fetch
        let season = {
            let t = tracker.lock().await;
            t.content.as_ref().map(|c| c.current_season.clone()).unwrap_or_default()
        };
        match riot.competitive_updates(puuid, 20, 0).await {
            Ok(payload) => {
                let parsed = parse_rank_from_updates(&payload, &season);
                let rows = parse_competitive_updates(&payload);
                let recent_ids: Vec<String> = rows.iter()
                    .map(|r| r.match_id.clone())
                    .filter(|id| !id.is_empty())
                    .take(STATS_MAX)
                    .collect();
                let res = {
                    let t = tracker.lock().await;
                    let (current, peak) = if let Some(ref c) = t.content {
                        let ct = c.tier(parsed.tier);
                        let pt = c.tier(parsed.peak_tier);
                        (
                            RankInfo { tier: parsed.tier, tier_name: ct.name, rr: parsed.rr, tier_color: ct.color, tier_icon: ct.icon },
                            RankInfo { tier: parsed.peak_tier, tier_name: pt.name, rr: 0, tier_color: pt.color, tier_icon: pt.icon },
                        )
                    } else {
                        (RankInfo { tier: parsed.tier, rr: parsed.rr, ..Default::default() },
                         RankInfo { tier: parsed.peak_tier, ..Default::default() })
                    };
                    PlayerResolve { current, peak, recent_ids, pending: false }
                };
                {
                    let mut t = tracker.lock().await;
                    t.mmr_cache.insert(puuid.clone(), (std::time::Instant::now(), res.clone()));
                }
                out.insert(puuid.clone(), res);
            }
            Err(_) => {
                // Reuse a prior good result if we have one; otherwise mark as
                // still-pending so the row shows a spinner and retries next poll
                // (rather than falsely showing "Unranked" on a failed fetch).
                let cached = tracker.lock().await.mmr_cache.get(puuid).map(|(_, r)| r.clone());
                out.insert(puuid.clone(), cached.unwrap_or(PlayerResolve { pending: true, ..Default::default() }));
            }
        }
        sleep(Duration::from_millis(500)).await;
    }
    out
}

/// After ranks are painted, fill in each player's recent-game stats (KDA/ACS/
/// ADR/HS/WR) and re-emit progressively. All enemy players are fetched in
/// parallel (shared limiter keeps total spend under Riot's ceiling), so every
/// row fills simultaneously rather than one-by-one.
async fn resolve_stats(
    tracker: Arc<Mutex<Tracker>>,
    app: &tauri::AppHandle,
    http: &reqwest::Client,
    creds: &Credentials,
) {
    let (limiter, henrik_limiter, live_use_henrik, henrik_key) = {
        let t = tracker.lock().await;
        (
            t.limiter.clone(),
            t.henrik_limiter.clone(),
            t.settings.live_use_henrik,
            t.settings.henrik_key.clone(),
        )
    };
    let now = std::time::Instant::now();
    let self_puuid = creds.puuid.clone();

    // Resolve own stats instantly from profile store (no rate-limit cost).
    {
        let cached = {
            let t = tracker.lock().await;
            t.stats_cache.get(&self_puuid)
                .filter(|(ts, _)| now.duration_since(*ts) < STATS_TTL)
                .map(|(_, s)| s.clone())
        };
        let stats = if let Some(s) = cached {
            s
        } else {
            let season = { tracker.lock().await.content.as_ref().map(|c| c.current_season.clone()).unwrap_or_default() };
            let store = ProfileStore::open(&profile_store_dir(), &self_puuid);
            let agg = store.aggregate(&season);
            let stats = if agg.games > 0 { agg.agg } else { store.aggregate("").agg };
            tracker.lock().await.stats_cache.insert(self_puuid.clone(), (std::time::Instant::now(), stats.clone()));
            stats
        };
        apply_stats(&tracker, app, &self_puuid, stats).await;
    }

    // Snapshot enemy/ally puuids (everyone except self).
    let puuids: Vec<String> = {
        let t = tracker.lock().await;
        t.state.players.iter()
            .map(|p| p.puuid.clone())
            .filter(|p| p != &self_puuid)
            .collect()
    };

    // Paint cached players immediately and collect those needing a fresh fetch.
    let mut to_fetch: Vec<String> = Vec::new();
    for puuid in &puuids {
        let cached = {
            let t = tracker.lock().await;
            t.stats_cache.get(puuid)
                .filter(|(ts, _)| now.duration_since(*ts) < STATS_TTL)
                .map(|(_, s)| s.clone())
        };
        if let Some(stats) = cached {
            apply_stats(&tracker, app, puuid, stats).await;
        } else {
            to_fetch.push(puuid.clone());
        }
    }

    // Spawn one task per uncached player — all share the Riot limiter so the
    // total spend stays under the ceiling, but they interleave so every row
    // gets its first few games within the first budget window.
    let mut handles = Vec::new();
    for puuid in to_fetch {
        let tracker2 = tracker.clone();
        let app2 = app.clone();
        let http2 = http.clone();
        let creds2 = creds.clone();
        let limiter2 = limiter.clone();
        let henrik_limiter2 = henrik_limiter.clone();
        let henrik_key2 = henrik_key.clone();

        handles.push(tauri::async_runtime::spawn(async move {
            fetch_player_stats(
                tracker2, app2, http2, creds2, puuid,
                limiter2, henrik_limiter2, henrik_key2, live_use_henrik,
            ).await;
        }));
    }

    for h in handles {
        let _ = h.await;
    }

    // Re-run party detection now that match cache is fully populated.
    detect_parties(&tracker, app).await;
}

/// Per-player stats fetch: pulls up to STATS_MAX recent match-details from the
/// Riot local API (gated by the shared limiter), then optionally deepens with
/// HenrikDev stored-matches. Emits a progressive update after the Riot pass and
/// again after Henrik if new games were merged.
async fn fetch_player_stats(
    tracker: Arc<Mutex<Tracker>>,
    app: tauri::AppHandle,
    http: reqwest::Client,
    creds: Credentials,
    puuid: String,
    limiter: SharedLimiter,
    henrik_limiter: SharedLimiter,
    henrik_key: String,
    live_use_henrik: bool,
) {
    let mut riot = RiotClient::new(http.clone(), creds.clone(), limiter);

    let recent_ids: Vec<String> = {
        let t = tracker.lock().await;
        t.mmr_cache.get(&puuid).map(|(_, r)| r.recent_ids.clone()).unwrap_or_default()
    };
    if recent_ids.is_empty() {
        apply_stats(&tracker, &app, &puuid, None).await;
        return;
    }

    let mut lines: Vec<Option<serde_json::Value>> = Vec::new();
    let mut fetched_match_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

    // If Henrik is enabled, fire its single stored-matches call NOW in a background
    // task so it runs concurrently with the Riot match-details loop below.
    let henrik_task: Option<tauri::async_runtime::JoinHandle<Vec<serde_json::Map<String, serde_json::Value>>>> =
        if live_use_henrik && !henrik_key.is_empty() {
            let henrik = HenrikClient::new(http.clone(), henrik_key.clone(), henrik_limiter);
            let region = creds.region.clone();
            let puuid2 = puuid.clone();
            Some(tauri::async_runtime::spawn(async move {
                match henrik.stored_matches_by_puuid(&region, &puuid2, 1, 20).await {
                    Ok(result) => parse_stored_matches(&result, &puuid2).0,
                    Err(_) => vec![],
                }
            }))
        } else {
            None
        };

    for mid in &recent_ids {
        // Bail if this player left the lobby.
        {
            let t = tracker.lock().await;
            if !t.state.players.iter().any(|p| p.puuid == puuid) {
                if let Some(h) = henrik_task { h.abort(); }
                return;
            }
        }

        let statmap_opt = {
            let t = tracker.lock().await;
            t.match_stats_cache.get(mid).cloned()
        };

        let statmap = match statmap_opt {
            Some(m) => m,
            None => match riot.match_details(mid).await {
                Ok(Some(details)) => {
                    let m = extract_all_match_stats(&details);
                    tracker.lock().await.match_stats_cache.insert(mid.clone(), m.clone());
                    sleep(Duration::from_millis(200)).await;
                    m
                }
                _ => { sleep(Duration::from_millis(500)).await; continue; }
            },
        };

        fetched_match_ids.insert(mid.clone());
        if let Some(line) = statmap.get(&puuid) {
            lines.push(Some(line.clone()));
        }

        // Progressive update after each game.
        if !lines.is_empty() {
            let partial = aggregate_stats(&lines);
            apply_stats(&tracker, &app, &puuid, partial).await;
        }
    }

    // Merge Henrik results (they've been running in parallel — await now).
    if let Some(task) = henrik_task {
        if let Ok(henrik_games) = task.await {
            let mut added = 0usize;
            for game in &henrik_games {
                let mid = game.get("match_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                if !mid.is_empty() && !fetched_match_ids.contains(&mid) {
                    fetched_match_ids.insert(mid);
                    lines.push(Some(serde_json::Value::Object(game.clone())));
                    added += 1;
                }
            }
            if added > 0 {
                let deep_stats = aggregate_stats(&lines);
                tracker.lock().await.stats_cache.insert(puuid.clone(), (std::time::Instant::now(), deep_stats.clone()));
                apply_stats(&tracker, &app, &puuid, deep_stats).await;
                return;
            }
        }
    }

    // Final Riot-only aggregate.
    let stats = aggregate_stats(&lines);
    tracker.lock().await.stats_cache.insert(puuid.clone(), (std::time::Instant::now(), stats.clone()));
    apply_stats(&tracker, &app, &puuid, stats).await;
}

/// Scan the match cache for current lobby players who share a party_id in any
/// recent game — those players queued together. Assigns sequential group numbers
/// and sets party_group / party_confirmed in-place on the player list.
fn detect_parties_in_place(
    players: &mut Vec<MatchPlayer>,
    match_cache: &HashMap<String, serde_json::Map<String, serde_json::Value>>,
) {
    let puuids: Vec<&str> = players.iter().map(|p| p.puuid.as_str()).collect();

    // party_uuid -> set of lobby puuids that appeared together under that party id.
    let mut confirmed: HashMap<String, Vec<String>> = HashMap::new();
    for statmap in match_cache.values() {
        let mut by_party: HashMap<&str, Vec<String>> = HashMap::new();
        for &puuid in &puuids {
            if let Some(line) = statmap.get(puuid) {
                let pid = line.get("party").and_then(|v| v.as_str()).unwrap_or("");
                if !pid.is_empty() {
                    by_party.entry(pid).or_default().push(puuid.to_string());
                }
            }
        }
        for (pid, group) in by_party {
            if group.len() >= 2 {
                let entry = confirmed.entry(pid.to_string()).or_default();
                for p in &group {
                    if !entry.contains(p) { entry.push(p.clone()); }
                }
            }
        }
    }
    if confirmed.is_empty() { return; }

    // Assign sequential group numbers, largest groups first for stable ordering.
    let mut groups: Vec<Vec<String>> = confirmed.into_values().collect();
    groups.sort_by(|a, b| b.len().cmp(&a.len()));
    let mut puuid_to_group: HashMap<String, i32> = HashMap::new();
    for (i, group) in groups.into_iter().enumerate() {
        for puuid in group {
            puuid_to_group.entry(puuid).or_insert((i as i32) + 1);
        }
    }

    for p in players.iter_mut() {
        if let Some(&g) = puuid_to_group.get(&p.puuid) {
            p.party_group = g;
            p.party_confirmed = true;
        }
    }
}

/// Re-run party detection on the live state using freshly cached match data and
/// emit a state-updated event if anything changed.
async fn detect_parties(tracker: &Arc<Mutex<Tracker>>, app: &tauri::AppHandle) {
    let changed = {
        let mut t = tracker.lock().await;
        let cache = t.match_stats_cache.clone();
        for p in &mut t.state.players { p.party_group = 0; p.party_confirmed = false; }
        detect_parties_in_place(&mut t.state.players, &cache);
        true // always emit — groupings only appear, never regress mid-match
    };
    if changed {
        let state = tracker.lock().await.state.clone();
        let _ = app.emit("state-updated", &state);
    }
}

/// Write a player's resolved stats into live state and emit an update.
async fn apply_stats(
    tracker: &Arc<Mutex<Tracker>>,
    app: &tauri::AppHandle,
    puuid: &str,
    stats: Option<PlayerStats>,
) {
    let state = {
        let mut t = tracker.lock().await;
        let mut changed = false;
        if let Some(p) = t.state.players.iter_mut().find(|p| p.puuid == puuid) {
            p.stats = stats;
            p.stats_pending = false;
            changed = true;
        }
        if changed { Some(t.state.clone()) } else { None }
    };
    if let Some(s) = state {
        let _ = app.emit("state-updated", &s);
    }
}

async fn build_players(
    raw: &[crate::riot::parse::RawPlayer],
    names: &HashMap<String, String>,
    mmr_map: &HashMap<String, PlayerResolve>,
    self_puuid: &str,
    tracker: Arc<Mutex<Tracker>>,
) -> Vec<MatchPlayer> {
    raw.iter().map(|p| {
        let resolve = mmr_map.get(&p.puuid).cloned().unwrap_or(PlayerResolve { pending: true, ..Default::default() });
        // "Loading" only while the fetch is genuinely unresolved — a resolved
        // player with tier 0 is Unranked, not still loading. A pregame inline
        // tier also counts as resolved.
        let pending = resolve.pending && p.tier == 0;
        let rank = if !resolve.pending {
            // Resolved — use it verbatim. For tier 0 this carries the proper
            // "Unrated" name/color, so unranked players show a label, not blank.
            resolve.current.clone()
        } else if p.tier > 0 {
            // Not resolved yet, but pregame gave an inline tier.
            RankInfo { tier: p.tier, ..Default::default() }
        } else {
            RankInfo::default()
        };
        let peak = resolve.peak.clone();
        let (agent_name, agent_icon, agent_color) = {
            let guard = tracker.try_lock().ok();
            if let Some(g) = guard {
                if let Some(ref c) = g.content {
                    let ai = c.agent(&p.agent);
                    (ai.name, ai.icon, ai.color)
                } else { (String::new(), String::new(), "#4b5160".into()) }
            } else { (String::new(), String::new(), "#4b5160".into()) }
        };
        MatchPlayer {
            puuid: p.puuid.clone(),
            team: p.team.clone(),
            name: names.get(&p.puuid).cloned().unwrap_or_default(),
            incognito: p.incognito,
            agent_id: p.agent.clone(),
            agent_name,
            agent_icon,
            agent_color,
            account_level: p.level,
            hide_level: p.hide_level,
            current: rank.clone(),
            peak,
            wins: 0,
            is_self: p.puuid == self_puuid,
            pending,
            stats: None,
            stats_pending: true,
            party_group: 0,
            party_confirmed: false,
        }
    }).collect()
}

fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

fn make_remote_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("Failed to build HTTP client")
}

fn make_local_client() -> reqwest::Client {
    reqwest::Client::builder()
        .danger_accept_invalid_certs(true)   // Riot local endpoint uses self-signed cert
        .timeout(Duration::from_secs(5))
        .build()
        .expect("Failed to build local HTTP client")
}
