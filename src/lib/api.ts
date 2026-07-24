import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

// ── Types ──────────────────────────────────────────────────────────────────

export interface RankInfo {
  tier: number;
  tier_name: string;
  rr: number;
  tier_color: string;
  tier_icon: string;
}

export interface PlayerStats {
  kda: number | null;
  acs: number | null;
  adr: number | null;
  hs: number | null;
  winrate: number | null;
  games: number;
  avg_k: number;
  avg_d: number;
  avg_a: number;
}

export interface MatchPlayer {
  puuid: string;
  team: string;
  name: string;
  incognito: boolean;
  agent_id: string;
  agent_name: string;
  agent_icon: string;
  agent_color: string;
  account_level: number;
  hide_level: boolean;
  current: RankInfo;
  peak: RankInfo;
  wins: number;
  is_self: boolean;
  pending: boolean;
  stats: PlayerStats | null;
  stats_pending: boolean;
  party_group: number;
  party_confirmed: boolean;
}

export interface AppState {
  connected: boolean;
  phase: "offline" | "menus" | "pregame" | "ingame";
  map: string;
  map_image: string;
  players: MatchPlayer[];
  self_name: string;
  error: string;
  updated: number;
}

export interface RRHistoryPoint {
  match_id: string;
  map_name: string;
  tier: number;
  tier_name: string;
  rr_after: number;
  rr_change: number;
  elo: number;
  date_ms: number;
  agent: string;
  kills: number;
  deaths: number;
  assists: number;
  hs: number;
}

export interface ProfileDeep {
  games_total: number;
  games_act: number;
  lifetime: PlayerStats | null;
  act: PlayerStats | null;
  oldest_ms: number;
  newest_ms: number;
  scanned: number;
}

export interface Profile {
  puuid: string;
  name: string;
  current: RankInfo;
  peak: RankInfo;
  wins: number;
  history: RRHistoryPoint[];
  deep: ProfileDeep | null;
}

export interface AppSettings {
  region_override: string;
  poll_seconds: number;
  has_key: boolean;
  key_hint: string;
  profile_pull_target: number;
  pull_source: "riot" | "henrik";
  always_on_top: boolean;
  henrik_rate_limit: number;
  live_use_henrik: boolean;
  version: string;
}

export interface PullStatus {
  running: boolean;
  added: number;
  target: number;
  want_max: boolean;
  bucket_full_in: number;
  done: boolean;
  error: string;
  total_games: number;
  history_seen: number;
  history_end: boolean;
}

export interface LookupResult {
  account?: { name: string; tag: string; region: string; level: number };
  mmr?: {
    current_tier_name: string;
    current_rr: number;
    peak_tier_name: string;
    peak_season: string;
  };
  matches?: Array<{
    agent: string;
    map: string;
    kills: number;
    deaths: number;
    assists: number;
    kd: string;
    hs: number;
    score: number;
    rounds: number;
    won: boolean | null;
  }>;
  error?: string;
}

// ── Commands ───────────────────────────────────────────────────────────────

export const api = {
  getState: (): Promise<AppState> => invoke("get_state"),
  refreshNow: (): Promise<void> => invoke("refresh_now"),

  refreshProfile: (): Promise<Profile & { error?: string }> =>
    invoke("refresh_profile"),
  profilePullStart: (count: number, wantMax: boolean): Promise<{ ok: boolean; error?: string }> =>
    invoke("profile_pull_start", { count, wantMax }),
  profilePullStatus: (): Promise<PullStatus> => invoke("profile_pull_status"),
  profilePullCancel: (): Promise<void> => invoke("profile_pull_cancel"),
  profileStats: (): Promise<ProfileDeep & { error?: string }> =>
    invoke("profile_stats"),
  profileDeleteOldest: (n: number): Promise<{ removed: number; games_total: number }> =>
    invoke("profile_delete_oldest", { n }),

  lookup: (riotId: string): Promise<LookupResult> => invoke("lookup", { riotId }),

  getSettings: (): Promise<AppSettings> => invoke("get_settings"),
  saveSettings: (data: Partial<AppSettings & { henrik_key?: string }>): Promise<AppSettings> =>
    invoke("save_settings", { data }),

  setAlwaysOnTop: (enabled: boolean): Promise<void> =>
    invoke("set_always_on_top", { enabled }),
};

// ── Event listener ─────────────────────────────────────────────────────────

export function onStateUpdate(cb: (state: AppState) => void) {
  return listen<AppState>("state-updated", (e) => cb(e.payload));
}
