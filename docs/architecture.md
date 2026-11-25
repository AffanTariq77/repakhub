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

Settings live in one JSON row (`settings` table): onboarding state, scan locations, and `coverArt`, the user's consent to load Steam cover art from Steam's image servers.

## Versions

Three strategies, in precedence order: the launcher's metadata (`installed_version`), a version the user entered (`version_overrides`), and the product version in the game's main executable (`exe_version`, read by `pe.rs`). `version::resolve` picks the shown value when listing, labels its confidence by strategy, and flags a conflict when two comparable values disagree. `version::compare` orders numeric dotted versions and otherwise only tests equality; build ids are never compared with version numbers.

## Installs and updates from authorised sources

A source is a trusted key (`keys/trusted.json`) with a `catalog`: an HTTPS folder holding `index.json` (game ids) and one signed `<gameId>.json` envelope per game ([manifests.md](manifests.md)).

1. `updates.rs` fetches the envelope, `manifest.rs` verifies signature and rules.
2. `downloads.rs` plans the files whose hash differs, downloads them into `<game>/.repakhub/staging` (HTTPS to listed hosts only, resume, retry, size limit, SHA-256, executable check).
3. `apply.rs` moves replaced and removed files to `<game>/.repakhub/backup`, moves staged files in, re-verifies, and rolls everything back on any failure. It then writes `<game>/.repakhub/install.json`.
4. `managed.rs` discovers those records on the next scan, so the game appears with `manifest` confidence and `launcher = repakhub`.

Jobs run on background threads and report progress to the UI as `repakhub://job` events. Pausing keeps `.part` files; starting again resumes.

## Adapters

Adapters only read launcher metadata. They never write to launcher directories. Steam updates are applied by Steam; RepakHub reports them. Each adapter is a plain module whose `scan(locations)` returns a `discovery::ScanResult` (games found, plus the folders actually read). `discovery.rs` holds the shared folder search and safe-read helpers. A `Source` trait arrives with the plugin SDK (PLAN.md, M5), not before.
