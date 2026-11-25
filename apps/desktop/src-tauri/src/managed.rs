//! Games RepakHub installed itself from a signed manifest, found by their `.repakhub/install.json`
//! record in RepakHub's library folder and the user's scan locations. Grouped by source, since each
//! authorised source is its own `source` in the library.

use crate::apply;
use crate::discovery::{self, DiscoveredGame, Reconcile, ScanResult, canon};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Where RepakHub installs games unless the user picks another folder.
pub fn library_dir() -> PathBuf {
    std::env::home_dir().unwrap_or_default().join("Games").join("RepakHub")
}

pub fn scan(locations: &[PathBuf]) -> Vec<(String, ScanResult)> {
    scan_roots(&std::iter::once(library_dir()).chain(locations.iter().cloned()).collect::<Vec<_>>())
}

fn scan_roots(roots: &[PathBuf]) -> Vec<(String, ScanResult)> {
    let mut dirs = vec![];
    let mut libraries = vec![];
    for root in roots {
        // Same rule as GOG: an empty (e.g. unmounted) folder does not count as scanned.
        if fs::read_dir(root).is_ok_and(|mut d| d.next().is_some()) {
            libraries.push(canon(root));
            let has_record = |d: &Path| apply::record_path(d).is_file();
            discovery::find_dirs(root, discovery::LOCATION_SEARCH_DEPTH, &has_record, &mut dirs);
        }
    }
    let mut by_source: BTreeMap<String, Vec<DiscoveredGame>> = BTreeMap::new();
    let mut complete = true;
    for dir in dirs.iter().map(|d| canon(d)).collect::<std::collections::BTreeSet<_>>() {
        let Some(r) = apply::read_record(&dir) else {
            complete = false; // mid-write or damaged: keep what is stored
            continue;
        };
        by_source.entry(r.source).or_default().push(DiscoveredGame {
            source_game_id: r.game_id,
            name: r.name,
            path: dir,
            version: Some(r.version),
            version_source: Some("manifest"),
            update_available: None, // set by the update check, kept across scans
            size_bytes: None,
            last_updated: None,
            launcher: Some("repakhub"),
            exe_version: None,
            dlc: vec![],
        });
    }
    let reconcile = if complete { Reconcile::ByLocation } else { Reconcile::AddOnly };
    by_source.into_iter().map(|(source, games)| (source, ScanResult { games, libraries: libraries.clone(), reconcile })).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_install_records_grouped_by_source() {
        let root = std::env::temp_dir().join(format!("repakhub-managed-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let write = |dir: &str, source: &str, id: &str| {
            let d = root.join(dir).join(".repakhub");
            fs::create_dir_all(&d).unwrap();
            let r = apply::InstallRecord { source: source.into(), game_id: id.into(), name: id.into(), version: "1.0".into(), files: vec![] };
            fs::write(d.join("install.json"), serde_json::to_vec(&r).unwrap()).unwrap();
        };
        write("demo", "src-a", "demo");
        write("nested/other", "src-b", "other");
        let result = scan_roots(std::slice::from_ref(&root));
        fs::remove_dir_all(&root).ok();

        let summary: Vec<_> = result.iter().map(|(s, r)| (s.as_str(), r.games.len(), r.games[0].version_source)).collect();
        assert_eq!(summary, vec![("src-a", 1, Some("manifest")), ("src-b", 1, Some("manifest"))]);
        assert!(result.iter().all(|(_, r)| r.games[0].launcher == Some("repakhub")));
    }
}
