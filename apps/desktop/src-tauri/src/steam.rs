//! Steam source: reads Steam's own library metadata. Read-only, never modifies Steam files.

use crate::vdf::{self, Vdf};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Steam `EAppState` flag: an update is pending for this app.
const STATE_UPDATE_REQUIRED: i64 = 2;

#[derive(Debug, PartialEq)]
pub struct SteamGame {
    pub app_id: String,
    pub name: String,
    pub path: PathBuf,
    pub build_id: Option<String>,
    pub size_bytes: Option<i64>,
    pub update_required: bool,
    pub last_updated: Option<i64>,
}

/// Default Steam install roots per OS.
// ponytail: fixed defaults only; read the Windows registry (SteamPath) or let users add a root when someone's Steam lives elsewhere.
fn steam_roots() -> Vec<PathBuf> {
    let home = std::env::home_dir().unwrap_or_default();
    let candidates: Vec<PathBuf> = if cfg!(windows) {
        vec![r"C:\Program Files (x86)\Steam".into(), r"C:\Program Files\Steam".into()]
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

pub fn scan() -> Vec<SteamGame> {
    library_paths(&steam_roots()).iter().flat_map(|l| scan_library(l)).collect()
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
            if let Some(p) = path {
                libs.insert(canon(Path::new(p)));
            }
        }
    }
    libs
}

fn scan_library(lib: &Path) -> Vec<SteamGame> {
    let Ok(entries) = fs::read_dir(lib.join("steamapps")) else { return vec![] };
    entries
        .flatten()
        .filter(|e| {
            let n = e.file_name();
            let n = n.to_string_lossy();
            n.starts_with("appmanifest_") && n.ends_with(".acf")
        })
        .filter_map(|e| read_vdf(&e.path()))
        .filter_map(|v| parse_manifest(lib, &v))
        .collect()
}

fn parse_manifest(lib: &Path, v: &Vdf) -> Option<SteamGame> {
    let s = v.get("appstate")?;
    let num = |k| s.str(k).and_then(|x| x.parse::<i64>().ok());
    Some(SteamGame {
        app_id: s.str("appid")?.to_string(),
        name: s.str("name")?.to_string(),
        path: lib.join("steamapps/common").join(s.str("installdir")?),
        build_id: s.str("buildid").filter(|b| !b.is_empty() && *b != "0").map(String::from),
        size_bytes: num("sizeondisk"),
        update_required: num("stateflags").is_some_and(|f| f & STATE_UPDATE_REQUIRED != 0),
        last_updated: num("lastupdated"),
    })
}

fn read_vdf(path: &Path) -> Option<Vdf> {
    vdf::parse(&fs::read_to_string(path).ok()?)
}

fn canon(p: &Path) -> PathBuf {
    fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
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
            format!(r#""libraryfolders" {{ "0" {{ "path" "{extra_escaped}" }} }}"#),
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

        let mut games: Vec<_> = library_paths(&[root]).iter().flat_map(|l| scan_library(l)).collect();
        games.sort_by(|a, b| a.app_id.cmp(&b.app_id));
        fs::remove_dir_all(&tmp).ok();

        assert_eq!(games.len(), 2, "malformed manifest is skipped");
        assert_eq!(games[0].name, "Game A");
        assert_eq!(games[0].build_id.as_deref(), Some("1234"));
        assert!(games[0].update_required);
        assert_eq!(games[0].size_bytes, Some(500));
        assert!(games[0].path.ends_with("steamapps/common/GameA"));
        assert_eq!(games[1].build_id, None, "buildid 0 means unknown, not version 0");
        assert!(!games[1].update_required);
    }
}
