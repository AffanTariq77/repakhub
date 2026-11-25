//! Steam source: reads Steam's own library metadata. Read-only, never modifies Steam files.

use crate::discovery::{self, DiscoveredGame, Reconcile, ScanResult, canon, is_network_path};
use crate::vdf::{self, Vdf};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Steam `EAppState` flag: an update is pending for this app.
const STATE_UPDATE_REQUIRED: i64 = 2;

/// Steam install roots: where Steam says it is (Windows registry), plus the default folders per OS.
/// Steam installed anywhere else is found through the user's scan locations.
fn steam_roots() -> Vec<PathBuf> {
    let home = std::env::home_dir().unwrap_or_default();
    let candidates: Vec<PathBuf> = if cfg!(windows) {
        let mut v = registry_steam_paths();
        v.extend([r"C:\Program Files (x86)\Steam".into(), r"C:\Program Files\Steam".into()]);
        v
    } else if cfg!(target_os = "macos") {
        vec![home.join("Library/Application Support/Steam")]
    } else {
        vec![
            home.join(".steam/steam"),
            home.join(".local/share/Steam"),
            home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"),
        ]
    };
    candidates.into_iter().filter(|p| p.is_dir()).collect()
}

/// Steam records its install folder in the registry (per user, and machine-wide by the installer).
/// Values look like `c:/program files (x86)/steam`; `canon` later normalises case and separators,
/// so duplicates of the default folders collapse in `library_paths`.
#[cfg(windows)]
fn registry_steam_paths() -> Vec<PathBuf> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    [
        (HKEY_CURRENT_USER, r"Software\Valve\Steam", "SteamPath"),
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Valve\Steam", "InstallPath"),
    ]
    .into_iter()
    .filter_map(|(hive, key, value)| RegKey::predef(hive).open_subkey(key).ok()?.get_value::<String, _>(value).ok())
    .filter(|p| !is_network_path(p)) // an unreachable share would stall the scan
    .map(PathBuf::from)
    .collect()
}

#[cfg(not(windows))]
fn registry_steam_paths() -> Vec<PathBuf> {
    vec![]
}

/// Scan default Steam roots plus any Steam folders found inside the user's chosen locations.
pub fn scan(locations: &[PathBuf]) -> ScanResult {
    let mut roots = steam_roots();
    for loc in locations {
        discovery::find_dirs(loc, discovery::LOCATION_SEARCH_DEPTH, &|d: &Path| d.join("steamapps").is_dir(), &mut roots);
    }
    let mut result = ScanResult { games: vec![], libraries: vec![], reconcile: Reconcile::ByLocation };
    for lib in library_paths(&roots) {
        if let Some(games) = scan_library(&lib) {
            result.games.extend(games);
            result.libraries.push(lib);
        }
    }
    result
}

/// All library folders, deduplicated (Linux roots are often symlinks to each other).
fn library_paths(roots: &[PathBuf]) -> BTreeSet<PathBuf> {
    let mut libs = BTreeSet::new();
    for root in roots {
        libs.insert(canon(root));
        let Some(v) = read_vdf(&root.join("steamapps/libraryfolders.vdf")) else { continue };
        let Some(Vdf::Obj(folders)) = v.get("libraryfolders") else { continue };
        for (key, entry) in folders {
            // New format: "0" { "path" "..." }. Old format: "1" "D:\\SteamLibrary".
            let path = match entry {
                Vdf::Obj(_) => entry.str("path"),
                Vdf::Str(p) if key.parse::<u32>().is_ok() => Some(p.as_str()),
                Vdf::Str(_) => None,
            };
            if let Some(p) = path.filter(|p| !is_network_path(p)) {
                libs.insert(canon(Path::new(p)));
            }
        }
    }
    libs
}

/// `None` if the library could not be read (drive offline, permissions), as opposed to an empty library.
fn scan_library(lib: &Path) -> Option<Vec<DiscoveredGame>> {
    let entries = fs::read_dir(lib.join("steamapps")).ok()?;
    let games = entries
        .flatten()
        .filter(|e| {
            let n = e.file_name();
            let n = n.to_string_lossy();
            n.starts_with("appmanifest_") && n.ends_with(".acf")
        })
        .filter_map(|e| read_vdf(&e.path()))
        .filter_map(|v| parse_manifest(lib, &v))
        .collect();
    Some(games)
}

fn parse_manifest(lib: &Path, v: &Vdf) -> Option<DiscoveredGame> {
    let s = v.get("appstate")?;
    let num = |k| s.str(k).and_then(|x| x.parse::<i64>().ok());
    let build_id = s.str("buildid").filter(|b| !b.is_empty() && *b != "0").map(String::from);
    let (app_id, installdir) = (s.str("appid")?, s.str("installdir")?);
    if is_steam_tool(app_id, installdir) {
        return None;
    }
    Some(DiscoveredGame {
        source_game_id: app_id.to_string(),
        name: s.str("name")?.to_string(),
        path: lib.join("steamapps/common").join(installdir),
        version_source: build_id.as_ref().map(|_| "steam_buildid"),
        version: build_id,
        update_available: Some(num("stateflags").is_some_and(|f| f & STATE_UPDATE_REQUIRED != 0)),
        size_bytes: num("sizeondisk"),
        last_updated: num("lastupdated"),
    })
}

