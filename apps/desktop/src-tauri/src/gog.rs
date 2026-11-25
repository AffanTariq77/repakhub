//! GOG source: reads the `goggame-<id>.info` file GOG puts in every game folder, plus GOG's
//! registry entries on Windows. Read-only, never modifies game folders.
//!
//! GOG's local metadata has no "update pending" flag, so update state is always unknown here.

use crate::discovery::{self, DiscoveredGame, ScanResult, canon};
use serde::Deserialize;
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

/// The parts of `goggame-<id>.info` we use. Its own `"version"` field is the file-format version,
/// not the game version, so it is deliberately ignored.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GogInfo {
    game_id: Id,
    /// Set on DLC to the base game's id. Equal to `game_id` (or absent) on the base game.
    root_game_id: Option<Id>,
    name: String,
    build_id: Option<Id>,
}

/// GOG ids appear as JSON strings in most files, but accept numbers too rather than drop the game.
#[derive(Deserialize)]
#[serde(untagged)]
enum Id {
    Str(String),
    Num(u64),
}

impl From<Id> for String {
    fn from(id: Id) -> String {
        match id {
            Id::Str(s) => s,
            Id::Num(n) => n.to_string(),
        }
    }
}

/// One entry under `HKLM\SOFTWARE\WOW6432Node\GOG.com\Games\<gameID>`.
struct RegistryGame {
    path: String,
    version: Option<String>,
    build_id: Option<String>,
}

/// Default GOG install folders. Games elsewhere are found via the registry or the user's scan locations.
fn default_roots() -> Vec<PathBuf> {
    if cfg!(windows) {
        vec![r"C:\Program Files (x86)\GOG Galaxy\Games".into(), r"C:\GOG Games".into()]
    } else {
        let home = std::env::home_dir().unwrap_or_default();
        vec![home.join("GOG Games"), home.join("Games/Heroic")]
    }
}

#[cfg(windows)]
fn registry_games() -> HashMap<String, RegistryGame> {
    use winreg::RegKey;
    use winreg::enums::HKEY_LOCAL_MACHINE;
    let Ok(games) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(r"SOFTWARE\WOW6432Node\GOG.com\Games") else {
        return HashMap::new();
    };
    games
        .enum_keys()
        .flatten()
        .filter_map(|id| {
            let key = games.open_subkey(&id).ok()?;
            let get = |name: &str| key.get_value::<String, _>(name).ok().filter(|v| !v.is_empty());
            let entry = RegistryGame { path: get("path")?, version: get("ver"), build_id: get("buildId") };
            Some((id, entry))
        })
        .filter(|(_, g)| !discovery::is_network_path(&g.path))
        .collect()
}

#[cfg(not(windows))]
fn registry_games() -> HashMap<String, RegistryGame> {
    HashMap::new()
}

pub fn scan(locations: &[PathBuf]) -> ScanResult {
    scan_with(&registry_games(), &default_roots(), locations)
}

fn scan_with(registry: &HashMap<String, RegistryGame>, defaults: &[PathBuf], locations: &[PathBuf]) -> ScanResult {
    let mut game_dirs: Vec<PathBuf> = registry.values().map(|g| PathBuf::from(&g.path)).collect();
    let mut libraries = vec![];
    for root in defaults.iter().chain(locations) {
        if fs::read_dir(root).is_ok() {
            libraries.push(canon(root));
            discovery::find_dirs(root, discovery::LOCATION_SEARCH_DEPTH, &|d: &Path| !info_files(d).is_empty(), &mut game_dirs);
        }
    }
    // ponytail: a registry-only install outside every searched folder that gets uninstalled stays listed as offline;
    // fix with the 'offline' UI state or by reconciling against the registry.
    let dirs: BTreeSet<PathBuf> = game_dirs.iter().map(|d| canon(d)).collect();
    let games = dirs.iter().flat_map(|d| read_game_dir(d, registry)).collect();
    ScanResult { games, libraries }
}

fn info_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else { return vec![] };
    entries
        .flatten()
        // file_type() does not follow links: a planted symlink to a network share is never opened.
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.path())
        .filter(|p| {
            let name = p.file_name().unwrap_or_default().to_string_lossy();
            name.starts_with("goggame-") && name.ends_with(".info")
        })
        .collect()
}

