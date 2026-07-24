/// Tauri command layer — bridges Svelte invoke() calls to the tracker and Riot APIs.
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tauri::{State, Emitter};

use crate::AppTracker;
use crate::models::{AppSettings, AppState, PullStatus};

use crate::store::profile::{ProfileStore, profile_store_dir};
use crate::riot::parse::{parse_rank_from_updates, parse_competitive_updates};
use crate::henrik::{HenrikClient, parse_account, parse_mmr, parse_matches};

// ── Live state ─────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_state(tracker: State<'_, AppTracker>) -> Result<AppState, String> {
    Ok(tracker.lock().await.state.clone())
}

#[tauri::command]
pub async fn refresh_now(tracker: State<'_, AppTracker>, app: tauri::AppHandle) -> Result<(), String> {
    // Force an immediate poll (detect phase/lobby changes now, don't wait ~7s).
    let t = tracker.inner().clone();
    crate::tracker::force_poll(t, app).await;
    Ok(())
}

// ── Profile ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn refresh_profile(tracker: State<'_, AppTracker>) -> Result<serde_json::Value, String> {
    let (puuid, season, creds, limiter) = {
        let t = tracker.lock().await;
        let creds = t.creds.clone().ok_or("Not connected — launch VALORANT first")?;
        let season = t.content.as_ref().map(|c| c.current_season.clone()).unwrap_or_default();
        (creds.puuid.clone(), season, creds, t.limiter.clone())
    };

    let http = remote_client();
    let mut riot = crate::riot::client::RiotClient::new(http.clone(), creds.clone(), limiter);

    // Fetch competitive updates
    let updates = riot.competitive_updates(&puuid, 20, 0).await
        .map_err(|e| e.to_string())?;

    let parsed = parse_rank_from_updates(&updates, &season);
    let history_rows = parse_competitive_updates(&updates);

    // Get profile store for deep stats
    let dir = profile_store_dir();
    let store = ProfileStore::open(&dir, &puuid);

    // Build RR history points, resolving map + tier display names via content,
    // and merging per-game stats (agent/KDA/HS%) from the store when available.
    let history: Vec<serde_json::Value> = {
        let t = tracker.lock().await;
        let content = t.content.as_ref();
        history_rows.iter().map(|r| {
            let map_name = content
                .map(|c| c.map(&r.map).name)
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| r.map.rsplit('/').next().unwrap_or("").to_string());
            let tier_name = content.map(|c| c.tier(r.tier).name).unwrap_or_default();

            // Per-game stats from the store (present only for deep-pulled games).
            let (agent, kills, deaths, assists, hs_pct) = match store.record(&r.match_id) {
                Some(rec) => {
                    let k = rec.get("kills").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let d = rec.get("deaths").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let a = rec.get("assists").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let hs = rec.get("headshots").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let bs = rec.get("bodyshots").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let ls = rec.get("legshots").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let total = hs + bs + ls;
                    let hs_pct = if total > 0.0 { (hs / total * 100.0).round() as i64 } else { 0 };
                    let agent_id = rec.get("agent").and_then(|v| v.as_str()).unwrap_or("");
                    let agent_name = content.map(|c| c.agent(agent_id).name).unwrap_or_default();
                    (agent_name, k as i64, d as i64, a as i64, hs_pct)
                }
                None => (String::new(), -1, -1, -1, -1),
            };

            serde_json::json!({
                "match_id": r.match_id,
                "map_name": map_name,
                "tier": r.tier,
                "tier_name": tier_name,
                "rr_after": r.rr_after,
                "rr_change": r.rr_change,
                "elo": r.elo,
                "date_ms": r.date_ms,
                "agent": agent,
                "kills": kills,
                "deaths": deaths,
                "assists": assists,
                "hs": hs_pct,
            })
        }).collect()
    };
    let act_stats = store.aggregate(&season);
    let lifetime_stats = store.aggregate("");

    let deep = serde_json::json!({
        "games_total": lifetime_stats.games,
        "games_act": act_stats.games,
        "lifetime": lifetime_stats.agg,
        "act": act_stats.agg,
        "oldest_ms": store.oldest_ms(),
        "newest_ms": store.newest_ms(),
        "scanned": store.scanned(),
    });

    // Resolve rank info from content
    let (tier_name, tier_color, tier_icon) = {
        let t = tracker.lock().await;
        if let Some(ref c) = t.content {
            let ti = c.tier(parsed.tier);
            (ti.name, ti.color, ti.icon)
        } else {
            (String::new(), "#8b90a0".into(), String::new())
        }
    };
    let (peak_name, peak_color, peak_icon) = {
        let t = tracker.lock().await;
        if let Some(ref c) = t.content {
            let ti = c.tier(parsed.peak_tier);
            (ti.name, ti.color, ti.icon)
        } else {
            (String::new(), "#8b90a0".into(), String::new())
        }
    };

    Ok(serde_json::json!({
        "puuid": puuid,
        "name": tracker.lock().await.state.self_name.clone(),
        "current": {
            "tier": parsed.tier,
            "tier_name": tier_name,
            "rr": parsed.rr,
            "tier_color": tier_color,
            "tier_icon": tier_icon,
        },
        "peak": {
            "tier": parsed.peak_tier,
            "tier_name": peak_name,
            "rr": 0,
            "tier_color": peak_color,
            "tier_icon": peak_icon,
        },
        "wins": parsed.wins,
        "history": history,
        "deep": deep,
    }))
}

