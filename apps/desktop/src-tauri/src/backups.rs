//! Save and config backups (M4, part 2). The user chooses which files or folders belong to a game; a
//! backup is a plain folder snapshot under the app data folder, so it can be opened in a file manager
//! and restored by copying back.
//!
//! Layout: `<root>/<id>/backup.json` plus `<root>/<id>/<n>/<name>` for the n-th chosen path. A backup
//! is written to `<id>.tmp` and renamed when complete, so a half-written one never appears. Restoring
//! first takes an automatic backup of the current state, then swaps each path in.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub id: String,
    pub created_at: i64,
    pub size_bytes: u64,
    /// The original locations, restored to these same paths.
    pub paths: Vec<String>,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum BackupError {
    #[error("{0} is not an absolute path")]
    NotAbsolute(String),
    #[error("{0} does not exist")]
    Missing(String),
    #[error("{0} contains RepakHub's backup folder, which would back up the backups")]
    ContainsBackups(String),
    #[error("{0} is a whole drive; choose the save or config folder inside it")]
    Root(String),
    #[error("no backup {0}")]
    NoSuchBackup(String),
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// Folder holding one game's backups. Source and game id become a safe folder name.
pub fn game_root(backups_root: &Path, source: &str, game_id: &str) -> PathBuf {
    let safe = |s: &str| s.chars().map(|c| if c.is_ascii_alphanumeric() || "-_.".contains(c) { c } else { '_' }).collect::<String>();
    backups_root.join(format!("{}-{}", safe(source), safe(game_id)))
}

/// Check paths the user chose: absolute, existing, and not containing the backups folder itself.
pub fn validate_paths(backups_root: &Path, paths: &[String]) -> Result<Vec<String>, BackupError> {
    let mut out: Vec<String> = vec![];
    for p in paths {
        let path = Path::new(p);
        if !path.is_absolute() {
            return Err(BackupError::NotAbsolute(p.clone()));
        }
        if !path.exists() {
            return Err(BackupError::Missing(p.clone()));
        }
        // A drive or filesystem root has no name to restore into, and backing up a whole drive is never meant.
        if path.file_name().is_none() {
            return Err(BackupError::Root(p.clone()));
        }
        if backups_root.starts_with(path) {
            return Err(BackupError::ContainsBackups(p.clone()));
        }
        if !out.contains(p) {
            out.push(p.clone());
        }
    }
    Ok(out)
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64
}

/// Snapshot `paths` (missing ones are skipped) into a new backup under `root`.
pub fn create(root: &Path, paths: &[String], note: Option<String>) -> Result<BackupInfo, BackupError> {
    fs::create_dir_all(root)?;
    // Ids are millisecond timestamps; bump past an existing one so two quick backups never collide.
    let mut created_at = now_ms();
    while root.join(created_at.to_string()).exists() {
        created_at += 1;
    }
    let id = created_at.to_string();
    let tmp = root.join(format!("{id}.tmp"));
    let _ = fs::remove_dir_all(&tmp);
    let mut size_bytes = 0;
    let mut kept = vec![];
    for p in paths.iter().filter(|p| Path::new(p).exists()) {
        let src = Path::new(p);
        let name = src.file_name().map(|n| n.to_os_string()).unwrap_or_else(|| "root".into());
        let slot = tmp.join(kept.len().to_string());
        fs::create_dir_all(&slot)?;
        size_bytes += copy_tree(src, &slot.join(name))?;
        kept.push(p.clone());
    }
    let info = BackupInfo { id: id.clone(), created_at, size_bytes, paths: kept, note };
    fs::create_dir_all(&tmp)?;
    fs::write(tmp.join("backup.json"), serde_json::to_vec_pretty(&info).map_err(io::Error::other)?)?;
    fs::rename(&tmp, root.join(&id))?;
    Ok(info)
}

/// Copy a file or folder, returning bytes copied. Symlinks are skipped, never followed.
fn copy_tree(src: &Path, dst: &Path) -> io::Result<u64> {
    let kind = fs::symlink_metadata(src)?.file_type();
    if kind.is_symlink() {
        return Ok(0);
    }
    if kind.is_file() {
        return fs::copy(src, dst);
    }
    fs::create_dir_all(dst)?;
    let mut total = 0;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        total += copy_tree(&entry.path(), &dst.join(entry.file_name()))?;
    }
    Ok(total)
}

/// Backups under `root`, newest first. Unfinished (`.tmp`) or unreadable ones are left out.
pub fn list(root: &Path) -> Vec<BackupInfo> {
    let Ok(entries) = fs::read_dir(root) else { return vec![] };
    let mut out: Vec<BackupInfo> = entries
        .flatten()
        .filter(|e| valid_id(&e.file_name().to_string_lossy()))
        .filter_map(|e| serde_json::from_slice(&fs::read(e.path().join("backup.json")).ok()?).ok())
        .collect();
    out.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    out
}

