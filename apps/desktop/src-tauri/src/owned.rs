//! Games the user owns, installed or not. Read on demand and never stored: the platforms are the
//! authority on ownership, so RepakHub only shows what they report.
//!
//! - Steam: the documented Web API (`IPlayerService/GetOwnedGames`) with the user's own API key.
//! - GOG: GOG Galaxy's local database. Read-only; no GOG login.
//! - GOG and Epic via Heroic: Heroic's local library caches.

use crate::downloads::{self, DownloadError, Transport};
use crate::{discovery, settings, steam};
use serde::{Deserialize, Serialize};
use sqlx::sqlite::{SqliteConnectOptions, SqliteConnection};
use sqlx::{Connection, SqlitePool};
use std::path::{Path, PathBuf};
use tauri::State;

/// Large Steam libraries are a few hundred KB.
const MAX_STEAM_BYTES: u64 = 8 * 1024 * 1024;

/// Shaped like an `Installation`'s identity, so the UI can match it against installed games.
#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OwnedGame {
    source: &'static str,
    source_game_id: String,
    name: String,
    /// App that can install it: `steam`, `galaxy` or `heroic`.
    launcher: &'static str,
}

#[derive(Serialize, Default)]
pub struct Owned {
    games: Vec<OwnedGame>,
    /// Sources that failed, as readable messages. Never includes the Steam API key.
    errors: Vec<String>,
}

#[tauri::command]
pub async fn list_owned(db: State<'_, SqlitePool>) -> Result<Owned, String> {
    let s = settings::load(&db).await?;
    let mut out = Owned::default();
    if let Some(path) = galaxy_db().filter(|p| p.is_file()) {
        match galaxy_owned(&path).await {
            Ok(games) => out.games.extend(games),
            Err(e) => out.errors.push(format!("GOG Galaxy: {e}")),
        }
    }
    let (games, errors) = tauri::async_runtime::spawn_blocking(move || {
        let mut games: Vec<OwnedGame> = discovery::heroic_config_dirs().iter().flat_map(|d| heroic_owned(d)).collect();
        let mut errors = vec![];
        if !s.steam_api_key.is_empty() {
            let id = Some(s.steam_id).filter(|i| !i.is_empty()).or_else(steam::signed_in_steam_id);
            match id {
                Some(id) => match steam_owned(&Transport::new(), &s.steam_api_key, &id) {
                    Ok(g) => games.extend(g),
                    Err(e) => errors.push(format!("Steam: {e}")),
                },
                None => errors.push("Steam: enter your SteamID64 in Settings (no signed-in Steam account found)".into()),
            }
        }
        (games, errors)
    })
    .await
    .map_err(|e| e.to_string())?;
    out.games.extend(games);
    out.errors.extend(errors);
    out.games.sort_by(|a, b| (a.source, &a.source_game_id).cmp(&(b.source, &b.source_game_id)));
    out.games.dedup_by(|a, b| a.source == b.source && a.source_game_id == b.source_game_id);
    out.games.sort_by_key(|g| g.name.to_lowercase());
    Ok(out)
}

#[derive(Deserialize)]
struct SteamResponse {
    response: SteamOwned,
}

/// `games` is absent when the profile's game details are private.
#[derive(Deserialize)]
struct SteamOwned {
    games: Option<Vec<SteamGame>>,
}

#[derive(Deserialize)]
struct SteamGame {
    appid: u64,
    name: Option<String>,
}

fn steam_owned(t: &Transport, key: &str, steam_id: &str) -> Result<Vec<OwnedGame>, String> {
    let url = url::Url::parse_with_params(
        "https://api.steampowered.com/IPlayerService/GetOwnedGames/v1/",
        [("key", key), ("steamid", steam_id), ("include_appinfo", "1"), ("format", "json")],
    )
    .map_err(|e| e.to_string())?;
    // Errors carry the URL, which holds the key, so only the status reaches the UI.
    let text = downloads::fetch_text(t, url.as_str(), MAX_STEAM_BYTES).map_err(|e| match e {
        DownloadError::Status { status: 401 | 403, .. } => "Steam rejected the API key".to_string(),
        DownloadError::Status { status, .. } => format!("Steam returned HTTP {status}"),
        _ => "Steam could not be reached".to_string(),
    })?;
    parse_steam(&text)
}

