use base64::{engine::general_purpose::STANDARD, Engine};
use regex::Regex;
use std::path::PathBuf;
use thiserror::Error;

use super::lockfile::Lockfile;

#[derive(Debug, Error)]
pub enum LocalError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Entitlements endpoint returned {0}")]
    Status(u16),
    #[error("Missing field in entitlements response")]
    MissingField,
}

#[derive(Debug, Clone)]
pub struct Credentials {
    pub access_token: String,
    pub entitlements_jwt: String,
    pub puuid: String,
    pub region: String,
    pub shard: String,
    pub client_version: String,
}

pub async fn get_credentials(
    client: &reqwest::Client,
    lockfile: &Lockfile,
    version: &str,
) -> Result<Credentials, LocalError> {
    let raw = format!("riot:{}", lockfile.password);
    let auth = "Basic ".to_string() + &STANDARD.encode(raw.as_bytes());
    let url = format!("{}/entitlements/v1/token", lockfile.base_url());
    let resp = client
        .get(&url)
        .header("Authorization", auth)
        .send()
        .await?;
    let status = resp.status().as_u16();
    if status != 200 {
        return Err(LocalError::Status(status));
    }
    let body: serde_json::Value = resp.json().await?;
    let access_token = body["accessToken"].as_str().ok_or(LocalError::MissingField)?.to_string();
    let entitlements_jwt = body["token"].as_str().ok_or(LocalError::MissingField)?.to_string();
    let puuid = body["subject"].as_str().ok_or(LocalError::MissingField)?.to_string();
    let (region, shard) = detect_region().unwrap_or_else(|| ("na".into(), "na".into()));
    Ok(Credentials {
        access_token,
        entitlements_jwt,
        puuid,
        region,
        shard,
        client_version: version.to_string(),
    })
}

// ── Region detection from ShooterGame.log ─────────────────────────────────

pub fn detect_region() -> Option<(String, String)> {
    let log_path = shooter_game_log_path()?;
    let text = std::fs::read_to_string(log_path).ok()?;
    parse_region_from_log(&text)
}

pub fn parse_region_from_log(text: &str) -> Option<(String, String)> {
    // glz URL: glz-{region}-1.{shard}.a.pvp.net
    let glz_re = Regex::new(r"glz-([a-z0-9]+)-1\.([a-z0-9]+)\.a\.pvp\.net").ok()?;
    if let Some(caps) = glz_re.captures(text) {
        let region = caps[1].to_lowercase();
        let shard = caps[2].to_lowercase();
        return Some((region, shard));
    }
    // pd URL: pd.{shard}.a.pvp.net
    let pd_re = Regex::new(r"\bpd\.([a-z0-9]+)\.a\.pvp\.net").ok()?;
    if let Some(caps) = pd_re.captures(text) {
        let shard = caps[1].to_lowercase();
        return Some((shard.clone(), shard));
    }
    None
}

fn shooter_game_log_path() -> Option<PathBuf> {
    let local = std::env::var("LOCALAPPDATA").ok()?;
    Some(
        PathBuf::from(local)
            .join("VALORANT")
            .join("Saved")
            .join("Logs")
            .join("ShooterGame.log"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_glz_region() {
        let log = "https://glz-na-1.na.a.pvp.net/something";
        assert_eq!(parse_region_from_log(log), Some(("na".into(), "na".into())));
    }

    #[test]
    fn parses_eu_region() {
        let log = "glz-eu-1.eu.a.pvp.net/match/123";
        assert_eq!(parse_region_from_log(log), Some(("eu".into(), "eu".into())));
    }

    #[test]
    fn parses_pd_fallback() {
        let log = "https://pd.na.a.pvp.net/";
        assert_eq!(parse_region_from_log(log), Some(("na".into(), "na".into())));
    }

    #[test]
    fn returns_none_on_missing() {
        assert_eq!(parse_region_from_log("no urls here"), None);
    }
}
