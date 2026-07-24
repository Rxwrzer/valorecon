/// Pure parsers for Riot API responses.
/// Ported from backend/riot_api.py — these are unit-testable with no network.
use serde_json::Value;

use crate::models::PlayerStats;

// ── Rank reconstruction from competitiveupdates ───────────────────────────
// Riot's /mmr/v1/players/{puuid} is dead (404). We reconstruct rank from
// the competitiveupdates endpoint.

#[derive(Debug, Clone, Default)]
pub struct ParsedRank {
    pub tier: i32,
    pub rr: i32,
    pub wins: i32,
    pub peak_tier: i32,
}

pub fn parse_rank_from_updates(payload: &Value, current_season: &str) -> ParsedRank {
    let matches = match payload.get("Matches").and_then(|m| m.as_array()) {
        Some(m) => m,
        None => return ParsedRank::default(),
    };

    let mut tier = 0i32;
    let mut rr = 0i32;
    let mut wins = 0i32;
    let mut peak_tier = 0i32;
    let mut got_current = false;

    for m in matches.iter() {
        let season = m.get("SeasonID").and_then(|s| s.as_str()).unwrap_or("");
        let cur_t = m.get("TierAfterUpdate").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        let cur_rr = m.get("RankedRatingAfterUpdate").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        let rr_before = m.get("RankedRatingBeforeUpdate").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        // A win is a positive RR delta. Prefer Riot's RankedRatingEarned (signed,
        // correct across tier promotions/demotions); fall back to after-before.
        // CompetitiveMovement is unreliable (MOVEMENT_UNKNOWN at Immortal+).
        let earned = m.get("RankedRatingEarned").and_then(|v| v.as_i64())
            .map(|v| v as i32)
            .unwrap_or(cur_rr - rr_before);
        let won = earned > 0;

        // Most recent match overall = current standing. Take it unconditionally;
        // the season guard was dropping valid current rank when SeasonID format
        // didn't match the content service's season id.
        if !got_current {
            tier = cur_t;
            rr = cur_rr;
            got_current = true;
        }
        if cur_t > peak_tier {
            peak_tier = cur_t;
        }
        if season == current_season || current_season.is_empty() {
            if won { wins += 1; }
        }
    }

    ParsedRank { tier, rr, wins, peak_tier }
}

// ── competitiveupdates rows ───────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct CompUpdateRow {
    pub match_id: String,
    pub tier: i32,
    pub rr_after: i32,
    pub rr_change: i32,
    pub elo: i32,
    pub map: String,
    pub date_ms: u64,
}

pub fn parse_competitive_updates(payload: &Value) -> Vec<CompUpdateRow> {
    let matches = match payload.get("Matches").and_then(|m| m.as_array()) {
        Some(m) => m,
        None => return vec![],
    };
    matches.iter().filter_map(|m| {
        let match_id = m.get("MatchID")?.as_str()?.to_string();
        if match_id.is_empty() { return None; }
        let tier = m.get("TierAfterUpdate").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        let rr_after = m.get("RankedRatingAfterUpdate").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        let rr_before = m.get("RankedRatingBeforeUpdate").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        let rr_change = m.get("RankedRatingEarned").and_then(|v| v.as_i64())
            .map(|v| v as i32)
            .unwrap_or(rr_after - rr_before);
        let elo = tier * 100 + rr_after;
        let map = m.get("MapID").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let date_ms = m.get("MatchStartTime").and_then(|v| v.as_u64()).unwrap_or(0);
        Some(CompUpdateRow { match_id, tier, rr_after, rr_change, elo, map, date_ms })
    }).collect()
}

// ── Name service ──────────────────────────────────────────────────────────

pub fn parse_name_service(data: &[Value]) -> Vec<(String, String)> {
    data.iter().filter_map(|p| {
        let puuid = p.get("Subject")?.as_str()?.to_string();
        let name = p.get("GameName").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let tag = p.get("TagLine").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let display = if tag.is_empty() { name.clone() } else { format!("{}#{}", name, tag) };
        Some((puuid, display))
    }).collect()
}

// ── Pregame / coregame ────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct RawPlayer {
    pub puuid: String,
    pub team: String,
    pub agent: String,
    pub incognito: bool,
    pub level: i32,
    pub hide_level: bool,
    pub tier: i32,
}

#[derive(Debug, Clone)]
pub struct ParsedMatch {
    pub map: String,
    pub players: Vec<RawPlayer>,
}

