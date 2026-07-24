use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LockfileError {
    #[error("VALORANT is not running (lockfile not found)")]
    NotFound,
    #[error("Failed to read lockfile: {0}")]
    Io(#[from] std::io::Error),
    #[error("Lockfile format unrecognized")]
    BadFormat,
}

#[derive(Debug, Clone)]
pub struct Lockfile {
    pub pid: u32,
    pub port: u16,
    pub password: String,
    pub protocol: String,
}

impl Lockfile {
    pub fn base_url(&self) -> String {
        format!("{}://127.0.0.1:{}", self.protocol, self.port)
    }
}

pub fn lockfile_path() -> PathBuf {
    let local = std::env::var("LOCALAPPDATA").unwrap_or_default();
    PathBuf::from(local)
        .join("Riot Games")
        .join("Riot Client")
        .join("Config")
        .join("lockfile")
}

pub fn read_lockfile() -> Result<Lockfile, LockfileError> {
    let path = lockfile_path();
    if !path.exists() {
        return Err(LockfileError::NotFound);
    }
    let text = std::fs::read_to_string(path)?;
    // Format: name:pid:port:password:protocol
    let parts: Vec<&str> = text.trim().splitn(5, ':').collect();
    if parts.len() < 5 {
        return Err(LockfileError::BadFormat);
    }
    Ok(Lockfile {
        pid: parts[1].parse().unwrap_or(0),
        port: parts[2].parse().map_err(|_| LockfileError::BadFormat)?,
        password: parts[3].to_string(),
        protocol: parts[4].to_string(),
    })
}
