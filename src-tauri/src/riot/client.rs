/// Riot pd/glz HTTP client with 429 backoff and rate limiting.
/// Mirrors backend/riot_api.py RiotClient.
use reqwest::Client;
use serde_json::Value;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use thiserror::Error;

use super::local::Credentials;
use super::ratelimit::RateLimiter;

/// Shared, cloneable rate limiter so every RiotClient in a poll cycle draws
/// from one budget (ranks + stats + profile pulls together stay under Riot's ceiling).
pub type SharedLimiter = Arc<Mutex<RateLimiter>>;

pub fn new_limiter() -> SharedLimiter {
    Arc::new(Mutex::new(RateLimiter::default()))
}

#[derive(Debug, Error)]
pub enum RiotApiError {
    #[error("HTTP {0}")]
    Status(u16),
    #[error("Rate limited (429)")]
    RateLimited,
    #[error("Not found (404)")]
    NotFound,
    #[error("Request error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("JSON error: {0}")]
    Json(String),
}

pub struct RiotClient {
    http: Client,
    creds: Credentials,
    limiter: SharedLimiter,
}

impl RiotClient {
    pub fn new(http: Client, creds: Credentials, limiter: SharedLimiter) -> Self {
        Self { http, creds, limiter }
    }

    /// Block until a rate-limit slot is free, then claim it.
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

    fn pd_url(&self, path: &str) -> String {
        format!("https://pd.{}.a.pvp.net{}", self.creds.shard, path)
    }
    fn glz_url(&self, path: &str) -> String {
        format!("https://glz-{}-1.{}.a.pvp.net{}", self.creds.region, self.creds.shard, path)
    }

    fn auth_headers(&self) -> [(&'static str, String); 4] {
        [
            ("Authorization", format!("Bearer {}", self.creds.access_token)),
            ("X-Riot-Entitlements-JWT", self.creds.entitlements_jwt.clone()),
            ("X-Riot-ClientVersion", self.creds.client_version.clone()),
            ("X-Riot-ClientPlatform", "ew0KCSJwbGF0Zm9ybVR5cGUiOiAiUEMiLA0KCSJwbGF0Zm9ybU9TIjogIldpbmRvd3MiLA0KCSJwbGF0Zm9ybU9TVmVyc2lvbiI6ICIxMC4wLjE5MDQyLjEuMjU2LjY0Yml0IiwNCgkicGxhdGZvcm1DaGlwc2V0IjogIlVua25vd24iDQp9".into()),
        ]
    }

    async fn get(&mut self, url: &str) -> Result<Value, RiotApiError> {
        let mut retries = 0u32;
        loop {
            self.gate().await;
            let mut req = self.http.get(url);
            for (k, v) in &self.auth_headers() {
                req = req.header(*k, v);
            }
            let resp = req.send().await?;
            let status = resp.status().as_u16();
            match status {
                200 => {
                    let v: Value = resp.json().await.map_err(|e| RiotApiError::Json(e.to_string()))?;
                    return Ok(v);
                }
                404 => return Err(RiotApiError::NotFound),
                429 => {
                    retries += 1;
                    if retries > 3 { return Err(RiotApiError::RateLimited); }
                    let wait = resp.headers()
                        .get("Retry-After")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|s| s.parse::<u64>().ok())
                        .unwrap_or(2);
                    tokio::time::sleep(Duration::from_secs(wait.min(30))).await;
                }
                _ => return Err(RiotApiError::Status(status)),
            }
        }
    }

    // ── Endpoints ─────────────────────────────────────────────────────────

    pub async fn competitive_updates(&mut self, puuid: &str, count: u32, start: u32) -> Result<Value, RiotApiError> {
        let url = self.pd_url(&format!(
            "/mmr/v1/players/{puuid}/competitiveupdates?queue=competitive&startIndex={start}&endIndex={}",
            start + count
        ));
        self.get(&url).await
    }

    pub async fn match_details(&mut self, match_id: &str) -> Result<Option<Value>, RiotApiError> {
        let url = self.pd_url(&format!("/match-details/v1/matches/{match_id}"));
        match self.get(&url).await {
            Ok(v) => Ok(Some(v)),
            Err(RiotApiError::NotFound) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub async fn name_service(&mut self, puuids: &[String]) -> Result<Vec<Value>, RiotApiError> {
        let url = self.pd_url("/name-service/v2/players");
        let body = serde_json::to_string(puuids).unwrap_or_default();
        self.gate().await;
        let mut req = self.http.put(&url).body(body).header("Content-Type", "application/json");
        for (k, v) in &self.auth_headers() {
            req = req.header(*k, v);
        }
        let resp = req.send().await?;
        let status = resp.status().as_u16();
        if status != 200 { return Err(RiotApiError::Status(status)); }
        let v: Vec<Value> = resp.json().await.map_err(|e| RiotApiError::Json(e.to_string()))?;
        Ok(v)
    }

    pub async fn pregame_match_id(&mut self) -> Result<Option<String>, RiotApiError> {
        let url = self.glz_url(&format!("/pregame/v1/players/{}", self.creds.puuid));
        match self.get(&url).await {
            Ok(v) => Ok(v.get("MatchID").and_then(|m| m.as_str()).map(|s| s.to_string())),
            Err(RiotApiError::NotFound | RiotApiError::Status(404)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub async fn pregame_match(&mut self, match_id: &str) -> Result<Value, RiotApiError> {
        let url = self.glz_url(&format!("/pregame/v1/matches/{match_id}"));
        self.get(&url).await
    }

    pub async fn coregame_match_id(&mut self) -> Result<Option<String>, RiotApiError> {
        let url = self.glz_url(&format!("/core-game/v1/players/{}", self.creds.puuid));
        match self.get(&url).await {
            Ok(v) => Ok(v.get("MatchID").and_then(|m| m.as_str()).map(|s| s.to_string())),
            Err(RiotApiError::NotFound | RiotApiError::Status(404)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub async fn coregame_match(&mut self, match_id: &str) -> Result<Value, RiotApiError> {
        let url = self.glz_url(&format!("/core-game/v1/matches/{match_id}"));
        self.get(&url).await
    }

    pub async fn party_id_for(&mut self, puuid: &str) -> Result<Option<String>, RiotApiError> {
        let url = self.glz_url(&format!("/parties/v1/players/{puuid}"));
        match self.get(&url).await {
            Ok(v) => Ok(v.get("CurrentPartyID").and_then(|m| m.as_str()).map(|s| s.to_string())),
            Err(RiotApiError::NotFound) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub async fn party_members(&mut self, party_id: &str) -> Result<Vec<String>, RiotApiError> {
        let url = self.glz_url(&format!("/parties/v1/parties/{party_id}"));
        let v = self.get(&url).await?;
        let members = v.get("Members").and_then(|m| m.as_array())
            .map(|arr| arr.iter()
                .filter_map(|m| m.get("Subject").and_then(|s| s.as_str()).map(|s| s.to_string()))
                .collect())
            .unwrap_or_default();
        Ok(members)
    }

    pub fn puuid(&self) -> &str { &self.creds.puuid }
    pub fn creds(&self) -> &Credentials { &self.creds }
}