pub fn parse_pregame_match(data: &Value) -> ParsedMatch {
    let map = data.get("MapID").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let allies = data
        .get("AllyTeam")
        .and_then(|t| t.get("Players"))
        .and_then(|p| p.as_array());
    let mut players = vec![];
    if let Some(arr) = allies {
        for p in arr {
            players.push(RawPlayer {
                puuid: p.get("Subject").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                team: "Blue".to_string(),
                agent: p.get("CharacterID").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                incognito: p.get("Incognito").and_then(|v| v.as_bool()).unwrap_or(false),
                level: p.get("PlayerIdentity").and_then(|id| id.get("AccountLevel"))
                    .and_then(|v| v.as_i64()).unwrap_or(0) as i32,
                hide_level: p.get("PlayerIdentity").and_then(|id| id.get("HideAccountLevel"))
                    .and_then(|v| v.as_bool()).unwrap_or(false),
                tier: p.get("SeasonalBadgeInfo").and_then(|b| b.get("Rank"))
                    .and_then(|v| v.as_i64()).unwrap_or(0) as i32,
            });
        }
    }
    ParsedMatch { map, players }
}

pub fn parse_coregame_match(data: &Value) -> ParsedMatch {
    let map = data.get("MapID").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let players_arr = data.get("Players").and_then(|p| p.as_array());
    let mut players = vec![];
    if let Some(arr) = players_arr {
        for p in arr {
            let team = p.get("TeamID").and_then(|v| v.as_str()).unwrap_or("").to_string();
            players.push(RawPlayer {
                puuid: p.get("Subject").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                team,
                agent: p.get("CharacterID").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                incognito: p.get("Incognito").and_then(|v| v.as_bool()).unwrap_or(false),
                level: p.get("PlayerIdentity").and_then(|id| id.get("AccountLevel"))
                    .and_then(|v| v.as_i64()).unwrap_or(0) as i32,
                hide_level: p.get("PlayerIdentity").and_then(|id| id.get("HideAccountLevel"))
                    .and_then(|v| v.as_bool()).unwrap_or(false),
                tier: 0, // not available in coregame
            });
        }
    }
    ParsedMatch { map, players }
}

// ── Match details stats ───────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct PlayerStatLine {
    pub kills: f64,
    pub deaths: f64,
    pub assists: f64,
    pub score: f64,
    pub damage: f64,
    pub headshots: i32,
    pub bodyshots: i32,
    pub legshots: i32,
    pub won: bool,
    pub party_id: String,
}

