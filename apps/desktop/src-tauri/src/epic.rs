//! Epic Games source: the Epic Games Launcher's install manifests (Windows), plus Legendary's
//! `installed.json` as used by Heroic and the Legendary CLI (mostly Linux). Read-only.
//!
//! Both are complete install lists, so a scan that read everything present is `Reconcile::Complete`:
//! a game no longer listed is uninstalled. Any unreadable file makes it `AddOnly`, so a half-written
//! manifest never removes a game. Epic's local metadata has no pending-update flag, so update state is unknown.

use crate::discovery::{self, DiscoveredGame, Reconcile, ScanResult, canon};
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
}

fn launcher_manifest_dir() -> Option<PathBuf> {
    std::env::var_os("PROGRAMDATA").map(|p| PathBuf::from(p).join(r"Epic\EpicGamesLauncher\Data\Manifests"))
}

fn legendary_files() -> Vec<PathBuf> {
    let home = std::env::home_dir().unwrap_or_default();
    let mut files: Vec<PathBuf> =
        discovery::heroic_config_dirs().iter().map(|d| d.join("legendaryConfig/legendary/installed.json")).collect();
    files.push(home.join(".config/legendary/installed.json"));
    files
}

pub fn scan() -> ScanResult {
    scan_from(launcher_manifest_dir().as_deref(), &legendary_files())
}

fn scan_from(manifest_dir: Option<&Path>, legendary: &[PathBuf]) -> ScanResult {
    // Keyed by app name so the same game seen by the launcher and by Legendary is listed once.
    let mut games: HashMap<String, DiscoveredGame> = HashMap::new();
    let mut read_any = false;
    let mut complete = true;

    for file in legendary.iter().filter(|f| f.exists()) {
        let parsed = discovery::read_capped(file).and_then(|j| serde_json::from_str::<HashMap<String, LegendaryEntry>>(&j).ok());
        let Some(entries) = parsed else {
            complete = false;
            continue;
        };
        read_any = true;
        for e in entries.into_values().filter(|e| !e.is_dlc && !discovery::is_network_path(&e.install_path)) {
            let game = game(e.app_name, e.title, &e.install_path, e.version, "legendary", e.install_size);
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
            if is_dlc || m.incomplete || discovery::is_network_path(&m.install_location) {
                continue;
            }
            // The official launcher wins over Legendary when both list the same game.
            let game = game(m.app_name, m.display_name, &m.install_location, m.app_version, "epic_manifest", m.install_size);
            games.insert(game.source_game_id.clone(), game);
        }
    }

    let reconcile = if read_any && complete { Reconcile::Complete } else { Reconcile::AddOnly };
    ScanResult { games: games.into_values().collect(), libraries: vec![], reconcile }
}

fn game(
    id: String,
    name: String,
    path: &str,
    version: Option<String>,
    version_source: &'static str,
    size: Option<i64>,
) -> DiscoveredGame {
    let version = version.filter(|v| !v.is_empty());
    DiscoveredGame {
        source_game_id: id,
        name,
        path: canon(Path::new(path)),
        version_source: version.as_ref().map(|_| version_source),
        version,
        update_available: None,
        size_bytes: size.filter(|s| *s > 0),
        last_updated: None,
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
        item("b.item", r#"{"DisplayName":"Game B","InstallLocation":"C:\\Games\\B","AppName":"b","bIsIncompleteInstall":true}"#);
        item("notes.txt", r#"{"DisplayName":"Ignored","InstallLocation":"x","AppName":"x"}"#);

        let legendary = tmp.join("installed.json");
        fs::write(
            &legendary,
            r#"{
              "a": {"app_name":"a","title":"Game A (Legendary)","install_path":"/g/A","version":"1.2.2","is_dlc":false},
              "c": {"app_name":"c","title":"Game C","install_path":"/g/C","version":"","is_dlc":false,"install_size":0},
              "cdlc": {"app_name":"cdlc","title":"C DLC","install_path":"/g/C","is_dlc":true}
            }"#,
        )
        .unwrap();

        let result = scan_from(Some(&manifests), &[legendary, tmp.join("missing.json")]);
        fs::remove_dir_all(&tmp).ok();

        let mut games: Vec<_> =
            result.games.iter().map(|g| (g.source_game_id.as_str(), g.name.as_str(), g.version.as_deref(), g.version_source)).collect();
        games.sort();
        assert_eq!(
            games,
            vec![("a", "Game A", Some("1.2.3"), Some("epic_manifest")), ("c", "Game C", None, None)],
            "DLC and incomplete installs skipped, launcher wins over Legendary, empty version is unknown"
        );
        assert_eq!(result.reconcile, Reconcile::Complete);
        assert!(result.games.iter().all(|g| g.update_available.is_none()));

        // A half-written manifest: games still listed, but nothing may be removed this time.
        fs::create_dir_all(&manifests).unwrap();
        item("a.item", r#"{"DisplayName":"Game A","InstallLocation":"C:\\Games\\A","AppName":"a"}"#);
        item("bad.item", "{ nope");
        let partial = scan_from(Some(&manifests), &[]);
        fs::remove_dir_all(&tmp).ok();
        assert_eq!(partial.games.len(), 1);
        assert_eq!(partial.reconcile, Reconcile::AddOnly);
    }

    #[test]
    fn nothing_readable_is_not_authoritative() {
        let result = scan_from(Some(Path::new("/definitely/missing")), &[PathBuf::from("/missing.json")]);
        assert!(result.games.is_empty());
        assert_eq!(result.reconcile, Reconcile::AddOnly, "absent launcher must not wipe stored Epic games");
    }
}