fn parse_steam(text: &str) -> Result<Vec<OwnedGame>, String> {
    let r: SteamResponse = serde_json::from_str(text).map_err(|_| "unexpected response from Steam".to_string())?;
    let games = r.response.games.ok_or("your profile's game details are private, so Steam lists no games")?;
    Ok(games
        .into_iter()
        .map(|g| OwnedGame {
            source: "steam",
            name: g.name.unwrap_or_else(|| format!("App {}", g.appid)),
            source_game_id: g.appid.to_string(),
            launcher: "steam",
        })
        .collect())
}

fn galaxy_db() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("PROGRAMDATA").map(|p| PathBuf::from(p).join(r"GOG.com\Galaxy\storage\galaxy-2.0.db"))
    } else if cfg!(target_os = "macos") {
        Some("/Users/Shared/GOG.com/Galaxy/Storage/galaxy-2.0.db".into())
    } else {
        None
    }
}

/// GOG titles in Galaxy's library. Galaxy also lists games from other platforms it is linked to
/// (`steam_`, `epic_` release keys); only `gog_` ones are taken, the rest come from their own source.
// ponytail: includes owned DLC; filter on the 'meta' game piece if that clutters the list.
async fn galaxy_owned(path: &Path) -> Result<Vec<OwnedGame>, sqlx::Error> {
    let opts = SqliteConnectOptions::new().filename(path).read_only(true);
    let mut conn = SqliteConnection::connect_with(&opts).await?;
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT DISTINCT p.releaseKey, p.value FROM GamePieces p
         JOIN GamePieceTypes t ON t.id = p.gamePieceTypeId
         WHERE t.type = 'title' AND p.releaseKey IN (SELECT releaseKey FROM LibraryReleases)",
    )
    .fetch_all(&mut conn)
    .await?;
    Ok(rows.into_iter().filter_map(|(key, value)| galaxy_game(&key, &value)).collect())
}

/// A Galaxy release key (`gog_1207658924`) and its title piece (`{"title": "..."}`).
fn galaxy_game(release_key: &str, value: &str) -> Option<OwnedGame> {
    #[derive(Deserialize)]
    struct Title {
        title: String,
    }
    let id = release_key.strip_prefix("gog_").filter(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))?;
    let name = serde_json::from_str::<Title>(value).ok()?.title;
    Some(OwnedGame { source: "gog", source_game_id: id.into(), name, launcher: "galaxy" })
}

/// Heroic's library caches: GOG (`store_cache/gog_library.json`, older `gog_store/library.json`) and
/// Epic via Legendary (`store_cache/legendary_library.json`). Missing or unreadable files are skipped.
fn heroic_owned(config_dir: &Path) -> Vec<OwnedGame> {
    let files = [
        ("store_cache/gog_library.json", "gog"),
        ("gog_store/library.json", "gog"),
        ("store_cache/legendary_library.json", "epic"),
    ];
    files
        .into_iter()
        .filter_map(|(file, source)| Some((discovery::read_capped(&config_dir.join(file))?, source)))
        .flat_map(|(json, source)| parse_heroic(&json, source))
        .collect()
}

