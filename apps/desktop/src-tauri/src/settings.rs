//! User settings and scan-location discovery.

use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqlitePool;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Default, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub onboarded: bool,
    /// Drives and folders the user chose. Searched for launcher libraries, never crawled in full.
    pub scan_locations: Vec<String>,
}

pub async fn load(db: &SqlitePool) -> Result<Settings, String> {
    let row: Option<(String,)> = sqlx::query_as("SELECT value FROM settings WHERE key = 'app'")
        .fetch_optional(db)
        .await
        .map_err(|e| e.to_string())?;
    // A row from a newer/older build that no longer parses falls back to defaults rather than bricking startup.
    Ok(row.and_then(|(v,)| serde_json::from_str(&v).ok()).unwrap_or_default())
}

pub async fn save(db: &SqlitePool, mut s: Settings) -> Result<Settings, String> {
    for loc in &mut s.scan_locations {
        *loc = loc.trim().to_string();
        if !Path::new(loc).is_absolute() {
            return Err(format!("Scan location must be an absolute path: {loc:?}"));
        }
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
        out.extend(('A'..='Z').map(|c| PathBuf::from(format!("{c}:\\"))));
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
