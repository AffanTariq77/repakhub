//! Epic Games source: the Epic Games Launcher's install manifests (Windows), plus Legendary's
//! `installed.json` as used by Heroic and the Legendary CLI (mostly Linux). Read-only.
//!
//! Both are complete install lists, so a scan that read everything present is `Reconcile::Complete`:
//! a game no longer listed is uninstalled. Any unreadable file makes it `AddOnly`, so a half-written
//! manifest never removes a game. Epic's local metadata has no pending-update flag, so update state is unknown.

use crate::discovery::{self, DiscoveredGame, Dlc, Reconcile, ScanResult, canon};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// One `<id>.item` file in the launcher's `Manifests` folder.
#[derive(Deserialize)]
struct LauncherManifest {
    #[serde(rename = "DisplayName")]
    display_name: String,
    #[serde(rename = "InstallLocation")]
    install_location: String,
    #[serde(rename = "AppName")]
    app_name: String,
    #[serde(rename = "AppVersionString")]
    app_version: Option<String>,
    /// DLC point at their base game here. Equal to `AppName` on the game itself.
    #[serde(rename = "MainGameAppName")]
    main_game_app_name: Option<String>,
    #[serde(rename = "bIsIncompleteInstall", default)]
    incomplete: bool,
    #[serde(rename = "InstallSize")]
    install_size: Option<i64>,
    /// Main executable, relative to the install folder.
    #[serde(rename = "LaunchExecutable")]
    launch_executable: Option<String>,
}

/// One value in Legendary's `installed.json` (a map of app name to entry).
#[derive(Deserialize)]
struct LegendaryEntry {
    app_name: String,
    title: String,
    install_path: String,
    version: Option<String>,
    #[serde(default)]
    is_dlc: bool,
    install_size: Option<i64>,
    /// Main executable, relative to the install folder.
    executable: Option<String>,
}

fn launcher_manifest_dir() -> Option<PathBuf> {
    std::env::var_os("PROGRAMDATA").map(|p| PathBuf::from(p).join(r"Epic\EpicGamesLauncher\Data\Manifests"))
}

