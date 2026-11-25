//! GOG source: reads the `goggame-<id>.info` file GOG puts in every game folder, plus the
//! launcher's own install list: GOG Galaxy's registry entries on Windows, Heroic on Linux.
//! Read-only, never modifies game folders.
//!
//! GOG's local metadata has no "update pending" flag, so update state is always unknown here.

use crate::discovery::{self, DiscoveredGame, Dlc, Reconcile, ScanResult, canon};
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
    #[serde(default)]
    play_tasks: Vec<PlayTask>,
}

/// How GOG launches the game; the primary task's `path` is the main executable, relative to the game folder.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlayTask {
    #[serde(default)]
    is_primary: bool,
    path: Option<String>,
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

/// An install the launcher told us about: a registry entry (`HKLM\SOFTWARE\WOW6432Node\GOG.com\Games\<gameID>`)
/// on Windows, or a Heroic `gog_store/installed.json` entry on Linux.
struct KnownInstall {
    path: String,
    /// `galaxy` (registry) or `heroic`.
    launcher: &'static str,
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

/// Heroic's list (any OS), overridden by GOG Galaxy's registry where both know a game.
fn known_installs() -> HashMap<String, KnownInstall> {
    let mut all: HashMap<String, KnownInstall> = discovery::heroic_config_dirs()
        .iter()
        .filter_map(|d| discovery::read_capped(&d.join("gog_store/installed.json")))
        .flat_map(|json| parse_heroic(&json))
        .collect();
    all.extend(registry_installs());
    all
}

#[cfg(windows)]
fn registry_installs() -> HashMap<String, KnownInstall> {
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
            let entry = KnownInstall { path: get("path")?, launcher: "galaxy", version: get("ver"), build_id: get("buildId") };
            Some((id, entry))
        })
        .filter(|(_, g)| !discovery::is_network_path(&g.path))
        .collect()
}

#[cfg(not(windows))]
fn registry_installs() -> HashMap<String, KnownInstall> {
    HashMap::new()
}

/// Heroic's `gog_store/installed.json`. Field names are Heroic's own mix of camel and snake case.
#[derive(Deserialize)]
struct HeroicInstalled {
    installed: Vec<HeroicGame>,
}

#[derive(Deserialize)]
struct HeroicGame {
    #[serde(rename = "appName")]
    app_name: String,
    install_path: String,
    version: Option<String>,
    #[serde(rename = "buildId")]
    build_id: Option<String>,
    #[serde(default)]
    is_dlc: bool,
}

fn parse_heroic(json: &str) -> Vec<(String, KnownInstall)> {
    let Ok(file) = serde_json::from_str::<HeroicInstalled>(json) else { return vec![] };
    file.installed
        .into_iter()
        .filter(|g| !g.is_dlc && !discovery::is_network_path(&g.install_path))
        .map(|g| {
            let nonempty = |v: Option<String>| v.filter(|s| !s.is_empty());
            let entry = KnownInstall {
                path: g.install_path,
                launcher: "heroic",
                version: nonempty(g.version),
                build_id: nonempty(g.build_id),
            };
            (g.app_name, entry)
        })
        .collect()
}

pub fn scan(locations: &[PathBuf]) -> ScanResult {
    scan_with(&known_installs(), &default_roots(), locations)
}

