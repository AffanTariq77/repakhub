mod discovery;
mod gog;
mod settings;
mod steam;
mod vdf;

use settings::Settings;

use serde::Serialize;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{Manager, State};

#[derive(Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
struct Installation {
    id: i64,
    source: String,
    source_game_id: String,
    name: String,
    path: String,
    installed_version: Option<String>,
    version_source: Option<String>,
    update_available: Option<bool>,
    size_bytes: Option<i64>,
    last_updated: Option<i64>,
    scanned_at: i64,
}

async fn open_db(dir: PathBuf) -> Result<SqlitePool, Box<dyn std::error::Error>> {
    std::fs::create_dir_all(&dir)?;
    let opts = SqliteConnectOptions::new().filename(dir.join("repakhub.db")).create_if_missing(true);
    let pool = SqlitePool::connect_with(opts).await?;
    sqlx::migrate!().run(&pool).await?;
    Ok(pool)
}

async fn all_installations(db: &SqlitePool) -> Result<Vec<Installation>, sqlx::Error> {
    sqlx::query_as("SELECT * FROM installations ORDER BY name COLLATE NOCASE").fetch_all(db).await
}

/// Upsert everything one source's scan found, then reconcile that source's rows it did not see:
/// - inside a library that was read: uninstalled, delete;
/// - outside, but the folder still exists: its library is no longer configured, delete;
/// - outside, and the folder is missing: drive offline or unreadable, keep.
// ponytail: offline rows are kept silently; show them as 'offline' in the UI when external drives matter.
async fn save_scan(db: &SqlitePool, source: &str, scan: &discovery::ScanResult, now_ms: i64) -> Result<(), sqlx::Error> {
    let mut tx = db.begin().await?;
    for g in &scan.games {
        sqlx::query(
            "INSERT INTO installations
               (source, source_game_id, name, path, installed_version, version_source,
                update_available, size_bytes, last_updated, scanned_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (source, source_game_id) DO UPDATE SET
               name = excluded.name, path = excluded.path,
               installed_version = excluded.installed_version, version_source = excluded.version_source,
               update_available = excluded.update_available, size_bytes = excluded.size_bytes,
               last_updated = excluded.last_updated, scanned_at = excluded.scanned_at",
        )
        .bind(source)
        .bind(&g.source_game_id)
        .bind(&g.name)
        .bind(g.path.to_string_lossy())
        .bind(&g.version)
        .bind(g.version_source)
        .bind(g.update_available)
        .bind(g.size_bytes)
        .bind(g.last_updated)
        .bind(now_ms)
        .execute(&mut *tx)
        .await?;
    }
    let stale: Vec<(i64, String)> =
        sqlx::query_as("SELECT id, path FROM installations WHERE source = ? AND scanned_at < ?")
            .bind(source)
            .bind(now_ms)
            .fetch_all(&mut *tx)
            .await?;
    for (id, path) in stale {
        let path = Path::new(&path);
        if scan.libraries.iter().any(|l| path.starts_with(l)) || path.exists() {
            sqlx::query("DELETE FROM installations WHERE id = ?").bind(id).execute(&mut *tx).await?;
        }
    }
    tx.commit().await
}

/// Held for the whole scan so overlapping scans cannot commit stale results over newer ones.
#[derive(Default)]
struct ScanLock(tauri::async_runtime::Mutex<()>);

#[tauri::command]
async fn list_games(db: State<'_, SqlitePool>) -> Result<Vec<Installation>, String> {
    all_installations(&db).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn scan_library(db: State<'_, SqlitePool>, lock: State<'_, ScanLock>) -> Result<Vec<Installation>, String> {
    let _guard = lock.0.lock().await;
    // Settings are read inside the lock, so a scan queued behind another sees the latest locations.
    let locations: Vec<PathBuf> = settings::load(&db).await?.scan_locations.into_iter().map(PathBuf::from).collect();
    let scans = tauri::async_runtime::spawn_blocking(move || [("steam", steam::scan(&locations)), ("gog", gog::scan(&locations))])
        .await
        .map_err(|e| e.to_string())?;
    let now_ms = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
    for (source, scan) in &scans {
        save_scan(&db, source, scan, now_ms).await.map_err(|e| e.to_string())?;
    }
    all_installations(&db).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_settings(db: State<'_, SqlitePool>) -> Result<Settings, String> {
    settings::load(&db).await
}

#[tauri::command]
async fn save_settings(
    db: State<'_, SqlitePool>,
    lock: State<'_, ScanLock>,
    settings: Settings,
) -> Result<Settings, String> {
    // Wait for a running scan, so it never reconciles against locations that changed mid-scan.
    let _guard = lock.0.lock().await;
    settings::save(&db, settings).await
}

#[tauri::command]
fn list_drives() -> Vec<String> {
    settings::list_drives()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let pool = tauri::async_runtime::block_on(open_db(app.path().app_data_dir()?))?;
            app.manage(pool);
            app.manage(ScanLock::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![list_games, scan_library, get_settings, save_settings, list_drives])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    fn game(id: &str, build: Option<&str>) -> discovery::DiscoveredGame {
        discovery::DiscoveredGame {
            source_game_id: id.into(),
            name: format!("Game {id}"),
            path: PathBuf::from("/games").join(id),
            version: build.map(String::from),
            version_source: build.map(|_| "steam_buildid"),
            update_available: Some(false),
            size_bytes: None,
            last_updated: None,
        }
    }

    async fn test_db() -> SqlitePool {
        // One connection: each in-memory SQLite connection is its own database.
        let db = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!().run(&db).await.unwrap();
        db
    }

    #[test]
    fn settings_default_roundtrip_and_validation() {
        tauri::async_runtime::block_on(async {
            let db = test_db().await;
            assert_eq!(settings::load(&db).await.unwrap(), Settings::default());

            let abs = std::env::temp_dir().to_string_lossy().into_owned();
            let input = Settings { onboarded: true, scan_locations: vec![abs.clone(), abs.clone()] };
            settings::save(&db, input).await.unwrap();
            let loaded = settings::load(&db).await.unwrap();
            assert!(loaded.onboarded);
            assert_eq!(loaded.scan_locations, vec![abs], "deduplicated");

            let relative = Settings { onboarded: true, scan_locations: vec!["games".into()] };
            assert!(settings::save(&db, relative).await.is_err());

            sqlx::query(r#"UPDATE settings SET value = '{"onboarded": "yes"}'"#).execute(&db).await.unwrap();
            assert!(settings::load(&db).await.is_err(), "unreadable settings are an error, not silent defaults");
        });
    }

    #[test]
    fn rescan_updates_dedupes_and_removes() {
        tauri::async_runtime::block_on(async {
            let db = test_db().await;

            let scan = |libraries: &[&str], games| discovery::ScanResult {
                games,
                libraries: libraries.iter().map(PathBuf::from).collect(),
            };
            // Game 3 lives outside /games in a folder that exists: its library is "no longer configured".
            let mut deselected = game("3", None);
            deselected.path = std::env::temp_dir();

            save_scan(&db, "steam", &scan(&["/games"], vec![game("1", Some("100")), game("2", None), deselected]), 1)
                .await
                .unwrap();
            save_scan(&db, "steam", &scan(&["/games"], vec![game("1", Some("101")), game("1", Some("101"))]), 2)
                .await
                .unwrap();

            let rows = all_installations(&db).await.unwrap();
            assert_eq!(rows.len(), 1, "duplicate collapsed, uninstalled (2) and deselected (3) removed");
            assert_eq!(rows[0].installed_version.as_deref(), Some("101"));
            assert_eq!(rows[0].version_source.as_deref(), Some("steam_buildid"));

            // /games was not readable this time and /games/1 does not exist: drive offline, keep the row.
            save_scan(&db, "steam", &scan(&[], vec![]), 3).await.unwrap();
            assert_eq!(all_installations(&db).await.unwrap().len(), 1, "offline library is not wiped");

            // Same id from another source is a separate row, and reconciling GOG never touches Steam rows.
            save_scan(&db, "gog", &scan(&["/games"], vec![game("1", None)]), 4).await.unwrap();
            save_scan(&db, "gog", &scan(&["/games"], vec![]), 5).await.unwrap();
            let rows = all_installations(&db).await.unwrap();
            assert_eq!(rows.iter().map(|r| r.source.as_str()).collect::<Vec<_>>(), vec!["steam"]);
        });
    }
}