pub fn extract_all_match_stats(data: &Value) -> serde_json::Map<String, Value> {
    let mut out = serde_json::Map::new();
    let players = match data.get("players").and_then(|p| p.as_array()) {
        Some(p) => p,
        None => return out,
    };
    let teams = data.get("teams").and_then(|t| t.as_array());
    let winning_team = teams.and_then(|t| {
        t.iter().find_map(|team| {
            if team.get("won").and_then(|v| v.as_bool()).unwrap_or(false) {
                team.get("teamId").and_then(|v| v.as_str()).map(|s| s.to_string())
            } else {
                None
            }
        })
    }).unwrap_or_default();

    // Damage, shot counts, and combat score are NOT reliable in player.stats —
    // they live in roundResults[].playerStats[]. Aggregate per puuid here; this
    // is the authoritative source for ACS, ADR, and HS%.
    // Tuple: (damage, headshots, bodyshots, legshots, score)
    let mut shot_agg: std::collections::HashMap<String, (f64, i64, i64, i64, f64)> = std::collections::HashMap::new();
    if let Some(rounds) = data.get("roundResults").and_then(|r| r.as_array()) {
        for round in rounds {
            let pstats = match round.get("playerStats").and_then(|p| p.as_array()) {
                Some(p) => p,
                None => continue,
            };
            for ps in pstats {
                let subject = match ps.get("subject").and_then(|v| v.as_str()) {
                    Some(s) if !s.is_empty() => s.to_string(),
                    _ => continue,
                };
                let e = shot_agg.entry(subject).or_insert((0.0, 0, 0, 0, 0.0));
                e.4 += ps.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0);
                if let Some(dmgs) = ps.get("damage").and_then(|d| d.as_array()) {
                    for d in dmgs {
                        e.0 += d.get("damage").and_then(|v| v.as_f64()).unwrap_or(0.0);
                        e.1 += d.get("headshots").and_then(|v| v.as_i64()).unwrap_or(0);
                        e.2 += d.get("bodyshots").and_then(|v| v.as_i64()).unwrap_or(0);
                        e.3 += d.get("legshots").and_then(|v| v.as_i64()).unwrap_or(0);
                    }
                }
            }
        }
    }

    for p in players {
        let puuid = match p.get("subject").and_then(|v| v.as_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        let team_id = p.get("teamId").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let won = !winning_team.is_empty() && team_id == winning_team;
        let stats = p.get("stats");
        let kills = stats.and_then(|s| s.get("kills")).and_then(|v| v.as_f64()).unwrap_or(0.0);
        let deaths = stats.and_then(|s| s.get("deaths")).and_then(|v| v.as_f64()).unwrap_or(0.0);
        let assists = stats.and_then(|s| s.get("assists")).and_then(|v| v.as_f64()).unwrap_or(0.0);
        // Score, damage, and shots all come from roundResults aggregation above —
        // players[i].stats.score is a different (smaller) metric, not combat score.
        let agg = shot_agg.get(&puuid).copied().unwrap_or((0.0, 0, 0, 0, 0.0));
        let score = if agg.4 > 0.0 {
            agg.4
        } else {
            stats.and_then(|s| s.get("score")).and_then(|v| v.as_f64()).unwrap_or(0.0)
        };
        let damage = if agg.0 > 0.0 {
            agg.0
        } else {
            p.get("roundDamage").and_then(|d| d.as_array())
                .map(|arr| arr.iter()
                    .map(|r| r.get("damage").and_then(|v| v.as_f64()).unwrap_or(0.0))
                    .sum::<f64>())
                .unwrap_or(0.0)
        };
        let hs = agg.1 as i32;
        let bs = agg.2 as i32;
        let ls = agg.3 as i32;
        let party_id = p.get("partyId").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let agent = p.get("characterId").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let rounds = stats.and_then(|s| s.get("roundsPlayed")).and_then(|v| v.as_f64()).unwrap_or(0.0);
        out.insert(puuid, serde_json::json!({
            "kills": kills, "deaths": deaths, "assists": assists,
            "score": score, "damage": damage, "rounds": rounds,
            "headshots": hs, "bodyshots": bs, "legshots": ls,
            "won": won, "party": party_id, "agent": agent,
        }));
    }
    out
}

pub fn aggregate_stats(rows: &[Option<serde_json::Value>]) -> Option<PlayerStats> {
    let valid: Vec<&serde_json::Value> = rows.iter().filter_map(|r| r.as_ref()).collect();
    if valid.is_empty() { return None; }
    let n = valid.len() as f64;

    let kills: f64 = valid.iter().map(|r| r["kills"].as_f64().unwrap_or(0.0)).sum();
    let deaths: f64 = valid.iter().map(|r| r["deaths"].as_f64().unwrap_or(0.0)).sum();
    let assists: f64 = valid.iter().map(|r| r["assists"].as_f64().unwrap_or(0.0)).sum();
    let score: f64 = valid.iter().map(|r| r["score"].as_f64().unwrap_or(0.0)).sum();
    let damage: f64 = valid.iter().map(|r| r["damage"].as_f64().unwrap_or(0.0)).sum();
    let hs: f64 = valid.iter().map(|r| r["headshots"].as_f64().unwrap_or(0.0)).sum();
    let bs: f64 = valid.iter().map(|r| r["bodyshots"].as_f64().unwrap_or(0.0)).sum();
    let ls: f64 = valid.iter().map(|r| r["legshots"].as_f64().unwrap_or(0.0)).sum();
    let wins: f64 = valid.iter().map(|r| if r["won"].as_bool().unwrap_or(false) { 1.0 } else { 0.0 }).sum();
    // Sum actual rounds played; fall back to ~24/game if the field is missing.
    let rounds: f64 = valid.iter().map(|r| r["rounds"].as_f64().unwrap_or(0.0)).sum();
    let total_rounds = if rounds > 0.0 { rounds } else { n * 24.0 };

    let kda = if deaths > 0.0 { (kills + assists) / deaths } else { kills + assists };
    let acs = score / total_rounds;
    let adr = damage / total_rounds;
    let total_shots = hs + bs + ls;
    let hs_pct = if total_shots > 0.0 { (hs / total_shots * 100.0).round() as i32 } else { 0 };
    let winrate = ((wins / n) * 100.0).round() as i32;

    Some(PlayerStats {
        kda: Some((kda * 100.0).round() / 100.0),
        acs: Some(acs.round() as i32),
        adr: Some(adr.round() as i32),
        hs: Some(hs_pct),
        winrate: Some(winrate),
        games: valid.len() as i32,
        avg_k: (kills / n * 10.0).round() / 10.0,
        avg_d: (deaths / n * 10.0).round() / 10.0,
        avg_a: (assists / n * 10.0).round() / 10.0,
    })
}

