//! Shared pieces for source adapters: the discovered-game shape and safe filesystem helpers.

use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq)]
pub struct DiscoveredGame {
    pub source_game_id: String,
    pub name: String,
    pub path: PathBuf,
    /// `None` = unknown. Never guessed.
    pub version: Option<String>,
    /// How `version` was determined, e.g. `steam_buildid`.
    pub version_source: Option<&'static str>,
    /// `None` = the source does not tell us.
    pub update_available: Option<bool>,
    pub size_bytes: Option<i64>,
    pub last_updated: Option<i64>,
}

pub struct ScanResult {
    pub games: Vec<DiscoveredGame>,
    /// Folders that were actually read. Stale rows outside these must not be treated as uninstalled.
    pub libraries: Vec<PathBuf>,
    /// The source's own install list was read in full (e.g. Epic's manifest folder), so any game
    /// it no longer lists is uninstalled, even if its folder is gone.
    pub authoritative: bool,
}

/// Heroic Games Launcher config folders: Linux native and Flatpak, and `%APPDATA%\heroic` on Windows.
/// Heroic manages GOG and Epic games, mostly on Linux.
pub fn heroic_config_dirs() -> Vec<PathBuf> {
    let home = std::env::home_dir().unwrap_or_default();
    let mut dirs = vec![home.join(".config/heroic"), home.join(".var/app/com.heroicgameslauncher.hgl/config/heroic")];
    dirs.extend(std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("heroic")));
    dirs
}

/// How far below a chosen location to search. 2 covers `D:\SteamLibrary`, `D:\GOG Games\<game>`
/// and `D:\Games\Steam` without crawling the drive.
pub const LOCATION_SEARCH_DEPTH: u32 = 2;

/// Collect `dir` and folders up to `depth` levels below it for which `is_match` holds.
/// Does not descend into a match. Hidden folders and symlinks are skipped.
pub fn find_dirs(dir: &Path, depth: u32, is_match: &impl Fn(&Path) -> bool, out: &mut Vec<PathBuf>) {
    if is_match(dir) {
        out.push(dir.to_path_buf());
        return;
    }
    if depth == 0 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        // file_type() does not follow symlinks, so no loops.
        if !is_hidden(&e) && e.file_type().is_ok_and(|t| t.is_dir()) {
            find_dirs(&e.path(), depth - 1, is_match, out);
        }
    }
}

/// Dot-folders everywhere (default roots already cover ~/.steam etc), plus hidden/system folders on Windows
/// such as `$Recycle.Bin`, where a deleted library would otherwise be picked up.
fn is_hidden(e: &fs::DirEntry) -> bool {
    e.file_name().to_string_lossy().starts_with('.') || has_hidden_attribute(e)
}

#[cfg(windows)]
fn has_hidden_attribute(e: &fs::DirEntry) -> bool {
    use std::os::windows::fs::MetadataExt;
    const HIDDEN_OR_SYSTEM: u32 = 0x2 | 0x4;
    e.metadata().is_ok_and(|m| m.file_attributes() & HIDDEN_OR_SYSTEM != 0)
}

#[cfg(not(windows))]
fn has_hidden_attribute(_: &fs::DirEntry) -> bool {
    false
}

/// UNC paths (`\\host\share`, `//host/share`). Touching one makes Windows authenticate to that host,
/// so paths read from launcher files or the registry must never be network paths.
pub fn is_network_path(p: &str) -> bool {
    p.starts_with(r"\\") || p.starts_with("//")
}

/// `dunce` avoids Windows' `\\?\C:\...` verbatim prefix, which would leak into stored and displayed paths.
pub fn canon(p: &Path) -> PathBuf {
    dunce::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

/// Real launcher metadata files are a few KB. Anything larger is not one, and is not read into memory.
const MAX_METADATA_BYTES: u64 = 4 * 1024 * 1024;

pub fn read_capped(path: &Path) -> Option<String> {
    use std::io::Read;
    let mut s = String::new();
    fs::File::open(path).ok()?.take(MAX_METADATA_BYTES + 1).read_to_string(&mut s).ok()?;
    (s.len() as u64 <= MAX_METADATA_BYTES).then_some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_dirs_within_depth_only() {
        let tmp = std::env::temp_dir().join(format!("repakhub-find-test-{}", std::process::id()));
        for d in ["SteamLibrary", "Games/Steam", "a/b/TooDeep", ".hidden/Steam"] {
            fs::create_dir_all(tmp.join(d).join("steamapps")).unwrap();
        }
        let mut found = vec![];
        find_dirs(&tmp, LOCATION_SEARCH_DEPTH, &|d: &Path| d.join("steamapps").is_dir(), &mut found);
        fs::remove_dir_all(&tmp).ok();

        found.sort();
        assert_eq!(found, vec![tmp.join("Games/Steam"), tmp.join("SteamLibrary")]);
    }
}
