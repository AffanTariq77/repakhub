//! Games no launcher manages (DRM-free downloads, itch.io, portable games), in folders the user marks as
//! holding one game per subfolder. The subfolder's name is the game's name, because executables' own
//! product names are often an engine's or a project's. The version comes from the main executable.

use crate::discovery::{self, DiscoveredGame, Reconcile, ScanResult};
use std::fs;
use std::path::{Path, PathBuf};

/// How far below a game's folder to look for its executable: Unreal games keep it in `Binaries/Win64`.
const EXE_SEARCH_DEPTH: u32 = 3;
/// Entries looked at per game, so a huge folder (a mod pack, a data dump) cannot stall the scan.
const MAX_ENTRIES: usize = 5000;
/// Executables that come with games but are not the game.
const NOT_THE_GAME: &[&str] = &["unins", "setup", "install", "redist", "vc_", "prereq", "dxweb", "dotnet", "crash"];

/// Every subfolder of every game folder is one game. Folders that are offline are left out of
/// `libraries`, so their games are kept rather than removed.
pub fn scan(folders: &[PathBuf]) -> ScanResult {
    let mut games = vec![];
    let mut libraries = vec![];
    for folder in folders {
        if discovery::is_network_path(&folder.to_string_lossy()) || !discovery::is_real_dir(folder) {
            continue;
        }
        let Ok(entries) = fs::read_dir(folder) else { continue };
        libraries.push(folder.clone());
        for e in entries.flatten() {
            // file_type() does not follow links, so a link to a share or elsewhere is never entered.
            if !discovery::is_hidden(&e) && e.file_type().is_ok_and(|t| t.is_dir()) {
                games.push(game(&e.path()));
            }
        }
    }
    ScanResult { games, libraries, reconcile: Reconcile::ByLocation }
}

fn game(dir: &Path) -> DiscoveredGame {
    DiscoveredGame {
        source_game_id: dir.to_string_lossy().into_owned(),
        name: dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        path: dir.to_path_buf(),
        version: None,
        version_source: None,
        update_available: None,
        size_bytes: None,
        last_updated: None,
        launcher: None,
        exe_version: main_exe(dir).and_then(|e| crate::pe::product_version(&e)),
        dlc: vec![],
    }
}

/// The largest executable that is not an installer, uninstaller or helper.
// ponytail: largest-.exe guess; let the user choose the executable in Game Details if it picks wrong.
fn main_exe(dir: &Path) -> Option<PathBuf> {
    let mut best: Option<(u64, PathBuf)> = None;
    let mut budget = MAX_ENTRIES;
    walk(dir, EXE_SEARCH_DEPTH, &mut budget, &mut |path, size| {
        if best.as_ref().is_none_or(|(s, _)| size > *s) {
            best = Some((size, path));
        }
    });
    best.map(|(_, p)| p)
}

fn walk(dir: &Path, depth: u32, budget: &mut usize, found: &mut impl FnMut(PathBuf, u64)) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        if *budget == 0 {
            return;
        }
        *budget -= 1;
        let Ok(t) = e.file_type() else { continue };
        if t.is_dir() && depth > 0 && !discovery::is_hidden(&e) {
            walk(&e.path(), depth - 1, budget, found);
        } else if t.is_file()
            && is_game_exe(&e.file_name().to_string_lossy())
            && let Ok(m) = e.metadata()
        {
            found(e.path(), m.len());
        }
    }
}

fn is_game_exe(name: &str) -> bool {
    let name = name.to_lowercase();
    name.ends_with(".exe") && !NOT_THE_GAME.iter().any(|n| name.contains(n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_subfolder_is_a_game_with_its_main_executable() {
        let root = std::env::temp_dir().join(format!("repakhub-folders-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let write = |p: &str, bytes: usize| {
            let p = root.join(p);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, vec![0u8; bytes]).unwrap();
        };
        write("Games/Celeste/Celeste.exe", 30);
        write("Games/Celeste/unins000.exe", 90);
        write("Games/Celeste/data/big.pak", 500);
        write("Games/Shooter/Binaries/Win64/Shooter-Win64-Shipping.exe", 80);
        write("Games/Shooter/Shooter.exe", 10);
        write("Games/Shooter/Engine/Extras/Redist/UEPrereqSetup_x64.exe", 900);
        write("Games/.hidden/x.exe", 1);
        write("Games/readme.txt", 1);
        fs::create_dir_all(root.join("Games/Empty")).unwrap();

        let exe = |g: &str| main_exe(&root.join("Games").join(g)).map(|p| p.file_name().unwrap().to_string_lossy().into_owned());
        assert_eq!(exe("Celeste").as_deref(), Some("Celeste.exe"), "uninstaller skipped");
        assert_eq!(exe("Shooter").as_deref(), Some("Shooter-Win64-Shipping.exe"), "largest, prerequisites skipped");
        assert_eq!(exe("Empty"), None);

        let offline = root.join("Unplugged");
        let scan = scan(&[root.join("Games"), offline]);
        let mut names: Vec<_> = scan.games.iter().map(|g| g.name.as_str()).collect();
        names.sort();
        assert_eq!(names, ["Celeste", "Empty", "Shooter"]);
        assert_eq!(scan.libraries, [root.join("Games")], "an offline folder is not reconciled");
        assert!(scan.games.iter().all(|g| g.launcher.is_none() && g.version.is_none() && g.source_game_id == g.path.to_string_lossy()));
        fs::remove_dir_all(&root).ok();
    }
}
