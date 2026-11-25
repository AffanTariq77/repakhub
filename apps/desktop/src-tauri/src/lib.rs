mod apply;
mod backups;
mod discovery;
mod downloads;
mod epic;
mod gog;
mod managed;
mod manifest;
mod owned;
mod pe;
mod settings;
mod steam;
mod store;
mod updates;
mod vdf;
mod version;

use settings::Settings;

use serde::Serialize;
use sqlx::sqlite::{SqliteConnectOptions, SqliteConnection, SqlitePool};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{Manager, State};

/// A stored installation plus the user's version override, as read from the database.
#[derive(sqlx::FromRow)]
struct Row {
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
    launcher: Option<String>,
    exe_version: Option<String>,
    user_version: Option<String>,
    dlc: Option<String>,
}

/// What the UI gets: `installed_version`/`version_source` are the resolved version (see `version::resolve`),
/// with every strategy's raw value alongside so conflicts can be shown.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Installation {
    id: i64,
    source: String,
    source_game_id: String,
    name: String,
    path: String,
    installed_version: Option<String>,
    version_source: Option<String>,
    version_confidence: &'static str,
    version_conflict: bool,
    launcher_version: Option<String>,
    exe_version: Option<String>,
    user_version: Option<String>,
    update_available: Option<bool>,
    size_bytes: Option<i64>,
    last_updated: Option<i64>,
    scanned_at: i64,
    launcher: Option<String>,
    dlc: Vec<discovery::Dlc>,
}

impl From<Row> for Installation {
    fn from(r: Row) -> Self {
        let v = version::resolve(
            r.installed_version.as_deref(),
            r.version_source.as_deref(),
            r.user_version.as_deref(),
            r.exe_version.as_deref(),
        );
        Installation {
            id: r.id,
            source: r.source,
            source_game_id: r.source_game_id,
            name: r.name,
            path: r.path,
            installed_version: v.version,
            version_source: v.source,
            version_confidence: v.confidence,
            version_conflict: v.conflict,
            launcher_version: r.installed_version,
            exe_version: r.exe_version,
            user_version: r.user_version,
            update_available: r.update_available,
            size_bytes: r.size_bytes,
            last_updated: r.last_updated,
            scanned_at: r.scanned_at,
            launcher: r.launcher,
            dlc: r.dlc.and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default(),
        }
    }
}

async fn open_db(dir: PathBuf) -> Result<SqlitePool, Box<dyn std::error::Error>> {
    std::fs::create_dir_all(&dir)?;
    let opts = SqliteConnectOptions::new().filename(dir.join("repakhub.db")).create_if_missing(true);
    let pool = SqlitePool::connect_with(opts).await?;
    sqlx::migrate!().run(&pool).await?;
    Ok(pool)
}

async fn all_installations(db: &SqlitePool) -> Result<Vec<Installation>, sqlx::Error> {
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT i.*, o.version AS user_version FROM installations i
         LEFT JOIN version_overrides o ON o.source = i.source AND o.source_game_id = i.source_game_id
         ORDER BY i.name COLLATE NOCASE",
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(Installation::from).collect())
}

/// Set (or with `None`/empty, clear) the version the user entered for a game.
async fn set_override(db: &SqlitePool, source: &str, source_game_id: &str, version: Option<&str>) -> Result<(), sqlx::Error> {
    match version.map(str::trim).filter(|v| !v.is_empty()) {
        Some(v) => {
            sqlx::query(
                "INSERT INTO version_overrides (source, source_game_id, version) VALUES (?, ?, ?)
                 ON CONFLICT (source, source_game_id) DO UPDATE SET version = excluded.version",
            )
            .bind(source)
            .bind(source_game_id)
            .bind(v)
            .execute(db)
            .await?
        }
        None => {
            sqlx::query("DELETE FROM version_overrides WHERE source = ? AND source_game_id = ?")
                .bind(source)
                .bind(source_game_id)
                .execute(db)
                .await?
        }
    };
    Ok(())
}