pub fn extract_own_game(data: &Value, puuid: &str) -> Option<serde_json::Value> {
    let stats = extract_all_match_stats(data);
    stats.get(puuid).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_rank_empty_payload() {
        let r = parse_rank_from_updates(&json!({}), "act1");
        assert_eq!(r.tier, 0);
    }

    #[test]
    fn parse_rank_extracts_tier() {
        let payload = json!({
            "Matches": [{
                "MatchID": "abc", "SeasonID": "act1",
                "TierAfterUpdate": 21, "RankedRatingAfterUpdate": 65,
                "RankedRatingBeforeUpdate": 45,
                "CompetitiveMovement": "MOVEMENT_UP"
            }]
        });
        let r = parse_rank_from_updates(&payload, "act1");
        assert_eq!(r.tier, 21);
        assert_eq!(r.rr, 65);
    }

    #[test]
    fn wins_counted_from_ranked_rating_earned() {
        // Two games in the current act: one win (+18), one loss (-15); plus a win
        // in another season that must NOT be counted.
        let payload = json!({
            "Matches": [
                { "MatchID": "m1", "SeasonID": "act-now",
                  "TierAfterUpdate": 12, "RankedRatingAfterUpdate": 30,
                  "RankedRatingEarned": 18 },
                { "MatchID": "m2", "SeasonID": "act-now",
                  "TierAfterUpdate": 12, "RankedRatingAfterUpdate": 12,
                  "RankedRatingEarned": -15 },
                { "MatchID": "m3", "SeasonID": "act-old",
                  "TierAfterUpdate": 11, "RankedRatingAfterUpdate": 80,
                  "RankedRatingEarned": 20 },
            ]
        });
        let r = parse_rank_from_updates(&payload, "act-now");
        assert_eq!(r.wins, 1, "only the current-act win should count");
        // Current standing = most-recent match (m1).
        assert_eq!(r.tier, 12);
        assert_eq!(r.rr, 30);
    }

    #[test]
    fn wins_win_on_tier_promotion_boundary() {
        // A promotion win: RR drops (98 -> 12 in next tier) but RankedRatingEarned
        // is positive. after-before would wrongly read as a loss.
        let payload = json!({
            "Matches": [{ "MatchID": "m1", "SeasonID": "s",
                "TierAfterUpdate": 13, "RankedRatingBeforeUpdate": 98,
                "RankedRatingAfterUpdate": 12, "RankedRatingEarned": 14 }]
        });
        let r = parse_rank_from_updates(&payload, "s");
        assert_eq!(r.wins, 1);
    }

    #[test]
    fn parse_competitive_updates_extracts_rows() {
        let payload = json!({
            "Matches": [{
                "MatchID": "abc123",
                "TierAfterUpdate": 18,
                "RankedRatingAfterUpdate": 40,
                "RankedRatingBeforeUpdate": 15,
                "MapID": "/Game/Maps/Ascent",
                "MatchStartTime": 1700000000000u64
            }]
        });
        let rows = parse_competitive_updates(&payload);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].match_id, "abc123");
        assert_eq!(rows[0].rr_change, 25);
    }

    #[test]
    fn parse_name_service_extracts_names() {
        let data = vec![json!({
            "Subject": "puuid-1",
            "GameName": "TenZ",
            "TagLine": "0505"
        })];
        let names = parse_name_service(&data);
        assert_eq!(names.len(), 1);
        assert_eq!(names[0].0, "puuid-1");
        assert_eq!(names[0].1, "TenZ#0505");
    }

    #[test]
    fn aggregate_stats_basic() {
        let rows = vec![
            Some(json!({ "kills": 20.0, "deaths": 15.0, "assists": 5.0,
                         "score": 5000.0, "damage": 3000.0,
                         "headshots": 10, "bodyshots": 30, "legshots": 5, "won": true })),
            Some(json!({ "kills": 15.0, "deaths": 18.0, "assists": 8.0,
                         "score": 4200.0, "damage": 2800.0,
                         "headshots": 8, "bodyshots": 25, "legshots": 4, "won": false })),
        ];
        let s = aggregate_stats(&rows).expect("should have stats");
        assert!(s.kda.unwrap() > 0.0);
        assert!(s.acs.unwrap() > 0);
        assert_eq!(s.winrate, Some(50));
    }
}
