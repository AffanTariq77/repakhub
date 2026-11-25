# Architecture

```
React UI  ──invoke──▶  Tauri commands (lib.rs)  ──▶  SQLite (app data dir)
                              │
                              └──▶  Source adapters (steam.rs, ...)  ──▶  launcher files on disk
```

## Data flow: scan

1. UI calls `scan_library`.
2. Rust runs each adapter (Steam, GOG, Epic) on a blocking thread. The Steam adapter finds Steam roots, reads `libraryfolders.vdf` for every library, then each `appmanifest_*.acf`.
3. Results are upserted into `installations` keyed by `(source, source_game_id)`, then rows of that source the scan did not see are reconciled according to the scan's `Reconcile` mode:
   - `Complete` (Epic, every install list read): unseen rows are uninstalled and removed.
   - `ByLocation` (Steam, GOG): removed if inside a library that was read (uninstalled) or if the folder still exists outside them (location deselected). Missing folders elsewhere are kept, so an unplugged drive does not wipe its games.
   - `AddOnly` (any launcher file unreadable or half-written): nothing is removed.

   Any launcher file that fails to parse (e.g. mid-update) makes that source's scan `AddOnly`. All sources are saved in one transaction with a timestamp forced past the newest stored one; scans and settings saves are serialised, and only one app instance runs at a time.

   Paths read from launcher files are never network (UNC) paths in any separator spelling, Steam `installdir` must be a plain relative folder, Windows symlinks and junctions to launcher files are not followed, launcher files over 4 MB or nested deeper than 32 levels are rejected, hidden or system folders are not searched, and symlinks are not followed.
4. The full list is returned to the UI.

`list_games` reads the stored library without scanning, so startup is instant and works offline.

## Storage

SQLite at the OS app data directory (`co.repakhub.desktop/repakhub.db`). Migrations in `src-tauri/migrations` are compiled into the binary and run on startup.

`installed_version` and `update_available` are nullable on purpose: `NULL` means unknown. `version_source` records how a version was determined (e.g. `steam_buildid`). `launcher` records the app that manages the install (`steam`, `galaxy`, `epic`, `heroic`, `legendary`); the UI's hand-off button is chosen from it, never guessed from the store.

Settings live in one JSON row (`settings` table): onboarding state, scan locations, and `coverArt`, the user's consent to load Steam cover art from Steam's image servers. The Steam Web API key is the exception: it is kept in the OS keychain (`secrets.rs`) and only falls back to the settings row, in the owner-only app data folder, when no keychain is available.

## Versions

Three strategies, in precedence order: the launcher's metadata (`installed_version`), a version the user entered (`version_overrides`), and the product version in the game's main executable (`exe_version`, read by `pe.rs`). `version::resolve` picks the shown value when listing, labels its confidence by strategy, and flags a conflict when two comparable values disagree. `version::compare` orders numeric dotted versions and otherwise only tests equality; build ids are never compared with version numbers.

## Installs and updates from authorised sources

A source is a trusted key (`keys/trusted.json`) with a `catalog`: an HTTPS folder holding `index.json` (game ids) and one signed `<gameId>.json` envelope per game ([manifests.md](manifests.md)).

1. `updates.rs` fetches the envelope, `manifest.rs` verifies signature and rules.
2. `downloads.rs` plans the files whose hash differs, downloads them into `<game>/.repakhub/staging` (HTTPS to listed hosts only, resume, retry, size limit, SHA-256, executable check).
3. `apply.rs` moves replaced and removed files to `<game>/.repakhub/backup`, moves staged files in, re-verifies, and rolls everything back on any failure. It then writes `<game>/.repakhub/install.json`.
4. `managed.rs` discovers those records on the next scan, so the game appears with `manifest` confidence and `launcher = repakhub`.

Jobs run on background threads and report progress to the UI as `repakhub://job` events. Pausing keeps `.part` files; starting again resumes.

## Owned games and the Store page

`owned.rs` lists games the user owns, installed or not, each time My Library opens: Steam's Web API with the user's own key (errors never echo the URL, which holds the key), GOG Galaxy's local database opened read-only, and Heroic's library caches. Nothing is stored; the UI hides owned games that are already installed. `store.rs` searches GOG's public catalogue for the Store page and passes on only `www.gog.com` links and `images.gog-statics.com` art. See "Store listing" in [SOURCE_POLICY.md](SOURCE_POLICY.md).

## Save and config backups

Per game, the user chooses files and folders (`backup_paths` table, keyed by source and game id so rescans keep them). A backup is a folder under `<app data>/backups/<source>-<gameId>/<timestamp>/` holding `backup.json` and a copy of each path; it is written as `<timestamp>.tmp` and renamed when complete. Symlinks are skipped, never followed. Restoring takes an automatic backup of the current state first, then copies each path next to the original and swaps it in, putting the original back if the swap fails.

## Adapters

Every way RepakHub finds games implements `Source` (`sources.rs`): an id, a name, the access it declares (files and registry keys it reads, hosts it may contact) and `scan(locations)`, returning `discovery::ScanResult`s. The built-in adapters wrap `steam.rs`, `gog.rs`, `epic.rs` and `managed.rs`. They only read launcher metadata and never write to launcher folders; the launcher adapters never use the network.

Users can switch adapters off on the Integrations page (`settings` row `sources`). A switched-off adapter is skipped, and its games are removed on the next scan. Third-party adapters will run as sandboxed WebAssembly components ([ADR 0001](adr/0001-third-party-adapters.md)).
