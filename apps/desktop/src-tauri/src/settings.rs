//! User settings and scan-location discovery.

use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqlitePool;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub onboarded: bool,
    /// Drives and folders the user chose. Searched for launcher libraries, never crawled in full.
    pub scan_locations: Vec<String>,
    /// Load Steam cover art from Steam's image servers, which can see which games' art is requested.
    /// Asked during onboarding; changeable in Settings.
    pub cover_art: bool,
    /// The user's own Steam Web API key, for listing owned Steam games. Empty = off.
    // ponytail: stored in the local settings row in plain text; move to the OS keychain if the threat model needs it.
    pub steam_api_key: String,
    /// SteamID64 to list owned games for. Empty = the account signed in to Steam on this machine.
    pub steam_id: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { onboarded: false, scan_locations: vec![], cover_art: true, steam_api_key: String::new(), steam_id: String::new() }
    }
}

pub async fn load(db: &SqlitePool) -> Result<Settings, String> {
    let row: Option<(String,)> = sqlx::query_as("SELECT value FROM settings WHERE key = 'app'")
        .fetch_optional(db)
        .await
        .map_err(|e| e.to_string())?;
    // Missing fields default via serde. A row that fails to parse is an error, not silent defaults:
    // scanning with an empty location list would treat those libraries as deselected.
    let parsed = row.map(|(v,)| serde_json::from_str(&v)).transpose();
    Ok(parsed.map_err(|e| format!("Settings are unreadable: {e}"))?.unwrap_or_default())
}

pub async fn save(db: &SqlitePool, mut s: Settings) -> Result<Settings, String> {
    // Paths come from the native picker. Not trimmed: leading/trailing spaces are valid in paths.
    for loc in &s.scan_locations {
        if !Path::new(loc).is_absolute() {
            return Err(format!("Scan location must be an absolute path: {loc:?}"));
        }
    }
    s.steam_api_key = s.steam_api_key.trim().to_string();
    s.steam_id = s.steam_id.trim().to_string();
    // Both end up in a URL; Steam keys are 32 hex characters.
    if !s.steam_api_key.is_empty() && !(s.steam_api_key.len() == 32 && s.steam_api_key.bytes().all(|b| b.is_ascii_hexdigit())) {
        return Err("A Steam Web API key is 32 letters and digits (0-9, A-F)".into());
    }
    if !s.steam_id.is_empty() && !crate::steam::is_steam_id(&s.steam_id) {
        return Err("A SteamID64 is 17 digits".into());
    }
    s.scan_locations.sort();
    s.scan_locations.dedup();
    let json = serde_json::to_string(&s).map_err(|e| e.to_string())?;
    sqlx::query("INSERT INTO settings (key, value) VALUES ('app', ?) ON CONFLICT (key) DO UPDATE SET value = excluded.value")
        .bind(json)
        .execute(db)
        .await
        .map_err(|e| e.to_string())?;
    Ok(s)
}

/// Drives and mount points to offer during onboarding.
// ponytail: common mount roots only; add a udisks/GetLogicalDrives query if users report missing drives.
pub fn list_drives() -> Vec<String> {
    let mut out: Vec<PathBuf> = Vec::new();
    if cfg!(windows) {
        // Mapped network drives are skipped: probing a disconnected one stalls the picker for seconds.
        let mapped = mapped_network_drives();
        out.extend(('A'..='Z').filter(|c| !mapped.contains(c)).map(|c| PathBuf::from(format!("{c}:\\"))));
    } else {
        out.extend(std::env::home_dir());
        let user = std::env::var("USER").unwrap_or_default();
        let bases = if cfg!(target_os = "macos") {
            vec![PathBuf::from("/Volumes")]
        } else {
            vec!["/mnt".into(), Path::new("/media").join(&user), Path::new("/run/media").join(&user)]
        };
        for base in bases {
            if let Ok(entries) = std::fs::read_dir(base) {
                out.extend(entries.flatten().map(|e| e.path()));
            }
        }
    }
    out.into_iter().filter(|p| p.is_dir()).map(|p| p.to_string_lossy().into_owned()).collect()
}

/// Drive letters of persistent mapped network drives (`HKCU\Network\<letter>`).
// ponytail: misses non-persistent `net use` mappings; switch to GetDriveTypeW if those matter.
#[cfg(windows)]
fn mapped_network_drives() -> Vec<char> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;
    let Ok(network) = RegKey::predef(HKEY_CURRENT_USER).open_subkey("Network") else { return vec![] };
    network.enum_keys().flatten().filter_map(|k| k.chars().next()).map(|c| c.to_ascii_uppercase()).collect()
}

#[cfg(not(windows))]
fn mapped_network_drives() -> Vec<char> {
    vec![]
}