/// Legendary install lists, with the app that manages those installs.
fn legendary_files() -> Vec<(PathBuf, &'static str)> {
    let home = std::env::home_dir().unwrap_or_default();
    let mut files: Vec<(PathBuf, &'static str)> = discovery::heroic_config_dirs()
        .iter()
        .map(|d| (d.join("legendaryConfig/legendary/installed.json"), "heroic"))
        .collect();
    files.push((home.join(".config/legendary/installed.json"), "legendary"));
    files
}

pub fn scan() -> ScanResult {
    scan_from(launcher_manifest_dir().as_deref(), &legendary_files())
}

fn scan_from(manifest_dir: Option<&Path>, legendary: &[(PathBuf, &'static str)]) -> ScanResult {
    // Keyed by app name so the same game seen by the launcher and by Legendary is listed once.
    let mut games: HashMap<String, DiscoveredGame> = HashMap::new();
    let mut read_any = false;
    let mut complete = true;
    // DLC are attached to their base game at the end: by app name (launcher), or by install folder (Legendary).
    let mut dlc_by_game: Vec<(String, Dlc)> = vec![];
    let mut dlc_by_folder: Vec<(PathBuf, Dlc)> = vec![];

    for (file, launcher) in legendary.iter().filter(|(f, _)| f.exists()) {
        let parsed = discovery::read_capped(file).and_then(|j| serde_json::from_str::<HashMap<String, LegendaryEntry>>(&j).ok());
        let Some(entries) = parsed else {
            complete = false;
            continue;
        };
        read_any = true;
        for e in entries.values().filter(|e| e.is_dlc && !discovery::is_network_path(&e.install_path)) {
            dlc_by_folder.push((canon(Path::new(&e.install_path)), Dlc { id: e.app_name.clone(), name: Some(e.title.clone()) }));
        }
        for e in entries.into_values().filter(|e| !e.is_dlc && !discovery::is_network_path(&e.install_path)) {
            let source = ("legendary", *launcher);
            let game = game(e.app_name, e.title, &e.install_path, e.version, source, e.install_size, e.executable.as_deref());
            games.insert(game.source_game_id.clone(), game);
        }
    }

    if let Some(Ok(entries)) = manifest_dir.map(fs::read_dir) {
        read_any = true;
        let mut manifests = vec![];
        for entry in entries {
            let Ok(e) = entry else {
                complete = false;
                continue;
            };
            if !(e.file_type().is_ok_and(|t| t.is_file()) && e.path().extension().is_some_and(|x| x == "item")) {
                continue;
            }
            match discovery::read_capped(&e.path()).and_then(|j| serde_json::from_str::<LauncherManifest>(&j).ok()) {
                Some(m) => manifests.push(m),
                None => complete = false,
            }
        }
        for m in manifests {
            let is_dlc = m.main_game_app_name.as_ref().is_some_and(|main| *main != m.app_name);
            if m.incomplete {
                // Installing or updating right now: keep whatever is stored rather than calling it uninstalled.
                complete = false;
                continue;
            }
            if is_dlc {
                let base = m.main_game_app_name.clone().unwrap_or_default();
                dlc_by_game.push((base, Dlc { id: m.app_name, name: Some(m.display_name) }));
                continue;
            }
            if discovery::is_network_path(&m.install_location) {
                continue;
            }
            // The official launcher wins over Legendary when both list the same game.
            let exe = m.launch_executable.as_deref();
            let game =
                game(m.app_name, m.display_name, &m.install_location, m.app_version, ("epic_manifest", "epic"), m.install_size, exe);
            games.insert(game.source_game_id.clone(), game);
        }
    }

    for game in games.values_mut() {
        let by_game = dlc_by_game.iter().filter(|(base, _)| *base == game.source_game_id).map(|(_, d)| d);
        let by_folder = dlc_by_folder.iter().filter(|(dir, _)| *dir == game.path).map(|(_, d)| d);
        for d in by_game.chain(by_folder) {
            if !game.dlc.iter().any(|x| x.id == d.id) {
                game.dlc.push(d.clone());
            }
        }
        game.dlc.sort_by(|a, b| a.id.cmp(&b.id));
    }
    let reconcile = if read_any && complete { Reconcile::Complete } else { Reconcile::AddOnly };
    ScanResult { games: games.into_values().collect(), libraries: vec![], reconcile }
}

/// `(version_source, launcher)` say where the entry came from; `exe` is the main executable, relative to `path`.
fn game(
    id: String,
    name: String,
    path: &str,
    version: Option<String>,
    (version_source, launcher): (&'static str, &'static str),
    size: Option<i64>,
    exe: Option<&str>,
) -> DiscoveredGame {
    let version = version.filter(|v| !v.is_empty());
    let path = canon(Path::new(path));
    DiscoveredGame {
        source_game_id: id,
        name,
        exe_version: exe.and_then(|e| discovery::exe_version(&path, e)),
        path,
        version_source: version.as_ref().map(|_| version_source),
        version,
        update_available: None,
        size_bytes: size.filter(|s| *s > 0),
        last_updated: None,
        launcher: Some(launcher),
        dlc: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_launcher_manifests_and_legendary_skipping_dlc() {
        let tmp = std::env::temp_dir().join(format!("repakhub-epic-test-{}", std::process::id()));
        let manifests = tmp.join("Manifests");
        fs::create_dir_all(&manifests).unwrap();
        let item = |file: &str, body: &str| fs::write(manifests.join(file), body).unwrap();
        item("a.item", r#"{"DisplayName":"Game A","InstallLocation":"C:\\Games\\A","AppName":"a","MainGameAppName":"a","AppVersionString":"1.2.3","InstallSize":100}"#);
        item("a-dlc.item", r#"{"DisplayName":"A DLC","InstallLocation":"C:\\Games\\A","AppName":"adlc","MainGameAppName":"a"}"#);
        item("notes.txt", r#"{"DisplayName":"Ignored","InstallLocation":"x","AppName":"x"}"#);

        let install_d = tmp.join("D");
        fs::create_dir_all(install_d.join("Bin")).unwrap();
        fs::write(install_d.join("Bin/d.exe"), crate::pe::fake_exe([3, 1, 4, 0])).unwrap();
        fs::write(tmp.join("outside.exe"), crate::pe::fake_exe([9, 9, 0, 0])).unwrap();
        let legendary = tmp.join("installed.json");
        let install_d_json = install_d.display().to_string().replace('\\', "\\\\");
        fs::write(
            &legendary,
            r#"{
              "a": {"app_name":"a","title":"Game A (Legendary)","install_path":"/g/A","version":"1.2.2","is_dlc":false},
              "c": {"app_name":"c","title":"Game C","install_path":"/g/C","version":"","is_dlc":false,"install_size":0},
              "d": {"app_name":"d","title":"Game D","install_path":"INSTALL_D","executable":"Bin/d.exe","is_dlc":false},
              "e": {"app_name":"e","title":"Game E","install_path":"INSTALL_D","executable":"../outside.exe","is_dlc":false},
              "cdlc": {"app_name":"cdlc","title":"C DLC","install_path":"/g/C","is_dlc":true}
            }"#
            .replace("INSTALL_D", &install_d_json),
        )
        .unwrap();

        let result = scan_from(Some(&manifests), &[(legendary, "heroic"), (tmp.join("missing.json"), "legendary")]);
        fs::remove_dir_all(&tmp).ok();

        let mut games: Vec<_> =
            result.games.iter().map(|g| (g.source_game_id.as_str(), g.name.as_str(), g.version.as_deref(), g.version_source)).collect();
        games.sort();
        assert_eq!(
            games,
            vec![
                ("a", "Game A", Some("1.2.3"), Some("epic_manifest")),
                ("c", "Game C", None, None),
                ("d", "Game D", None, None),
                ("e", "Game E", None, None),
            ],
            "DLC skipped, launcher wins over Legendary, empty version is unknown"
        );
        assert_eq!(result.reconcile, Reconcile::Complete);
        assert!(result.games.iter().all(|g| g.update_available.is_none()));
        let launcher = |id: &str| result.games.iter().find(|g| g.source_game_id == id).unwrap().launcher;
        let exe = |id: &str| result.games.iter().find(|g| g.source_game_id == id).unwrap().exe_version.clone();
        assert_eq!(exe("d").as_deref(), Some("3.1.4"));
        assert_eq!(exe("e"), None, "an executable path escaping the install folder is refused");
        assert_eq!((launcher("a"), launcher("c")), (Some("epic"), Some("heroic")));
        let dlc = |id: &str| result.games.iter().find(|g| g.source_game_id == id).unwrap().dlc.clone();
        assert_eq!(dlc("a"), vec![Dlc { id: "adlc".into(), name: Some("A DLC".into()) }], "launcher DLC by MainGameAppName");
        assert_eq!(dlc("c"), vec![Dlc { id: "cdlc".into(), name: Some("C DLC".into()) }], "Legendary DLC by install folder");

        // A half-written manifest: games still listed, but nothing may be removed this time.
        fs::create_dir_all(&manifests).unwrap();
        item("a.item", r#"{"DisplayName":"Game A","InstallLocation":"C:\\Games\\A","AppName":"a"}"#);
        item("bad.item", "{ nope");
        let partial = scan_from(Some(&manifests), &[]);
        assert_eq!(partial.games.len(), 1);
        assert_eq!(partial.reconcile, Reconcile::AddOnly);

        // A game mid-update (incomplete install) is not listed, and nothing may be removed.
        fs::remove_file(manifests.join("bad.item")).unwrap();
        item("b.item", r#"{"DisplayName":"Game B","InstallLocation":"C:\\Games\\B","AppName":"b","bIsIncompleteInstall":true}"#);
        let updating = scan_from(Some(&manifests), &[]);
        fs::remove_dir_all(&tmp).ok();
        assert_eq!(updating.games.len(), 1);
        assert_eq!(updating.reconcile, Reconcile::AddOnly);
    }

    #[test]
    fn nothing_readable_is_not_authoritative() {
        let result = scan_from(Some(Path::new("/definitely/missing")), &[(PathBuf::from("/missing.json"), "legendary")]);
        assert!(result.games.is_empty());
        assert_eq!(result.reconcile, Reconcile::AddOnly, "absent launcher must not wipe stored Epic games");
    }
}
