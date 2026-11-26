//! The source adapter interface (M5). Every way RepakHub finds games is a `Source`: it declares what
//! it reads and which hosts it may contact, and the user can switch it off. Built-in adapters wrap
//! the modules in `steam.rs`, `gog.rs`, `epic.rs` and `managed.rs`. Third-party adapters are not
//! supported yet; how they will run is decided in docs/adr/0001-third-party-adapters.md.

use crate::discovery::{Reconcile, ScanResult};
use crate::{ScanLock, epic, gog, managed, manifest, steam};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::path::PathBuf;
use tauri::State;

/// What an adapter may touch, shown to the user before they rely on it.
pub struct Access {
    /// Files, folders and registry keys it reads.
    pub reads: Vec<String>,
    /// Hosts it may contact. Empty for the launcher adapters: they never use the network.
    pub network: Vec<String>,
}

pub trait Source: Send + Sync {
    /// Stable id, also the `source` of the games it finds (except `repakhub`, see `managed.rs`).
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn access(&self) -> Access;
    /// Find installed games. One adapter may report several library sources (RepakHub installs are
    /// grouped by the authorised source they came from).
    fn scan(&self, locations: &[PathBuf]) -> Vec<(String, ScanResult)>;
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

struct Steam;
impl Source for Steam {
    fn id(&self) -> &'static str {
        "steam"
    }
    fn name(&self) -> &'static str {
        "Steam"
    }
    fn description(&self) -> &'static str {
        "Games installed by Steam, in every Steam library folder, with build ids, pending updates and DLC."
    }
    fn access(&self) -> Access {
        Access {
            reads: strings(&[
                "Steam's install folder and library folders (steamapps/libraryfolders.vdf, appmanifest_*.acf)",
                "Windows registry: HKCU\\Software\\Valve\\Steam (SteamPath), HKLM\\SOFTWARE\\WOW6432Node\\Valve\\Steam",
                "Your scan locations, two folders deep, looking for steamapps",
            ]),
            network: vec![],
        }
    }
    fn scan(&self, locations: &[PathBuf]) -> Vec<(String, ScanResult)> {
        vec![(self.id().into(), steam::scan(locations))]
    }
}

struct Gog;
impl Source for Gog {
    fn id(&self) -> &'static str {
        "gog"
    }
    fn name(&self) -> &'static str {
        "GOG"
    }
    fn description(&self) -> &'static str {
        "GOG games installed by GOG Galaxy, Heroic or offline installers, with versions and DLC."
    }
    fn access(&self) -> Access {
        Access {
            reads: strings(&[
                "goggame-*.info files in GOG's default folders and your scan locations",
                "Version information inside each game's main executable",
                "Windows registry: HKLM\\SOFTWARE\\WOW6432Node\\GOG.com\\Games",
                "Heroic's gog_store/installed.json",
            ]),
            network: vec![],
        }
    }
    fn scan(&self, locations: &[PathBuf]) -> Vec<(String, ScanResult)> {
        vec![(self.id().into(), gog::scan(locations))]
    }
}

struct Epic;
impl Source for Epic {
    fn id(&self) -> &'static str {
        "epic"
    }
    fn name(&self) -> &'static str {
        "Epic Games"
    }
    fn description(&self) -> &'static str {
        "Games installed by the Epic Games Launcher, Heroic or Legendary, with versions and DLC."
    }
    fn access(&self) -> Access {
        Access {
            reads: strings(&[
                "Epic Games Launcher manifests (ProgramData\\Epic\\EpicGamesLauncher\\Data\\Manifests)",
                "Legendary's installed.json (Heroic and the Legendary CLI)",
                "Version information inside each game's main executable",
            ]),
            network: vec![],
        }
    }
    fn scan(&self, _locations: &[PathBuf]) -> Vec<(String, ScanResult)> {
        vec![(self.id().into(), epic::scan())]
    }
}

struct RepakHubInstalls;
impl Source for RepakHubInstalls {
    fn id(&self) -> &'static str {
        "repakhub"
    }
    fn name(&self) -> &'static str {
        "RepakHub installs"
    }
    fn description(&self) -> &'static str {
        "Games RepakHub installed from authorised sources, kept up to date with signed manifests."
    }
    fn access(&self) -> Access {
        let mut hosts: Vec<String> = manifest::trusted_keys()
            .iter()
            .filter_map(|k| url::Url::parse(k.catalog.as_deref()?).ok()?.host_str().map(String::from))
            .collect();
        hosts.sort();
        hosts.dedup();
        Access {
            reads: strings(&[
                ".repakhub/install.json in RepakHub's library folder and your scan locations",
                "Files of the games it installed, to verify and update them",
            ]),
            // Only when you install or check for updates, never during a scan.
            network: hosts.into_iter().map(|h| format!("{h} (only when installing or checking for updates)")).collect(),
        }
    }
    fn scan(&self, locations: &[PathBuf]) -> Vec<(String, ScanResult)> {
        managed::scan(locations)
    }
}

pub fn all() -> Vec<Box<dyn Source>> {
    vec![Box::new(Steam), Box::new(Gog), Box::new(Epic), Box::new(RepakHubInstalls)]
}

/// The ids of games' `source` values that belong to a launcher adapter (one-to-one with its id).
pub const LAUNCHER_IDS: &[&str] = &["steam", "gog", "epic"];

