//! Installs and updates from authorised sources (M3, part 3): catalog, update check, and background
//! jobs that download (downloads.rs) and apply (apply.rs), reporting progress to the UI as events.
//!
//! A source is a trusted key with a `catalog`: an HTTPS folder holding `index.json` (a JSON list of
//! game ids) and one signed `<gameId>.json` envelope per game. The index is unsigned, but nothing is
//! shown or installed unless its manifest verifies.

use crate::downloads::{self, DownloadError, Transport};
use crate::manifest::{self, Manifest, TrustedKey};
use crate::{ScanLock, apply, managed, settings};
use serde::Serialize;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager, State};

const MAX_INDEX_BYTES: u64 = 64 * 1024;
const WORKERS: usize = 3;
pub const JOB_EVENT: &str = "repakhub://job";

/// Running jobs (by id) with their cancel flags, plus what the last update check found.
#[derive(Default)]
pub struct UpdateState {
    jobs: Mutex<HashMap<String, Arc<AtomicBool>>>,
    available: Mutex<HashMap<i64, AvailableUpdate>>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEntry {
    source: String,
    game_id: String,
    name: String,
    version: String,
    changelog: String,
    size_bytes: u64,
    installed: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    entries: Vec<CatalogEntry>,
    /// Sources that could not be reached or did not verify, as readable messages.
    errors: Vec<String>,
    /// How many sources are registered at all (0 until one is added under the source policy).
    sources: usize,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AvailableUpdate {
    installation_id: i64,
    version: String,
    changelog: String,
    /// The new version's total size; only files that changed are downloaded.
    size_bytes: u64,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct JobEvent {
    id: String,
    name: String,
    /// `checking`, `downloading`, `applying`, `done`, `paused` or `failed`.
    phase: &'static str,
    done: u64,
    total: u64,
    error: Option<String>,
}

/// Game ids end up in URLs and folder names, so only a conservative character set is accepted.
fn valid_game_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 100 && !id.starts_with('.') && id.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
}

fn catalog_base(key: &TrustedKey) -> Option<&str> {
    key.catalog.as_deref().filter(|c| c.starts_with("https://") && c.ends_with('/'))
}

fn manifest_url(key: &TrustedKey, game_id: &str) -> Option<String> {
    let base = catalog_base(key)?;
    valid_game_id(game_id).then(|| format!("{base}{game_id}.json"))
}

fn key_for(source: &str) -> Option<TrustedKey> {
    manifest::trusted_keys().into_iter().find(|k| k.source == source && catalog_base(k).is_some())
}

fn fetch_for(source: &str, game_id: &str) -> Result<Manifest, String> {
    let key = key_for(source).ok_or_else(|| format!("{source} is not a registered source"))?;
    let url = manifest_url(&key, game_id).ok_or_else(|| format!("invalid game id {game_id:?}"))?;
    let m = downloads::fetch_manifest(&Transport::new(), &url).map_err(|e| e.to_string())?;
    if m.game_id != game_id {
        return Err(format!("{url} describes {:?}, not {game_id:?}", m.game_id));
    }
    Ok(m)
}

#[tauri::command]
pub async fn list_catalog(db: State<'_, SqlitePool>) -> Result<Catalog, String> {
    let installed: Vec<(String, String)> = sqlx::query_as("SELECT source, source_game_id FROM installations WHERE launcher = 'repakhub'")
        .fetch_all(db.inner())
        .await
        .map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        let keys: Vec<TrustedKey> = manifest::trusted_keys().into_iter().filter(|k| catalog_base(k).is_some()).collect();
        let t = Transport::new();
        let mut catalog = Catalog { entries: vec![], errors: vec![], sources: keys.len() };
        for key in &keys {
            let base = catalog_base(key).unwrap_or_default();
            let ids = downloads::fetch_text(&t, &format!("{base}index.json"), MAX_INDEX_BYTES)
                .map_err(|e| e.to_string())
                .and_then(|text| serde_json::from_str::<Vec<String>>(&text).map_err(|e| e.to_string()));
            let ids = match ids {
                Ok(ids) => ids,
                Err(e) => {
                    catalog.errors.push(format!("{}: {e}", key.source));
                    continue;
                }
            };
            for id in ids.iter().filter(|id| valid_game_id(id)) {
                match fetch_for(&key.source, id) {
                    Ok(m) => catalog.entries.push(CatalogEntry {
                        installed: installed.iter().any(|(s, g)| *s == m.source && *g == m.game_id),
                        size_bytes: m.total_size(),
                        source: m.source,
                        game_id: m.game_id,
                        name: m.name,
                        version: m.version,
                        changelog: m.changelog,
                    }),
                    Err(e) => catalog.errors.push(format!("{}/{id}: {e}", key.source)),
                }
            }
        }
        catalog
    })
    .await
    .map_err(|e| e.to_string())
}