// ── Profile deep pull ────────────────────────────────────────────────────────

#[tauri::command]
pub async fn profile_pull_start(
    tracker: State<'_, AppTracker>,
    app: tauri::AppHandle,
    count: i32,
    want_max: bool,
) -> Result<serde_json::Value, String> {
    {
        let mut t = tracker.lock().await;
        if t.pull_status.running {
            return Ok(serde_json::json!({ "error": "Pull already running" }));
        }
        t.pull_status = PullStatus {
            running: true,
            target: if want_max { 0 } else { count },
            want_max,
            ..Default::default()
        };
    }

    let tracker_clone = Arc::clone(&tracker);
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        run_profile_pull(tracker_clone, app_clone, count, want_max).await;
    });

    Ok(serde_json::json!({ "ok": true }))
}

#[tauri::command]
pub async fn profile_pull_status(tracker: State<'_, AppTracker>) -> Result<PullStatus, String> {
    Ok(tracker.lock().await.pull_status.clone())
}

#[tauri::command]
pub async fn profile_pull_cancel(tracker: State<'_, AppTracker>) -> Result<(), String> {
    tracker.lock().await.pull_status.running = false;
    Ok(())
}

#[tauri::command]
pub async fn profile_stats(tracker: State<'_, AppTracker>) -> Result<serde_json::Value, String> {
    let puuid = {
        let t = tracker.lock().await;
        t.creds.as_ref().map(|c| c.puuid.clone()).ok_or("Not connected")?
    };
    let season = tracker.lock().await.content.as_ref().map(|c| c.current_season.clone()).unwrap_or_default();

    let dir = profile_store_dir();
    let store = ProfileStore::open(&dir, &puuid);
    let lifetime = store.aggregate("");
    let act = store.aggregate(&season);

    Ok(serde_json::json!({
        "games_total": lifetime.games,
        "games_act": act.games,
        "lifetime": lifetime.agg,
        "act": act.agg,
        "oldest_ms": store.oldest_ms(),
        "newest_ms": store.newest_ms(),
        "scanned": store.scanned(),
    }))
}

#[tauri::command]
pub async fn profile_delete_oldest(
    tracker: State<'_, AppTracker>,
    n: usize,
) -> Result<i32, String> {
    let puuid = {
        let t = tracker.lock().await;
        t.creds.as_ref().map(|c| c.puuid.clone()).ok_or("Not connected")?
    };
    let dir = profile_store_dir();
    let mut store = ProfileStore::open(&dir, &puuid);
    let removed = store.delete_oldest(n);
    store.save();
    Ok(removed)
}

