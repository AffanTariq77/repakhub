# Architecture

```
React UI  ──invoke──▶  Tauri commands (lib.rs)  ──▶  SQLite (app data dir)
                              │
                              └──▶  Source adapters (steam.rs, ...)  ──▶  launcher files on disk
```

## Data flow: scan

1. UI calls `scan_library`.
2. Rust runs each adapter (Steam, GOG) on a blocking thread. The Steam adapter finds Steam roots, reads `libraryfolders.vdf` for every library, then each `appmanifest_*.acf`.
3. Results are upserted into `installations` keyed by `(source, source_game_id)`. Rows the scan did not see are removed if they sit in a library that was read (uninstalled) or their folder still exists outside any scanned library (location deselected). Rows whose folder is missing outside scanned libraries are kept, so an unplugged drive does not wipe its games. All in one transaction, and scans are serialised.

Paths read from launcher files are never network (UNC) paths, launcher files over 4 MB or nested deeper than 32 levels are rejected, and hidden or system folders are not searched.
4. The full list is returned to the UI.

`list_games` reads the stored library without scanning, so startup is instant and works offline.

## Storage

SQLite at the OS app data directory (`co.repakhub.desktop/repakhub.db`). Migrations in `src-tauri/migrations` are compiled into the binary and run on startup.

`installed_version` and `update_available` are nullable on purpose: `NULL` means unknown. `version_source` records how a version was determined (e.g. `steam_buildid`).

## Adapters

Adapters only read launcher metadata. They never write to launcher directories. Steam updates are applied by Steam; RepakHub reports them. Each adapter is a plain module whose `scan(locations)` returns a `discovery::ScanResult` (games found, plus the folders actually read). `discovery.rs` holds the shared folder search and safe-read helpers. A `Source` trait arrives with the plugin SDK (PLAN.md, M5), not before.