/// Check every RepakHub-installed game against its source. Stores the result on the installation
/// (`update_available`) and keeps the details for the UI.
#[tauri::command]
pub async fn check_updates(db: State<'_, SqlitePool>, state: State<'_, UpdateState>) -> Result<Vec<AvailableUpdate>, String> {
    let rows: Vec<(i64, String, String, Option<String>)> = sqlx::query_as(
        "SELECT id, source, source_game_id, installed_version FROM installations WHERE launcher = 'repakhub'",
    )
    .fetch_all(db.inner())
    .await
    .map_err(|e| e.to_string())?;
    let results = tauri::async_runtime::spawn_blocking(move || {
        rows.into_iter().map(|(id, source, game, installed)| (id, installed, fetch_for(&source, &game))).collect::<Vec<_>>()
    })
    .await
    .map_err(|e| e.to_string())?;

    let mut available = vec![];
    for (id, installed, fetched) in results {
        // Unreachable source or incomparable versions: unknown, never guessed.
        let newer = fetched.as_ref().ok().and_then(|m| manifest::is_newer(installed.as_deref(), &m.version));
        sqlx::query("UPDATE installations SET update_available = ? WHERE id = ?")
            .bind(newer)
            .bind(id)
            .execute(db.inner())
            .await
            .map_err(|e| e.to_string())?;
        if let (Some(true), Ok(m)) = (newer, fetched) {
            available.push(AvailableUpdate { installation_id: id, size_bytes: m.total_size(), version: m.version, changelog: m.changelog });
        }
    }
    let mut map = state.available.lock().unwrap();
    map.clear();
    map.extend(available.iter().map(|u| (u.installation_id, u.clone())));
    Ok(available)
}

/// Install a game from a source into `parent` (default: RepakHub's library folder)/`gameId`.
/// Returns the job id; progress arrives as `repakhub://job` events.
#[tauri::command]
pub async fn install_game(
    app: AppHandle,
    db: State<'_, SqlitePool>,
    lock: State<'_, ScanLock>,
    source: String,
    game_id: String,
    parent: Option<String>,
) -> Result<String, String> {
    if !valid_game_id(&game_id) {
        return Err(format!("invalid game id {game_id:?}"));
    }
    let parent = parent.map(PathBuf::from).unwrap_or_else(managed::library_dir);
    if !parent.is_absolute() {
        return Err("install folder must be an absolute path".into());
    }
    let dir = parent.join(&game_id);
    let foreign = std::fs::read_dir(&dir).is_ok_and(|mut d| d.next().is_some())
        && apply::read_record(&dir).is_none_or(|r| r.source != source || r.game_id != game_id);
    if foreign {
        return Err(format!("{} already exists and is not this game", dir.display()));
    }
    // Make sure the next scan finds it: add the parent to the scan locations unless it is the library folder.
    if parent != managed::library_dir() {
        let _guard = lock.0.lock().await;
        let mut s = settings::load(db.inner()).await?;
        let p = parent.to_string_lossy().into_owned();
        if !s.scan_locations.contains(&p) {
            s.scan_locations.push(p);
            settings::save(db.inner(), s).await?;
        }
    }
    Ok(start_job(app, format!("{source}/{game_id}"), source, game_id, dir))
}