fn scan_with(registry: &HashMap<String, KnownInstall>, defaults: &[PathBuf], locations: &[PathBuf]) -> ScanResult {
    let mut game_dirs: Vec<PathBuf> = registry.values().map(|g| PathBuf::from(&g.path)).collect();
    let mut libraries = vec![];
    for root in defaults.iter().chain(locations) {
        // An unmounted mount point is an empty folder: it must not count as "scanned", or its games look uninstalled.
        if fs::read_dir(root).is_ok_and(|mut d| d.next().is_some()) {
            libraries.push(canon(root));
            discovery::find_dirs(root, discovery::LOCATION_SEARCH_DEPTH, &|d: &Path| !info_files(d).is_empty(), &mut game_dirs);
        }
    }
    // ponytail: a registry-only install outside every searched folder that gets uninstalled stays listed as offline;
    // fix with the 'offline' UI state or by reconciling against the registry.
    let dirs: BTreeSet<PathBuf> = game_dirs.iter().map(|d| canon(d)).collect();
    let mut games = vec![];
    let mut complete = true;
    for dir in &dirs {
        let (found, ok) = read_game_dir(dir, registry);
        games.extend(found);
        complete &= ok;
    }
    // An info file GOG is rewriting can fail to parse; never treat its game as uninstalled.
    let reconcile = if complete { Reconcile::ByLocation } else { Reconcile::AddOnly };
    ScanResult { games, libraries, reconcile }
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

/// Base games in one folder (DLC info files are skipped). Version precedence: launcher version,
/// then a build id (registry or info file), else unknown.
/// The flag is false if any info file in the folder could not be read or parsed.
fn read_game_dir(dir: &Path, registry: &HashMap<String, KnownInstall>) -> (Vec<DiscoveredGame>, bool) {
    let parsed: Vec<Option<GogInfo>> =
        info_files(dir).iter().map(|p| serde_json::from_str::<GogInfo>(&discovery::read_capped(p)?).ok()).collect();
    let complete = parsed.iter().all(Option::is_some);
    let infos: Vec<_> = parsed
        .into_iter()
        .flatten()
        .map(|i| {
            let exe = i.play_tasks.into_iter().find(|t| t.is_primary).and_then(|t| t.path);
            (String::from(i.game_id), i.root_game_id.map(String::from), i.name, i.build_id.map(String::from), exe)
        })
        .collect();
    // An info file whose rootGameId is another game's id is that game's installed DLC.
    let dlc_of = |base: &str| -> Vec<Dlc> {
        infos
            .iter()
            .filter(|(id, root, ..)| root.as_deref() == Some(base) && id != base)
            .map(|(id, _, name, ..)| Dlc { id: id.clone(), name: Some(name.clone()) })
            .collect()
    };
    let games = infos
        .iter()
        .filter(|(id, root, ..)| root.as_ref().is_none_or(|r| r == id))
        .map(|(game_id, _, name, info_build_id, exe)| {
            let (game_id, name, info_build_id, exe) = (game_id.clone(), name.clone(), info_build_id.clone(), exe.clone());
            let exe_version = exe.and_then(|e| discovery::exe_version(dir, &e));
            let reg = registry.get(&game_id);
            let build_id = reg.and_then(|r| r.build_id.clone()).or(info_build_id).filter(|b| !b.is_empty());
            let (version, version_source) = match (reg.and_then(|r| r.version.clone()), build_id) {
                (Some(v), _) => (Some(v), Some("gog_version")),
                (None, Some(b)) => (Some(b), Some("gog_buildid")),
                (None, None) => (None, None),
            };
            DiscoveredGame {
                name,
                path: dir.to_path_buf(),
                version,
                version_source,
                update_available: None,
                size_bytes: None,
                last_updated: None,
                launcher: reg.map(|r| r.launcher),
                exe_version,
                dlc: dlc_of(&game_id),
                source_game_id: game_id,
            }
        })
        .collect();
    (games, complete)
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
        write(
            "GOG Games/Game B/goggame-3.info",
            r#"{"gameId":3,"name":"Game B","version":1,"playTasks":[{"isPrimary":false,"path":"tools\\cfg.exe"},{"isPrimary":true,"path":"bin\\game.exe"}]}"#,
        );
        fs::create_dir_all(tmp.join("GOG Games/Game B/bin")).unwrap();
        fs::write(tmp.join("GOG Games/Game B/bin/game.exe"), crate::pe::fake_exe([2, 5, 0, 0])).unwrap();
        write("GOG Games/Game B/goggame-6.info", r#"{"gameId":6,"rootGameId":3,"name":"Game B DLC","version":1}"#);
        // Found only through the registry, outside every searched folder; registry has a build id but no ver.
        write("Elsewhere/Game C/goggame-5.info", r#"{"gameId":"5","name":"Game C","version":1}"#);
        // A symlinked info file is never followed (would otherwise list game 1 a second time under Game B).
        #[cfg(unix)]
        std::os::unix::fs::symlink(tmp.join("GOG Games/Game A/goggame-1.info"), tmp.join("GOG Games/Game B/goggame-7.info"))
            .unwrap();

        let registry = HashMap::from([
            ("1".to_string(), KnownInstall {
                path: root.join("Game A").to_string_lossy().into(),
                launcher: "galaxy",
                version: Some("1.32".into()),
                build_id: None,
            }),
            ("5".to_string(), KnownInstall {
                path: tmp.join("Elsewhere/Game C").to_string_lossy().into(),
                launcher: "heroic",
                version: None,
                build_id: Some("777".into()),
            }),
        ]);

        fs::create_dir_all(tmp.join("Unmounted")).unwrap();
        let result = scan_with(&registry, &[root.clone(), tmp.join("missing"), tmp.join("Unmounted")], &[]);
        let mut games = result.games;
        games.sort_by(|a, b| a.source_game_id.cmp(&b.source_game_id));
        let root_canon = canon(&root);

        // A half-written info file: games still found, but nothing may be removed this time.
        write("GOG Games/Broken/goggame-4.info", "{ not json");
        let partial = scan_with(&registry, std::slice::from_ref(&root), &[]);
        fs::remove_dir_all(&tmp).ok();
        assert_eq!(result.reconcile, Reconcile::ByLocation);
        assert_eq!(partial.reconcile, Reconcile::AddOnly);
        assert_eq!(partial.games.len(), 3);

        let summary: Vec<_> = games.iter().map(|g| (g.source_game_id.as_str(), g.version.as_deref(), g.version_source)).collect();
        assert_eq!(
            summary,
            vec![
                ("1", Some("1.32"), Some("gog_version")),
                ("3", None, None),
                ("5", Some("777"), Some("gog_buildid")),
            ],
            "DLC skipped; registry folder not duplicated with the searched one"
        );
        assert_eq!(games[1].exe_version.as_deref(), Some("2.5"), "primary play task's executable, Windows separators");
        assert_eq!(games[0].dlc, vec![Dlc { id: "2".into(), name: Some("Game A DLC".into()) }]);
        assert_eq!(games[1].dlc, vec![Dlc { id: "6".into(), name: Some("Game B DLC".into()) }], "numeric ids too");
        assert!(games[2].dlc.is_empty());
        assert_eq!(games[0].exe_version, None, "no play task, no executable version");
        let launchers: Vec<_> = games.iter().map(|g| g.launcher).collect();
        assert_eq!(launchers, vec![Some("galaxy"), None, Some("heroic")], "offline-installer games have no launcher");
        assert!(games.iter().all(|g| g.update_available.is_none()), "GOG never claims an update state");
        assert_eq!(result.libraries, vec![root_canon], "only readable, non-empty roots count as scanned");
    }

    #[test]
    fn parses_heroic_installed_list() {
        let json = r#"{"installed":[
            {"appName":"1","install_path":"/home/u/Games/Heroic/A","version":"1.0.4","buildId":"9","is_dlc":false,"platform":"windows"},
            {"appName":"2","install_path":"/home/u/Games/Heroic/A","version":"1.0","is_dlc":true},
            {"appName":"3","install_path":"//evil/share","version":"1.0"},
            {"appName":"4","install_path":"/home/u/Games/Heroic/B","version":""}
        ]}"#;
        let parsed: Vec<_> = parse_heroic(json).into_iter().map(|(id, k)| (id, k.version)).collect();
        assert_eq!(parsed, vec![("1".into(), Some("1.0.4".into())), ("4".into(), None)], "DLC, network paths and empty versions dropped");
        assert!(parse_heroic("not json").is_empty());
    }
}
