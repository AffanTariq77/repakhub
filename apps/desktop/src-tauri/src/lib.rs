mod steam;
mod vdf;

use serde::Serialize;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool};
use std::path::PathBuf;
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

/// Upsert everything a Steam scan found and drop Steam rows the scan no longer sees.
// ponytail: a game on an unplugged drive is removed; add a 'missing' state when external drives matter.
async fn save_steam_scan(db: &SqlitePool, games: &[steam::SteamGame], now_ms: i64) -> Result<(), sqlx::Error> {
    let mut tx = db.begin().await?;
    for g in games {
        sqlx::query(
            "INSERT INTO installations
               (source, source_game_id, name, path, installed_version, version_source,
                update_available, size_bytes, last_updated, scanned_at)
             VALUES ('steam', ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (source, source_game_id) DO UPDATE SET
               name = excluded.name, path = excluded.path,
               installed_version = excluded.installed_version, version_source = excluded.version_source,
               update_available = excluded.update_available, size_bytes = excluded.size_bytes,
               last_updated = excluded.last_updated, scanned_at = excluded.scanned_at",
        )
        .bind(&g.app_id)
        .bind(&g.name)
        .bind(g.path.to_string_lossy())
        .bind(&g.build_id)
        .bind(g.build_id.as_ref().map(|_| "steam_buildid"))
        .bind(g.update_required)
        .bind(g.size_bytes)
        .bind(g.last_updated)
        .bind(now_ms)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query("DELETE FROM installations WHERE source = 'steam' AND scanned_at < ?")
        .bind(now_ms)
        .execute(&mut *tx)
        .await?;
    tx.commit().await
}

#[tauri::command]
async fn list_games(db: State<'_, SqlitePool>) -> Result<Vec<Installation>, String> {
    all_installations(&db).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn scan_library(db: State<'_, SqlitePool>) -> Result<Vec<Installation>, String> {
    let games = tauri::async_runtime::spawn_blocking(steam::scan).await.map_err(|e| e.to_string())?;
    let now_ms = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
    save_steam_scan(&db, &games, now_ms).await.map_err(|e| e.to_string())?;
    all_installations(&db).await.map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let pool = tauri::async_runtime::block_on(open_db(app.path().app_data_dir()?))?;
            app.manage(pool);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![list_games, scan_library])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    fn game(id: &str, build: Option<&str>) -> steam::SteamGame {
        steam::SteamGame {
            app_id: id.into(),
            name: format!("Game {id}"),
            path: PathBuf::from("/games").join(id),
            build_id: build.map(String::from),
            size_bytes: None,
            update_required: false,
            last_updated: None,
        }
    }

    #[test]
    fn rescan_updates_dedupes_and_removes() {
        tauri::async_runtime::block_on(async {
            // One connection: each in-memory SQLite connection is its own database.
            let db = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
            sqlx::migrate!().run(&db).await.unwrap();

            save_steam_scan(&db, &[game("1", Some("100")), game("2", None)], 1).await.unwrap();
            save_steam_scan(&db, &[game("1", Some("101")), game("1", Some("101"))], 2).await.unwrap();

            let rows = all_installations(&db).await.unwrap();
            assert_eq!(rows.len(), 1, "duplicate collapsed, uninstalled game removed");
            assert_eq!(rows[0].installed_version.as_deref(), Some("101"));
            assert_eq!(rows[0].version_source.as_deref(), Some("steam_buildid"));
        });
    }
}