#[tauri::command]
pub async fn update_game(app: AppHandle, db: State<'_, SqlitePool>, installation_id: i64) -> Result<String, String> {
    let (source, game_id, path): (String, String, String) =
        sqlx::query_as("SELECT source, source_game_id, path FROM installations WHERE id = ? AND launcher = 'repakhub'")
            .bind(installation_id)
            .fetch_optional(db.inner())
            .await
            .map_err(|e| e.to_string())?
            .ok_or("not a game RepakHub installed")?;
    Ok(start_job(app, format!("{source}/{game_id}"), source, game_id, PathBuf::from(path)))
}

/// Pause a running job. Downloaded parts are kept; starting the same install or update resumes.
#[tauri::command]
pub fn pause_job(state: State<'_, UpdateState>, id: String) {
    if let Some(flag) = state.jobs.lock().unwrap().get(&id) {
        flag.store(true, Ordering::Relaxed);
    }
}

fn start_job(app: AppHandle, id: String, source: String, game_id: String, dir: PathBuf) -> String {
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let state = app.state::<UpdateState>();
        let mut jobs = state.jobs.lock().unwrap();
        if jobs.contains_key(&id) {
            return id; // already running
        }
        jobs.insert(id.clone(), cancel.clone());
    }
    let job_id = id.clone();
    std::thread::spawn(move || {
        let emit = |name: &str, phase: &'static str, done: u64, total: u64, error: Option<String>| {
            let _ = app.emit(JOB_EVENT, JobEvent { id: job_id.clone(), name: name.into(), phase, done, total, error });
        };
        emit(&game_id, "checking", 0, 0, None);
        let result = run_job(&source, &game_id, &dir, &cancel, &emit);
        app.state::<UpdateState>().jobs.lock().unwrap().remove(&job_id);
        match result {
            Ok(name) => {
                let db = app.state::<SqlitePool>().inner().clone();
                let rescan = tauri::async_runtime::block_on(async {
                    // Just applied the latest version: up to date (rescans keep the last check's answer otherwise).
                    sqlx::query("UPDATE installations SET update_available = 0 WHERE source = ? AND source_game_id = ?")
                        .bind(&source)
                        .bind(&game_id)
                        .execute(&db)
                        .await
                        .map_err(|e| e.to_string())?;
                    crate::run_scan(&db, &app.state::<ScanLock>()).await
                });
                emit(&name, "done", 0, 0, rescan.err());
            }
            Err(JobError::Paused(name)) => emit(&name, "paused", 0, 0, None),
            Err(JobError::Failed(name, e)) => emit(&name, "failed", 0, 0, Some(e)),
        }
    });
    id
}

/// Report a job's progress: (game name, phase, bytes done, bytes total, error).
type Emit<'a> = dyn Fn(&str, &'static str, u64, u64, Option<String>) + Sync + 'a;

enum JobError {
    Paused(String),
    Failed(String, String),
}