/// Base games in one folder (DLC info files are skipped). Version precedence: registry `ver`,
/// then a build id (registry or info file), else unknown.
fn read_game_dir(dir: &Path, registry: &HashMap<String, RegistryGame>) -> Vec<DiscoveredGame> {
    info_files(dir)
        .iter()
        .filter_map(|p| serde_json::from_str::<GogInfo>(&discovery::read_capped(p)?).ok())
        .map(|i| (String::from(i.game_id), i.root_game_id.map(String::from), i.name, i.build_id.map(String::from)))
        .filter(|(id, root, _, _)| root.as_ref().is_none_or(|r| r == id))
        .map(|(game_id, _, name, info_build_id)| {
            let reg = registry.get(&game_id);
            let build_id = reg.and_then(|r| r.build_id.clone()).or(info_build_id).filter(|b| !b.is_empty());
            let (version, version_source) = match (reg.and_then(|r| r.version.clone()), build_id) {
                (Some(v), _) => (Some(v), Some("gog_version")),
                (None, Some(b)) => (Some(b), Some("gog_buildid")),
                (None, None) => (None, None),
            };
            DiscoveredGame {
                source_game_id: game_id,
                name,
                path: dir.to_path_buf(),
                version,
                version_source,
                update_available: None,
                size_bytes: None,
                last_updated: None,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_base_games_skips_dlc_and_ranks_version_sources() {
        let tmp = std::env::temp_dir().join(format!("repakhub-gog-test-{}", std::process::id()));
        let root = tmp.join("GOG Games");
        let write = |rel: &str, body: &str| {
            let p = tmp.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, body).unwrap();
        };
        // Base game + DLC in one folder; registry supplies a human version.
        write("GOG Games/Game A/goggame-1.info", r#"{"gameId":"1","rootGameId":"1","name":"Game A","buildId":"555","version":1}"#);
        write("GOG Games/Game A/goggame-2.info", r#"{"gameId":"2","rootGameId":"1","name":"Game A DLC","version":1}"#);
        // Numeric ids, no rootGameId, no build id, not in registry: found, version unknown.
        write("GOG Games/Game B/goggame-3.info", r#"{"gameId":3,"name":"Game B","version":1}"#);
        write("GOG Games/Game B/goggame-6.info", r#"{"gameId":6,"rootGameId":3,"name":"Game B DLC","version":1}"#);
        // Malformed info file is skipped.
        write("GOG Games/Broken/goggame-4.info", "{ not json");
        // Found only through the registry, outside every searched folder; registry has a build id but no ver.
        write("Elsewhere/Game C/goggame-5.info", r#"{"gameId":"5","name":"Game C","version":1}"#);
        // A symlinked info file is never followed (would otherwise list game 1 a second time under Game B).
        #[cfg(unix)]
        std::os::unix::fs::symlink(tmp.join("GOG Games/Game A/goggame-1.info"), tmp.join("GOG Games/Game B/goggame-7.info"))
            .unwrap();

        let registry = HashMap::from([
            ("1".to_string(), RegistryGame {
                path: root.join("Game A").to_string_lossy().into(),
                version: Some("1.32".into()),
                build_id: None,
            }),
            ("5".to_string(), RegistryGame {
                path: tmp.join("Elsewhere/Game C").to_string_lossy().into(),
                version: None,
                build_id: Some("777".into()),
            }),
        ]);

        let result = scan_with(&registry, &[root.clone(), tmp.join("missing")], &[]);
        let mut games = result.games;
        games.sort_by(|a, b| a.source_game_id.cmp(&b.source_game_id));
        let root_canon = canon(&root);
        fs::remove_dir_all(&tmp).ok();

        let summary: Vec<_> = games.iter().map(|g| (g.source_game_id.as_str(), g.version.as_deref(), g.version_source)).collect();
        assert_eq!(
            summary,
            vec![
                ("1", Some("1.32"), Some("gog_version")),
                ("3", None, None),
                ("5", Some("777"), Some("gog_buildid")),
            ],
            "DLC and malformed files skipped; registry folder not duplicated with the searched one"
        );
        assert!(games.iter().all(|g| g.update_available.is_none()), "GOG never claims an update state");
        assert_eq!(result.libraries, vec![root_canon], "only readable roots count as scanned");
    }
}