// ── Player lookup ─────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn lookup(
    tracker: State<'_, AppTracker>,
    riot_id: String,
) -> Result<serde_json::Value, String> {
    let (name, tag) = match riot_id.split_once('#') {
        Some((n, t)) => (n.to_string(), t.to_string()),
        None => return Ok(serde_json::json!({ "error": "Enter a Riot ID like Name#TAG" })),
    };

    let (key, region) = {
        let t = tracker.lock().await;
        let k = t.settings.henrik_key.clone();
        let r = t.settings.region_override.clone();
        (k, r)
    };

    if key.is_empty() {
        return Ok(serde_json::json!({ "error": "No HenrikDev key set — add one in Settings." }));
    }

    let henrik_limiter = { tracker.lock().await.henrik_limiter.clone() };
    let http = remote_client();
    let henrik = HenrikClient::new(http, key, henrik_limiter);

    let account = match henrik.account(&name, &tag).await {
        Ok(v) => v,
        Err(e) => return Ok(serde_json::json!({ "error": e.to_string() })),
    };
    let account_parsed = match parse_account(&account) {
        Some(v) => v,
        None => return Ok(serde_json::json!({ "error": "Failed to parse account response." })),
    };

    let effective_region = if !region.is_empty() {
        region.clone()
    } else {
        account_parsed.get("region")
            .and_then(|v| v.as_str())
            .unwrap_or("eu")
            .to_string()
    };

    let mmr = henrik.mmr(&effective_region, &name, &tag).await;
    let mmr_parsed = mmr.ok().map(|m| parse_mmr(&m));

    let matches = henrik.matches(&effective_region, &name, &tag).await.ok()
        .map(|m| parse_matches(&m, &name, &tag))
        .unwrap_or_default();

    Ok(serde_json::json!({
        "account": account_parsed,
        "mmr": mmr_parsed,
        "matches": matches,
    }))
}