/// Ids are the digits of a timestamp; anything else is refused before it can reach a path.
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 20 && id.chars().all(|c| c.is_ascii_digit())
}

fn load(root: &Path, id: &str) -> Result<BackupInfo, BackupError> {
    if !valid_id(id) {
        return Err(BackupError::NoSuchBackup(id.into()));
    }
    let raw = fs::read(root.join(id).join("backup.json")).map_err(|_| BackupError::NoSuchBackup(id.into()))?;
    serde_json::from_slice(&raw).map_err(|_| BackupError::NoSuchBackup(id.into()))
}

/// Restore backup `id` to its original paths, after taking a "before restore" backup of what is there now.
/// Returns that safety backup.
pub fn restore(root: &Path, id: &str) -> Result<BackupInfo, BackupError> {
    let info = load(root, id)?;
    for p in &info.paths {
        if !Path::new(p).is_absolute() {
            return Err(BackupError::NotAbsolute(p.clone()));
        }
    }
    let when = info.created_at;
    let safety = create(root, &info.paths, Some(format!("Automatic, before restoring the backup from {when}")))?;
    for (n, p) in info.paths.iter().enumerate() {
        let target = Path::new(p);
        let name = target.file_name().map(|n| n.to_os_string()).unwrap_or_else(|| "root".into());
        let src = root.join(id).join(n.to_string()).join(&name);
        swap_in(&src, target)?;
    }
    Ok(safety)
}

/// Copy `src` next to `target`, then swap it in; if the swap fails the original is put back.
fn swap_in(src: &Path, target: &Path) -> io::Result<()> {
    let sibling = |suffix: &str| {
        let mut name = std::ffi::OsString::from(".");
        name.push(target.file_name().unwrap_or_default());
        name.push(suffix);
        target.with_file_name(name)
    };
    let (incoming, old) = (sibling(".repakhub-restore"), sibling(".repakhub-old"));
    for leftover in [&incoming, &old] {
        let _ = fs::remove_dir_all(leftover).or_else(|_| fs::remove_file(leftover));
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    copy_tree(src, &incoming)?;
    let had_original = target.exists();
    if had_original {
        fs::rename(target, &old)?;
    }
    if let Err(e) = fs::rename(&incoming, target) {
        if had_original {
            let _ = fs::rename(&old, target);
        }
        return Err(e);
    }
    if had_original {
        let _ = fs::remove_dir_all(&old).or_else(|_| fs::remove_file(&old));
    }
    Ok(())
}

pub fn delete(root: &Path, id: &str) -> Result<(), BackupError> {
    load(root, id)?; // validates the id and that it is a backup
    fs::remove_dir_all(root.join(id))?;
    Ok(())
}

// --- Commands -------------------------------------------------------------------------------

use sqlx::SqlitePool;
use tauri::{AppHandle, Manager, State};

/// A game's chosen paths and its backups, newest first.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupState {
    paths: Vec<String>,
    backups: Vec<BackupInfo>,
}

fn backups_root(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app.path().app_data_dir().map_err(|e| e.to_string())?.join("backups"))
}

async fn chosen_paths(db: &SqlitePool, source: &str, game_id: &str) -> Result<Vec<String>, String> {
    let row: Option<(String,)> = sqlx::query_as("SELECT paths FROM backup_paths WHERE source = ? AND source_game_id = ?")
        .bind(source)
        .bind(game_id)
        .fetch_optional(db)
        .await
        .map_err(|e| e.to_string())?;
    Ok(row.and_then(|(j,)| serde_json::from_str(&j).ok()).unwrap_or_default())
}

async fn state(app: &AppHandle, db: &SqlitePool, source: &str, game_id: &str) -> Result<BackupState, String> {
    let root = game_root(&backups_root(app)?, source, game_id);
    Ok(BackupState { paths: chosen_paths(db, source, game_id).await?, backups: list(&root) })
}

#[tauri::command]
pub async fn get_backups(app: AppHandle, db: State<'_, SqlitePool>, source: String, source_game_id: String) -> Result<BackupState, String> {
    state(&app, &db, &source, &source_game_id).await
}

#[tauri::command]
pub async fn set_backup_paths(
    app: AppHandle,
    db: State<'_, SqlitePool>,
    source: String,
    source_game_id: String,
    paths: Vec<String>,
) -> Result<BackupState, String> {
    let paths = validate_paths(&backups_root(&app)?, &paths).map_err(|e| e.to_string())?;
    sqlx::query(
        "INSERT INTO backup_paths (source, source_game_id, paths) VALUES (?, ?, ?)
         ON CONFLICT (source, source_game_id) DO UPDATE SET paths = excluded.paths",
    )
    .bind(&source)
    .bind(&source_game_id)
    .bind(serde_json::to_string(&paths).map_err(|e| e.to_string())?)
    .execute(db.inner())
    .await
    .map_err(|e| e.to_string())?;
    state(&app, &db, &source, &source_game_id).await
}