/// Steam installs its own tools (Proton, runtimes, redistributables) as apps. Matched by app id, never
/// by display name, so a game that happens to be called "Proton ..." is never hidden.
// ponytail: fixed list; a new Proton release shows in the library until its id is added (the safe direction).
const STEAM_TOOL_APP_IDS: &[&str] = &[
    "228980", // Steamworks Common Redistributables
    "858280", "930400", "961940", "1054830", "1113280", "1245040", "1420170", "1580130", "1887720", "2348590",
    "2805730", // Proton releases
    "1493710", // Proton Experimental
    "2180100", // Proton Hotfix
    "1161040", "1826330", // Proton BattlEye / EasyAntiCheat runtimes
    "1070560", "1391110", "1628350", // Steam Linux Runtime 1.0 / 2.0 / 3.0
];

fn is_steam_tool(app_id: &str, installdir: &str) -> bool {
    STEAM_TOOL_APP_IDS.contains(&app_id) || installdir.starts_with("SteamLinuxRuntime")
}

fn read_vdf(path: &Path) -> Option<Vdf> {
    vdf::parse(&discovery::read_capped(path)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_fake_steam_root_with_two_libraries() {
        let tmp = std::env::temp_dir().join(format!("repakhub-steam-test-{}", std::process::id()));
        let root = tmp.join("Steam");
        let extra = tmp.join("ExtraLib");
        fs::create_dir_all(root.join("steamapps")).unwrap();
        fs::create_dir_all(extra.join("steamapps")).unwrap();

        let extra_escaped = extra.display().to_string().replace('\\', "\\\\");
        fs::write(
            root.join("steamapps/libraryfolders.vdf"),
            format!(
                r#""libraryfolders" {{ "0" {{ "path" "{extra_escaped}" }} "1" {{ "path" "\\\\evil-host\\share" }} "2" "//evil-host/share" }}"#
            ),
        )
        .unwrap();
        fs::write(
            root.join("steamapps/appmanifest_10.acf"),
            r#""AppState" { "appid" "10" "name" "Game A" "installdir" "GameA"
               "buildid" "1234" "StateFlags" "6" "SizeOnDisk" "500" }"#,
        )
        .unwrap();
        fs::write(
            extra.join("steamapps/appmanifest_20.acf"),
            r#""AppState" { "appid" "20" "name" "Game B" "installdir" "GameB" "buildid" "0" "StateFlags" "4" }"#,
        )
        .unwrap();
        fs::write(extra.join("steamapps/appmanifest_30.acf"), "garbage {").unwrap();
        let tools = [
            ("1493710", "Proton Experimental", "Proton - Experimental"),
            ("4000000", "Steam Linux Runtime 4.0", "SteamLinuxRuntime_future"),
            ("228980", "Steamworks Common Redistributables", "Steamworks Shared"),
            ("99", "Proton Rush", "Proton Rush"), // a real game whose name happens to start with "Proton"
        ];
        for (id, name, dir) in tools {
            fs::write(
                extra.join(format!("steamapps/appmanifest_{id}.acf")),
                format!(r#""AppState" {{ "appid" "{id}" "name" "{name}" "installdir" "{dir}" }}"#),
            )
            .unwrap();
        }

        let libs = library_paths(&[root]);
        let mut games: Vec<_> = libs.iter().filter_map(|l| scan_library(l)).flatten().collect();
        games.sort_by(|a, b| a.source_game_id.cmp(&b.source_game_id));
        fs::remove_dir_all(&tmp).ok();

        assert_eq!(libs.len(), 2, "network paths from libraryfolders.vdf are never touched");
        assert!(libs.iter().all(|l| !l.to_string_lossy().contains("evil-host")));
        assert_eq!(scan_library(&tmp.join("gone")), None, "unreadable library is None, not empty");

        assert_eq!(games.len(), 3, "malformed manifest and Steam tools skipped, game named Proton kept");
        assert_eq!(games[2].name, "Proton Rush");
        assert_eq!(games[0].name, "Game A");
        assert_eq!(games[0].version.as_deref(), Some("1234"));
        assert_eq!(games[0].version_source, Some("steam_buildid"));
        assert_eq!(games[0].update_available, Some(true));
        assert_eq!(games[0].size_bytes, Some(500));
        assert!(games[0].path.ends_with("steamapps/common/GameA"));
        assert_eq!(games[1].version, None, "buildid 0 means unknown, not version 0");
        assert_eq!(games[1].version_source, None);
        assert_eq!(games[1].update_available, Some(false));
    }
}