// ── Settings ──────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_settings(tracker: State<'_, AppTracker>) -> Result<AppSettings, String> {
    let t = tracker.lock().await;
    let s = &t.settings;
    Ok(AppSettings {
        region_override: s.region_override.clone(),
        poll_seconds: s.poll_seconds,
        has_key: !s.henrik_key.is_empty(),
        key_hint: if s.henrik_key.len() > 8 {
            format!("{}…{}", &s.henrik_key[..4], &s.henrik_key[s.henrik_key.len()-4..])
        } else if !s.henrik_key.is_empty() {
            "set".into()
        } else {
            String::new()
        },
        profile_pull_target: s.profile_pull_target,
        pull_source: s.pull_source.clone(),
        always_on_top: s.always_on_top,
        henrik_rate_limit: s.henrik_rate_limit,
        live_use_henrik: s.live_use_henrik,
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

#[tauri::command]
pub async fn save_settings(
    tracker: State<'_, AppTracker>,
    data: serde_json::Value,
) -> Result<AppSettings, String> {
    let mut t = tracker.lock().await;
    if let Some(v) = data.get("region_override").and_then(|v| v.as_str()) {
        t.settings.region_override = v.to_string();
    }
    if let Some(v) = data.get("poll_seconds").and_then(|v| v.as_u64()) {
        t.settings.poll_seconds = v.max(3);
    }
    if let Some(v) = data.get("henrik_key").and_then(|v| v.as_str()) {
        if !v.is_empty() { t.settings.henrik_key = v.to_string(); }
    }
    if let Some(v) = data.get("profile_pull_target").and_then(|v| v.as_i64()) {
        t.settings.profile_pull_target = v as i32;
    }
    if let Some(v) = data.get("pull_source").and_then(|v| v.as_str()) {
        t.settings.pull_source = v.to_string();
    }
    if let Some(v) = data.get("henrik_rate_limit").and_then(|v| v.as_u64()) {
        let new_limit = (v.clamp(10, 300)) as u32;
        if t.settings.henrik_rate_limit != new_limit {
            t.settings.henrik_rate_limit = new_limit;
            t.henrik_limiter = crate::henrik::new_henrik_limiter(new_limit);
        }
    }
    if let Some(v) = data.get("live_use_henrik").and_then(|v| v.as_bool()) {
        t.settings.live_use_henrik = v;
    }
    t.settings.save();
    let s = &t.settings;
    Ok(AppSettings {
        region_override: s.region_override.clone(),
        poll_seconds: s.poll_seconds,
        has_key: !s.henrik_key.is_empty(),
        key_hint: if s.henrik_key.len() > 8 {
            format!("{}…{}", &s.henrik_key[..4], &s.henrik_key[s.henrik_key.len()-4..])
        } else if !s.henrik_key.is_empty() {
            "set".into()
        } else { String::new() },
        profile_pull_target: s.profile_pull_target,
        pull_source: s.pull_source.clone(),
        always_on_top: s.always_on_top,
        henrik_rate_limit: s.henrik_rate_limit,
        live_use_henrik: s.live_use_henrik,
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

#[tauri::command]
pub async fn set_always_on_top(
    tracker: State<'_, AppTracker>,
    app: tauri::AppHandle,
    on: bool,
) -> Result<(), String> {
    use tauri::Manager;
    let window = app.get_webview_window("main")
        .ok_or("Window not found")?;
    window.set_always_on_top(on).map_err(|e| e.to_string())?;
    tracker.lock().await.settings.always_on_top = on;
    Ok(())
}

// ── Internal: profile deep pull ───────────────────────────────────────────────

pub async fn run_profile_pull_pub(tracker: Arc<Mutex<crate::tracker::Tracker>>, app: tauri::AppHandle, count: i32, want_max: bool) {
    run_profile_pull(tracker, app, count, want_max).await;
}

async fn run_profile_pull(tracker: Arc<Mutex<crate::tracker::Tracker>>, app: tauri::AppHandle, count: i32, want_max: bool) {
    // want_max pulls until history is exhausted; otherwise stop at `count`.
    let target = if want_max { i32::MAX } else { count };
    let (puuid, creds, season, henrik_key, region, pull_source) = {
        let t = tracker.lock().await;
        let creds = match t.creds.clone() {
            Some(c) => c,
            None => {
                let mut t2 = tracker.lock().await;
                t2.pull_status.running = false;
                t2.pull_status.error = "Not connected".into();
                t2.pull_status.done = true;
                return;
            }
        };
        let season = t.content.as_ref().map(|c| c.current_season.clone()).unwrap_or_default();
        let key = t.settings.henrik_key.clone();
        let region = t.settings.region_override.clone();
        let src = t.settings.pull_source.clone();
        (creds.puuid.clone(), creds, season, key, region, src)
    };

    let dir = profile_store_dir();
    let http = remote_client();

    // HenrikDev needs a play region (na/eu/ap/kr). Fall back to the region
    // detected from the game client when the user left it on Auto-detect —
    // otherwise the stored-matches URL is malformed and returns nothing.
    let henrik_region = if region.is_empty() { creds.region.clone() } else { region.clone() };

    let added = if pull_source == "henrik" && !henrik_key.is_empty() {
        pull_via_henrik(&tracker, &app, &http, &henrik_key, &puuid, &henrik_region, &dir, target, &season).await
    } else {
        pull_via_riot(&tracker, &app, &http, &creds, &puuid, &dir, target, &season).await
    };

    let mut t = tracker.lock().await;
    t.pull_status.running = false;
    t.pull_status.done = true;
    t.pull_status.added = added;
    let _ = app.emit("pull-status", &t.pull_status);
}

async fn pull_via_riot(
    tracker: &Arc<Mutex<crate::tracker::Tracker>>,
    app: &tauri::AppHandle,
    http: &reqwest::Client,
    creds: &crate::riot::local::Credentials,
    puuid: &str,
    dir: &std::path::Path,
    target: i32,
    season: &str,
) -> i32 {
    let mut store = ProfileStore::open(dir, puuid);
    let known = store.known_ids();
    let skipped = store.skipped_ids();
    let limiter = { tracker.lock().await.limiter.clone() };
    let mut riot = crate::riot::client::RiotClient::new(http.clone(), creds.clone(), limiter);
    let mut added = 0i32;
    let mut start = 0u32;
    let batch_size = 20u32;

    loop {
        {
            let t = tracker.lock().await;
            if !t.pull_status.running { break; }
        }
        if added >= target { break; }

        let updates = match riot.competitive_updates(puuid, batch_size, start).await {
            Ok(v) => v,
            Err(_) => break,
        };
        let rows = crate::riot::parse::parse_competitive_updates(&updates);
        if rows.is_empty() { break; }

        for row in &rows {
            if known.contains(&row.match_id) || skipped.contains(&row.match_id) {
                continue;
            }
            if added >= target { break; }

            tokio::time::sleep(Duration::from_millis(600)).await;
            match riot.match_details(&row.match_id).await {
                Ok(Some(details)) => {
                    if let Some(statline) = crate::riot::parse::extract_own_game(&details, puuid) {
                        let mut val = statline;
                        if let Some(obj) = val.as_object_mut() {
                            obj.insert("match_id".into(), row.match_id.clone().into());
                            obj.insert("start_ms".into(), row.date_ms.into());
                            obj.insert("season".into(), season.to_string().into());
                        }
                        store.add(&row.match_id, val);
                        added += 1;
                        {
                            let mut t = tracker.lock().await;
                            t.pull_status.added = added;
                            t.pull_status.total_games = store.count();
                        }
                        let _ = app.emit("pull-status", &tracker.lock().await.pull_status);
                    } else {
                        store.mark_skipped(&row.match_id);
                    }
                }
                Ok(None) => { store.mark_skipped(&row.match_id); }
                Err(_) => {}
            }
        }
        start += batch_size;
    }

    store.set_scanned(store.count());
    store.save();
    added
}

async fn pull_via_henrik(
    tracker: &Arc<Mutex<crate::tracker::Tracker>>,
    app: &tauri::AppHandle,
    http: &reqwest::Client,
    key: &str,
    puuid: &str,
    region: &str,
    dir: &std::path::Path,
    target: i32,
    season: &str,
) -> i32 {
    let henrik_limiter = { tracker.lock().await.henrik_limiter.clone() };
    let henrik = HenrikClient::new(http.clone(), key.to_string(), henrik_limiter);
    let mut store = ProfileStore::open(dir, puuid);
    let known = store.known_ids();
    let mut added = 0i32;
    let mut page = 1u32; // HenrikDev stored-matches is 1-indexed (page=0 → HTTP 400)

    loop {
        {
            let t = tracker.lock().await;
            if !t.pull_status.running { break; }
        }
        if added >= target { break; }

        // Rate limiting is now handled by HenrikClient::gate() inside get().

        let result = match henrik.stored_matches_by_puuid(region, puuid, page, 20).await {
            Ok(v) => v,
            Err(e) => {
                // Only report if we got nothing at all — a mid-pagination error
                // just means we've reached the end of what's available.
                if added == 0 {
                    let mut t = tracker.lock().await;
                    t.pull_status.error = format!("HenrikDev: {e}");
                }
                break;
            }
        };

        let (games, after) = crate::henrik::parse_stored_matches(&result, puuid);
        if games.is_empty() { break; }

        for game in &games {
            let match_id = game.get("match_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if known.contains(&match_id) { continue; }
            if added >= target { break; }

            let mut val = serde_json::Value::Object(game.clone());
            if let Some(obj) = val.as_object_mut() {
                obj.entry("start_ms").or_insert(0u64.into());
                // Keep the game's real season from the parser; only fall back to
                // the current act if it's somehow missing.
                obj.entry("season").or_insert(season.to_string().into());
            }
            store.add(&match_id, val);
            added += 1;
            {
                let mut t = tracker.lock().await;
                t.pull_status.added = added;
                t.pull_status.history_seen += 1;
                t.pull_status.total_games = store.count();
            }
            let _ = app.emit("pull-status", &tracker.lock().await.pull_status);
        }

        if after == 0 {
            tracker.lock().await.pull_status.history_end = true;
            break;
        }
        page += 1;
    }

    store.set_scanned(store.count());
    store.save();
    added
}

fn remote_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("Failed to build HTTP client")
}