/// Heroic has stored its list under `games` and under `library` across versions; both are accepted.
fn parse_heroic(json: &str, source: &'static str) -> Vec<OwnedGame> {
    #[derive(Deserialize)]
    struct Entry {
        app_name: String,
        title: String,
        #[serde(default)]
        is_dlc: bool,
    }
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else { return vec![] };
    let Some(list) = v.get("games").or_else(|| v.get("library")).and_then(|l| l.as_array()) else { return vec![] };
    list.iter()
        .filter_map(|e| serde_json::from_value::<Entry>(e.clone()).ok())
        .filter(|e| !e.is_dlc)
        .map(|e| OwnedGame { source, source_game_id: e.app_name, name: e.title, launcher: "heroic" })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steam_owned_games_and_private_profiles() {
        let games = parse_steam(r#"{"response":{"game_count":2,"games":[{"appid":10,"name":"Counter-Strike"},{"appid":20}]}}"#).unwrap();
        assert_eq!(games.iter().map(|g| (g.source_game_id.as_str(), g.name.as_str())).collect::<Vec<_>>(), [("10", "Counter-Strike"), ("20", "App 20")]);
        assert!(parse_steam(r#"{"response":{}}"#).unwrap_err().contains("private"));
        assert!(parse_steam("<html>").is_err());
    }

    #[test]
    fn galaxy_release_keys() {
        let g = galaxy_game("gog_1207658924", r#"{"title":"The Witcher"}"#).unwrap();
        assert_eq!((g.source, g.source_game_id.as_str(), g.name.as_str()), ("gog", "1207658924", "The Witcher"));
        assert_eq!(galaxy_game("steam_70", r#"{"title":"Half-Life"}"#), None, "other platforms come from their own source");
        assert_eq!(galaxy_game("gog_../x", r#"{"title":"x"}"#), None);
        assert_eq!(galaxy_game("gog_1", "not json"), None);
    }

    #[test]
    fn reads_a_galaxy_database() {
        tauri::async_runtime::block_on(async {
            let path = std::env::temp_dir().join(format!("repakhub-galaxy-test-{}.db", std::process::id()));
            let opts = SqliteConnectOptions::new().filename(&path).create_if_missing(true);
            let mut conn = SqliteConnection::connect_with(&opts).await.unwrap();
            for sql in [
                "CREATE TABLE GamePieceTypes (id INTEGER PRIMARY KEY, type TEXT)",
                "CREATE TABLE GamePieces (releaseKey TEXT, gamePieceTypeId INTEGER, userId INTEGER, value TEXT)",
                "CREATE TABLE LibraryReleases (userId INTEGER, releaseKey TEXT)",
                "INSERT INTO GamePieceTypes VALUES (1, 'title'), (2, 'meta')",
                r#"INSERT INTO GamePieces VALUES ('gog_1', 1, 1, '{"title":"Owned"}'), ('gog_1', 2, 1, '{}'),
                   ('gog_2', 1, 1, '{"title":"Not owned"}'), ('steam_3', 1, 1, '{"title":"Steam"}')"#,
                "INSERT INTO LibraryReleases VALUES (1, 'gog_1'), (1, 'steam_3')",
            ] {
                sqlx::query(sql).execute(&mut conn).await.unwrap();
            }
            conn.close().await.unwrap();
            let games = galaxy_owned(&path).await;
            std::fs::remove_file(&path).ok();
            assert_eq!(games.unwrap().iter().map(|g| g.name.as_str()).collect::<Vec<_>>(), ["Owned"]);
        });
    }

    #[test]
    fn heroic_caches_in_both_shapes() {
        let gog = parse_heroic(r#"{"games":[{"app_name":"1207658924","title":"The Witcher"},{"app_name":"2","title":"DLC","is_dlc":true}]}"#, "gog");
        assert_eq!(gog.iter().map(|g| g.name.as_str()).collect::<Vec<_>>(), ["The Witcher"]);
        let epic = parse_heroic(r#"{"library":[{"app_name":"Fortnite","title":"Fortnite","extra":1},{"title":"no id"}]}"#, "epic");
        assert_eq!(epic.iter().map(|g| (g.source, g.source_game_id.as_str())).collect::<Vec<_>>(), [("epic", "Fortnite")]);
        assert!(parse_heroic("[]", "gog").is_empty());
    }
}