/// Upsert everything one source's scan found, then reconcile that source's rows it did not see,
/// as `scan.reconcile` says (see `discovery::Reconcile`).
// ponytail: offline rows are kept silently; show them as 'offline' in the UI when external drives matter.
async fn save_scan(
    tx: &mut SqliteConnection,
    source: &str,
    scan: &discovery::ScanResult,
    now_ms: i64,
) -> Result<(), sqlx::Error> {
    for g in &scan.games {
        sqlx::query(
            "INSERT INTO installations
               (source, source_game_id, name, path, installed_version, version_source,
                update_available, size_bytes, last_updated, scanned_at, launcher, exe_version, dlc)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (source, source_game_id) DO UPDATE SET
               name = excluded.name, path = excluded.path,
               installed_version = excluded.installed_version, version_source = excluded.version_source,
               -- A source that cannot tell (NULL) keeps what the update check found.
               update_available = COALESCE(excluded.update_available, installations.update_available),
               size_bytes = excluded.size_bytes,
               last_updated = excluded.last_updated, scanned_at = excluded.scanned_at,
               launcher = excluded.launcher, exe_version = excluded.exe_version, dlc = excluded.dlc",
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
        .bind(g.launcher)
        .bind(&g.exe_version)
        .bind(serde_json::to_string(&g.dlc).unwrap_or_else(|_| "[]".into()))
        .execute(&mut *tx)
        .await?;
    }
    if scan.reconcile == discovery::Reconcile::AddOnly {
        return Ok(());
    }
    let stale: Vec<(i64, String)> =
        sqlx::query_as("SELECT id, path FROM installations WHERE source = ? AND scanned_at < ?")
            .bind(source)
            .bind(now_ms)
            .fetch_all(&mut *tx)
            .await?;
    for (id, path) in stale {
        let path = Path::new(&path);
        let uninstalled = scan.reconcile == discovery::Reconcile::Complete
            || scan.libraries.iter().any(|l| path.starts_with(l))
            || path.exists();
        if uninstalled {
            sqlx::query("DELETE FROM installations WHERE id = ?").bind(id).execute(&mut *tx).await?;
        }
    }
    Ok(())
}

/// Save every source's results in one transaction, so a failure leaves the library exactly as it was.
/// The timestamp is forced past the newest stored one, so rows from the previous scan always count as stale.
async fn save_all(db: &SqlitePool, scans: &[(String, discovery::ScanResult)]) -> Result<(), sqlx::Error> {
    let mut tx = db.begin().await?;
    let (newest,): (i64,) = sqlx::query_as("SELECT COALESCE(MAX(scanned_at), 0) FROM installations").fetch_one(&mut *tx).await?;
    let clock = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
    let now_ms = clock.max(newest + 1);
    for (source, scan) in scans {
        save_scan(&mut tx, source, scan, now_ms).await?;
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

/// Scan every source and save the results. Used by the Scan button and after installs and updates.
async fn run_scan(db: &SqlitePool, lock: &ScanLock) -> Result<Vec<Installation>, String> {
    let _guard = lock.0.lock().await;
    // Settings are read inside the lock, so a scan queued behind another sees the latest locations.
    let locations: Vec<PathBuf> = settings::load(db).await?.scan_locations.into_iter().map(PathBuf::from).collect();
    let mut scans = tauri::async_runtime::spawn_blocking(move || {
        let mut scans = vec![
            ("steam".to_string(), steam::scan(&locations)),
            ("gog".to_string(), gog::scan(&locations)),
            ("epic".to_string(), epic::scan()),
        ];
        scans.extend(managed::scan(&locations));
        scans
    })
    .await
    .map_err(|e| e.to_string())?;

    // A managed source whose last game is gone reports nothing; reconcile it as empty so its rows go too.
    let known: Vec<(String,)> = sqlx::query_as("SELECT DISTINCT source FROM installations WHERE launcher = 'repakhub'")
        .fetch_all(db)
        .await
        .map_err(|e| e.to_string())?;
    let managed_libraries = scans.iter().find(|(_, s)| s.games.iter().any(|g| g.launcher == Some("repakhub")));
    let libraries = managed_libraries.map(|(_, s)| s.libraries.clone()).unwrap_or_else(|| vec![managed::library_dir()]);
    for (source, ) in known {
        if !scans.iter().any(|(s, _)| *s == source) {
            let empty = discovery::ScanResult { games: vec![], libraries: libraries.clone(), reconcile: discovery::Reconcile::ByLocation };
            scans.push((source, empty));
        }
    }
    save_all(db, &scans).await.map_err(|e| e.to_string())?;
    all_installations(db).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn scan_library(db: State<'_, SqlitePool>, lock: State<'_, ScanLock>) -> Result<Vec<Installation>, String> {
    run_scan(&db, &lock).await
}

#[tauri::command]
async fn set_version_override(
    db: State<'_, SqlitePool>,
    source: String,
    source_game_id: String,
    version: Option<String>,
) -> Result<Vec<Installation>, String> {
    set_override(&db, &source, &source_game_id, version.as_deref()).await.map_err(|e| e.to_string())?;
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
        // First, so a second launch focuses the running window instead of opening the same database twice.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let pool = tauri::async_runtime::block_on(open_db(app.path().app_data_dir()?))?;
            app.manage(pool);
            app.manage(ScanLock::default());
            app.manage(updates::UpdateState::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_games,
            scan_library,
            set_version_override,
            get_settings,
            save_settings,
            list_drives,
            updates::list_catalog,
            updates::check_updates,
            updates::install_game,
            updates::update_game,
            updates::pause_job,
            owned::list_owned,
            store::search_gog,
            backups::get_backups,
            backups::set_backup_paths,
            backups::create_backup,
            backups::restore_backup,
            backups::delete_backup
        ])
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
            launcher: Some("steam"),
            exe_version: None,
            dlc: vec![],
        }
    }

    /// One source, one transaction, with an explicit timestamp.
    async fn save(db: &SqlitePool, source: &str, scan: discovery::ScanResult, now_ms: i64) {
        let mut tx = db.begin().await.unwrap();
        save_scan(&mut tx, source, &scan, now_ms).await.unwrap();
        tx.commit().await.unwrap();
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
            let input = Settings { onboarded: true, scan_locations: vec![abs.clone(), abs.clone()], cover_art: false, ..Settings::default() };
            settings::save(&db, input).await.unwrap();
            let loaded = settings::load(&db).await.unwrap();
            assert!(loaded.onboarded);
            assert_eq!(loaded.scan_locations, vec![abs], "deduplicated");
            assert!(!loaded.cover_art);
            sqlx::query(r#"UPDATE settings SET value = '{"onboarded": true}'"#).execute(&db).await.unwrap();
            assert!(settings::load(&db).await.unwrap().cover_art, "older settings rows default to cover art on");

            let relative = Settings { scan_locations: vec!["games".into()], ..Settings::default() };
            assert!(settings::save(&db, relative).await.is_err());
            let steam = Settings { steam_api_key: " 0123456789ABCDEF0123456789abcdef ".into(), steam_id: "76561197960287930".into(), ..Settings::default() };
            assert_eq!(settings::save(&db, steam).await.unwrap().steam_api_key, "0123456789ABCDEF0123456789abcdef", "trimmed");
            for bad in [Settings { steam_api_key: "key&steamid=1".into(), ..Settings::default() }, Settings { steam_id: "123".into(), ..Settings::default() }] {
                assert!(settings::save(&db, bad).await.is_err());
            }

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
                reconcile: discovery::Reconcile::ByLocation,
            };
            // Game 3 lives outside /games in a folder that exists: its library is "no longer configured".
            let mut deselected = game("3", None);
            deselected.path = std::env::temp_dir();

            save(&db, "steam", scan(&["/games"], vec![game("1", Some("100")), game("2", None), deselected]), 1).await;
            save(&db, "steam", scan(&["/games"], vec![game("1", Some("101")), game("1", Some("101"))]), 2).await;

            let rows = all_installations(&db).await.unwrap();
            assert_eq!(rows.len(), 1, "duplicate collapsed, uninstalled (2) and deselected (3) removed");
            assert_eq!(rows[0].installed_version.as_deref(), Some("101"));
            assert_eq!(rows[0].version_source.as_deref(), Some("steam_buildid"));

            // /games was not readable this time and /games/1 does not exist: drive offline, keep the row.
            save(&db, "steam", scan(&[], vec![]), 3).await;
            assert_eq!(all_installations(&db).await.unwrap().len(), 1, "offline library is not wiped");

            // Same id from another source is a separate row, and reconciling GOG never touches Steam rows.
            save(&db, "gog", scan(&["/games"], vec![game("1", None)]), 4).await;
            save(&db, "gog", scan(&["/games"], vec![]), 5).await;
            let rows = all_installations(&db).await.unwrap();
            assert_eq!(rows.iter().map(|r| r.source.as_str()).collect::<Vec<_>>(), vec!["steam"]);

            // An authoritative list (Epic) drops games it no longer lists, even with the folder gone.
            let epic = |games, reconcile| discovery::ScanResult { games, libraries: vec![], reconcile };
            let mut existing = game("9", None);
            existing.path = std::env::temp_dir(); // folder exists: ByLocation would call it deselected
            save(&db, "epic", epic(vec![existing], discovery::Reconcile::Complete), 6).await;
            save(&db, "epic", epic(vec![], discovery::Reconcile::AddOnly), 7).await;
            assert_eq!(all_installations(&db).await.unwrap().len(), 2, "AddOnly never removes");
            save(&db, "epic", epic(vec![], discovery::Reconcile::Complete), 8).await;
            assert_eq!(all_installations(&db).await.unwrap().len(), 1, "uninstalled Epic game removed");

            let rows = all_installations(&db).await.unwrap();
            assert_eq!(rows[0].launcher.as_deref(), Some("steam"));

            // save_all forces the timestamp past the newest stored row, even if the clock is behind it,
            // so the previous scan's rows are always stale: game 1 sits in a library that was read, so it goes.
            sqlx::query("UPDATE installations SET scanned_at = 9999999999999").execute(&db).await.unwrap();
            save_all(&db, &[("steam".to_string(), scan(&["/games"], vec![]))]).await.unwrap();
            assert!(all_installations(&db).await.unwrap().is_empty(), "stale despite a clock behind the stored stamp");
        });
    }

    #[test]
    fn user_version_overrides_survive_rescans_and_resolve() {
        tauri::async_runtime::block_on(async {
            let db = test_db().await;
            let mut g = game("7", None);
            g.exe_version = Some("1.2.0".into());
            let scan = |games| discovery::ScanResult { games, libraries: vec![], reconcile: discovery::Reconcile::AddOnly };
            save(&db, "gog", scan(vec![g]), 1).await;

            let row = &all_installations(&db).await.unwrap()[0];
            assert_eq!((row.installed_version.as_deref(), row.version_confidence), (Some("1.2.0"), "executable"));

            set_override(&db, "gog", "7", Some(" 1.3 ")).await.unwrap();
            save(&db, "gog", scan(vec![{ let mut g = game("7", None); g.exe_version = Some("1.2.0".into()); g }]), 2).await;
            let row = &all_installations(&db).await.unwrap()[0];
            assert_eq!((row.installed_version.as_deref(), row.version_confidence), (Some("1.3"), "user"), "trimmed, kept across rescans");
            assert!(row.version_conflict, "user 1.3 vs executable 1.2.0");

            let mut with_dlc = game("8", None);
            with_dlc.dlc = vec![discovery::Dlc { id: "81".into(), name: Some("Expansion".into()) }];
            save(&db, "gog", scan(vec![with_dlc]), 3).await;
            let row = all_installations(&db).await.unwrap().into_iter().find(|r| r.source_game_id == "8").unwrap();
            assert_eq!(row.dlc, vec![discovery::Dlc { id: "81".into(), name: Some("Expansion".into()) }], "DLC round-trips");

            set_override(&db, "gog", "7", Some("")).await.unwrap();
            assert_eq!(all_installations(&db).await.unwrap()[0].version_confidence, "executable", "empty clears");
        });
    }
}