#[tauri::command]
pub async fn create_backup(app: AppHandle, db: State<'_, SqlitePool>, source: String, source_game_id: String) -> Result<BackupState, String> {
    let paths = chosen_paths(&db, &source, &source_game_id).await?;
    if paths.is_empty() {
        return Err("choose at least one file or folder to back up".into());
    }
    let root = game_root(&backups_root(&app)?, &source, &source_game_id);
    tauri::async_runtime::spawn_blocking(move || create(&root, &paths, None))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    state(&app, &db, &source, &source_game_id).await
}

#[tauri::command]
pub async fn restore_backup(
    app: AppHandle,
    db: State<'_, SqlitePool>,
    source: String,
    source_game_id: String,
    id: String,
) -> Result<BackupState, String> {
    let root = game_root(&backups_root(&app)?, &source, &source_game_id);
    tauri::async_runtime::spawn_blocking(move || restore(&root, &id))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    state(&app, &db, &source, &source_game_id).await
}

#[tauri::command]
pub async fn delete_backup(
    app: AppHandle,
    db: State<'_, SqlitePool>,
    source: String,
    source_game_id: String,
    id: String,
) -> Result<BackupState, String> {
    let root = game_root(&backups_root(&app)?, &source, &source_game_id);
    delete(&root, &id).map_err(|e| e.to_string())?;
    state(&app, &db, &source, &source_game_id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("repakhub-backup-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn backs_up_and_restores_files_and_folders() {
        let base = tmp("roundtrip");
        let saves = base.join("Saves");
        fs::create_dir_all(saves.join("slot1")).unwrap();
        fs::write(saves.join("slot1/save.dat"), b"level 3").unwrap();
        let config = base.join("settings.ini");
        fs::write(&config, b"vsync=on").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("/etc", saves.join("link")).unwrap();
        let root = game_root(&base.join("backups"), "gog", "1/2");
        assert!(root.ends_with("gog-1_2"), "unsafe characters replaced");

        let paths = vec![saves.to_string_lossy().into_owned(), config.to_string_lossy().into_owned()];
        let b = create(&root, &paths, None).unwrap();
        assert_eq!(b.size_bytes, 15);
        assert!(!root.join(&b.id).join("0/Saves/link").exists(), "symlinks are not followed");

        // Progress is lost: the save is overwritten, a new file appears, the config is deleted.
        fs::write(saves.join("slot1/save.dat"), b"level 1").unwrap();
        fs::write(saves.join("slot1/new.dat"), b"x").unwrap();
        fs::remove_file(&config).unwrap();

        let safety = restore(&root, &b.id).unwrap();
        assert_eq!(fs::read(saves.join("slot1/save.dat")).unwrap(), b"level 3");
        assert!(!saves.join("slot1/new.dat").exists(), "restore replaces, not merges");
        assert_eq!(fs::read(&config).unwrap(), b"vsync=on");
        assert!(safety.note.unwrap().starts_with("Automatic"));
        assert_eq!(safety.paths.len(), 1, "the deleted config had nothing to save");

        let all = list(&root);
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].id, safety.id, "newest first");
        assert!(base.read_dir().unwrap().flatten().all(|e| !e.file_name().to_string_lossy().contains(".repakhub-")), "no leftovers");
    }

    #[test]
    fn validates_paths_and_ids() {
        let base = tmp("validate");
        let backups = base.join("data/backups");
        fs::create_dir_all(&backups).unwrap();
        let p = |path: &Path| path.to_string_lossy().into_owned();
        assert!(matches!(validate_paths(&backups, &["relative".into()]), Err(BackupError::NotAbsolute(_))));
        assert!(matches!(validate_paths(&backups, &[p(&base.join("nope"))]), Err(BackupError::Missing(_))));
        assert!(matches!(validate_paths(&backups, &[p(&base.join("data"))]), Err(BackupError::ContainsBackups(_))));
        let root = if cfg!(windows) { r"C:\" } else { "/" };
        assert!(matches!(validate_paths(&base.join("elsewhere"), &[root.into()]), Err(BackupError::Root(_))));
        let saves = base.join("saves");
        fs::create_dir_all(&saves).unwrap();
        assert_eq!(validate_paths(&backups, &[p(&saves), p(&saves)]).unwrap(), vec![p(&saves)], "deduplicated");

        let root = base.join("game");
        let b = create(&root, &[], None).unwrap();
        for bad in ["../x", "1.tmp", "", "abc"] {
            assert!(matches!(delete(&root, bad), Err(BackupError::NoSuchBackup(_))), "{bad:?}");
        }
        fs::create_dir_all(root.join("999.tmp")).unwrap();
        assert_eq!(list(&root).len(), 1, "unfinished backups are not listed");
        delete(&root, &b.id).unwrap();
        assert!(list(&root).is_empty());
    }
}