fn run_job(
    source: &str,
    game_id: &str,
    dir: &Path,
    cancel: &AtomicBool,
    emit: &Emit<'_>,
) -> Result<String, JobError> {
    let m = fetch_for(source, game_id).map_err(|e| JobError::Failed(game_id.into(), e))?;
    let name = m.name.clone();
    let fail = |e: String| JobError::Failed(name.clone(), e);
    std::fs::create_dir_all(dir).map_err(|e| fail(e.to_string()))?;
    let needed = downloads::plan(dir, &m).map_err(|e| fail(e.to_string()))?;
    let total: u64 = needed.iter().map(|f| f.size).sum();

    let per_file: Mutex<HashMap<String, u64>> = Mutex::new(HashMap::new());
    let last = Mutex::new(Instant::now());
    let progress = |path: &str, done: u64| {
        let mut files = per_file.lock().unwrap();
        files.insert(path.to_string(), done);
        let mut last = last.lock().unwrap();
        if last.elapsed().as_millis() >= 200 {
            *last = Instant::now();
            emit(&name, "downloading", files.values().sum(), total, None);
        }
    };
    emit(&name, "downloading", 0, total, None);
    let t = Transport::new();
    match downloads::download_all(&t, &m, &needed, &apply::staging_dir(dir), WORKERS, cancel, &progress) {
        Ok(()) => {}
        Err(DownloadError::Cancelled) => return Err(JobError::Paused(name)),
        Err(e) => return Err(fail(e.to_string())),
    }
    emit(&name, "applying", total, total, None);
    apply::apply(dir, &m, &needed).map_err(|e| fail(e.to_string()))?;
    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_ids_and_catalog_urls_are_constrained() {
        for ok in ["demo", "super-tux_2", "v1.2"] {
            assert!(valid_game_id(ok), "{ok}");
        }
        for bad in ["", "../x", "a/b", ".hidden", "a b", "a?b=1", &"x".repeat(101)] {
            assert!(!valid_game_id(bad), "{bad:?}");
        }
        let key = |catalog: &str| TrustedKey {
            id: "k".into(),
            source: "s".into(),
            public_key: String::new(),
            rights_holder: false,
            catalog: Some(catalog.into()),
        };
        assert_eq!(manifest_url(&key("https://cdn.example.org/repakhub/"), "demo").as_deref(), Some("https://cdn.example.org/repakhub/demo.json"));
        assert_eq!(manifest_url(&key("http://cdn.example.org/"), "demo"), None, "http catalog ignored");
        assert_eq!(manifest_url(&key("https://cdn.example.org/repakhub"), "demo"), None, "must end in /");
        assert_eq!(manifest_url(&key("https://cdn.example.org/"), "../etc"), None);
        assert!(key_for("anything").is_none(), "only registered sources resolve");
    }

    fn demo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../demo-source")
    }

    /// Offline: the committed demo catalog verifies against the compiled-in key, and every file it
    /// lists matches the file in the repository. Catches a forgotten re-sign or a hand-edited file.
    #[test]
    fn committed_demo_catalog_verifies_and_matches_its_files() {
        let root = demo_root();
        let key = key_for("repakhub-demo").expect("demo source registered with a catalog");
        let envelope = std::fs::read_to_string(root.join("repakhub-demo.json")).unwrap();
        let m = manifest::verify_with(&manifest::trusted_keys(), &envelope).unwrap();
        assert_eq!((m.source.as_str(), m.game_id.as_str()), ("repakhub-demo", "repakhub-demo"));
        let index: Vec<String> = serde_json::from_str(&std::fs::read_to_string(root.join("index.json")).unwrap()).unwrap();
        assert_eq!(index, vec!["repakhub-demo"]);
        for f in &m.files {
            let local = root.join("files").join(&m.version).join(&f.path);
            assert_eq!(std::fs::metadata(&local).unwrap().len(), f.size, "{}", f.path);
            assert_eq!(downloads::sha256_file(&local).unwrap(), f.sha256, "{}", f.path);
            assert_eq!(f.url, format!("{}files/{}/{}", catalog_base(&key).unwrap(), m.version, f.path));
        }
    }

    /// Live: installs the demo pack from the real catalog on `main`. Run with `cargo test -- --ignored`.
    #[test]
    #[ignore = "needs network and the demo catalog published on main"]
    fn live_demo_install() {
        let dir = std::env::temp_dir().join(format!("repakhub-live-demo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let phases = Mutex::new(vec![]);
        let emit = |_: &str, phase: &'static str, _: u64, _: u64, _: Option<String>| phases.lock().unwrap().push(phase);
        let name = run_job("repakhub-demo", "repakhub-demo", &dir, &AtomicBool::new(false), &emit).unwrap_or_else(|_| panic!("install failed"));
        assert_eq!(name, "RepakHub Demo Pack");
        let record = apply::read_record(&dir).expect("install record written");
        assert!(!record.files.is_empty() && record.files.iter().all(|f| dir.join(f).is_file()));
        assert!(phases.lock().unwrap().contains(&"applying"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