/// Scans that empty the library of switched-off adapters: a launcher's own source, or for RepakHub
/// installs every authorised source seen before (`known_managed`).
pub fn removals(disabled: &[String], known_managed: &[String]) -> Vec<(String, ScanResult)> {
    let empty = || ScanResult { games: vec![], libraries: vec![], reconcile: Reconcile::Complete };
    let mut out: Vec<(String, ScanResult)> =
        LAUNCHER_IDS.iter().filter(|id| disabled.iter().any(|d| d == *id)).map(|id| (id.to_string(), empty())).collect();
    if disabled.iter().any(|d| d == "repakhub") {
        out.extend(known_managed.iter().map(|s| (s.clone(), empty())));
    }
    out
}

#[derive(Serialize, Deserialize, Default)]
struct SourcesSetting {
    #[serde(default)]
    disabled: Vec<String>,
}

/// Switched-off adapter ids. Kept in their own settings row (`sources`).
pub async fn disabled(db: &SqlitePool) -> Result<Vec<String>, String> {
    let row: Option<(String,)> = sqlx::query_as("SELECT value FROM settings WHERE key = 'sources'")
        .fetch_optional(db)
        .await
        .map_err(|e| e.to_string())?;
    Ok(row.and_then(|(v,)| serde_json::from_str::<SourcesSetting>(&v).ok()).unwrap_or_default().disabled)
}

async fn save_disabled(db: &SqlitePool, disabled: Vec<String>) -> Result<(), String> {
    let json = serde_json::to_string(&SourcesSetting { disabled }).map_err(|e| e.to_string())?;
    sqlx::query("INSERT INTO settings (key, value) VALUES ('sources', ?) ON CONFLICT (key) DO UPDATE SET value = excluded.value")
        .bind(json)
        .execute(db)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterInfo {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    reads: Vec<String>,
    network: Vec<String>,
    enabled: bool,
}

/// Features outside the adapters that contact the network, each only when the user uses it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OnlineFeature {
    name: &'static str,
    hosts: &'static [&'static str],
    when: &'static str,
}

const ONLINE_FEATURES: &[OnlineFeature] = &[
    OnlineFeature {
        name: "Cover art",
        hosts: &["cdn.cloudflare.steamstatic.com", "api.gog.com", "images.gog-statics.com"],
        when: "While \"Show cover art\" is on in Settings",
    },
    OnlineFeature {
        name: "Owned Steam games",
        hosts: &["api.steampowered.com"],
        when: "Only after you add your own Steam Web API key in Settings",
    },
    OnlineFeature {
        name: "GOG store search",
        hosts: &["catalog.gog.com", "images.gog-statics.com"],
        when: "When you search on the Store page",
    },
    OnlineFeature { name: "Store links", hosts: &["www.gog.com"], when: "Opened in your browser when you choose a store link" },
    OnlineFeature {
        name: "RepakHub updates",
        hosts: &["github.com", "objects.githubusercontent.com", "release-assets.githubusercontent.com"],
        when: "On startup, unless switched off in Settings; installed only when you choose",
    },
];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Integrations {
    adapters: Vec<AdapterInfo>,
    online: &'static [OnlineFeature],
}

async fn integrations(db: &SqlitePool) -> Result<Integrations, String> {
    Ok(Integrations { adapters: infos(db).await?, online: ONLINE_FEATURES })
}

async fn infos(db: &SqlitePool) -> Result<Vec<AdapterInfo>, String> {
    let disabled = disabled(db).await?;
    Ok(all()
        .iter()
        .map(|s| {
            let access = s.access();
            AdapterInfo {
                id: s.id(),
                name: s.name(),
                description: s.description(),
                reads: access.reads,
                network: access.network,
                enabled: !disabled.iter().any(|d| d == s.id()),
            }
        })
        .collect())
}

#[tauri::command]
pub async fn list_integrations(db: State<'_, SqlitePool>) -> Result<Integrations, String> {
    integrations(&db).await
}

/// Switch an adapter on or off. Takes effect on the next scan (the UI runs one).
#[tauri::command]
pub async fn set_adapter_enabled(
    db: State<'_, SqlitePool>,
    lock: State<'_, ScanLock>,
    id: String,
    enabled: bool,
) -> Result<Integrations, String> {
    if !all().iter().any(|s| s.id() == id) {
        return Err(format!("unknown adapter {id:?}"));
    }
    let _guard = lock.0.lock().await; // never change the set mid-scan
    let mut off = disabled(&db).await?;
    off.retain(|d| *d != id);
    if !enabled {
        off.push(id);
    }
    save_disabled(&db, off).await?;
    drop(_guard);
    integrations(&db).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapters_are_unique_and_declare_their_access() {
        let ids: Vec<_> = all().iter().map(|s| s.id()).collect();
        assert_eq!(ids, vec!["steam", "gog", "epic", "repakhub"]);
        for s in all() {
            assert!(!s.access().reads.is_empty(), "{} declares what it reads", s.id());
        }
        for id in LAUNCHER_IDS {
            let s = all().into_iter().find(|s| s.id() == *id).unwrap();
            assert!(s.access().network.is_empty(), "{id} never uses the network");
        }
        let managed = all().into_iter().find(|s| s.id() == "repakhub").unwrap();
        assert!(managed.access().network.iter().any(|h| h.starts_with("raw.githubusercontent.com")), "demo source host declared");
    }

    #[test]
    fn switched_off_adapters_empty_their_sources() {
        let r = removals(&["gog".into(), "repakhub".into()], &["repakhub-demo".into()]);
        let ids: Vec<_> = r.iter().map(|(s, _)| s.as_str()).collect();
        assert_eq!(ids, vec!["gog", "repakhub-demo"]);
        assert!(r.iter().all(|(_, s)| s.games.is_empty() && s.reconcile == Reconcile::Complete));
        assert!(removals(&[], &["repakhub-demo".into()]).is_empty());
    }
}
