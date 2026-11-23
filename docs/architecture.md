# Architecture

```
React UI  ──invoke──▶  Tauri commands (lib.rs)  ──▶  SQLite (app data dir)
                              │
                              └──▶  Source adapters (steam.rs, ...)  ──▶  launcher files on disk
```

## Data flow: scan

1. UI calls `scan_library`.
2. Rust runs each adapter on a blocking thread. The Steam adapter finds Steam roots, reads `libraryfolders.vdf` for every library, then each `appmanifest_*.acf`.
3. Results are upserted into `installations` keyed by `(source, source_game_id)`. Rows from that source not seen in this scan are removed. All in one transaction.
4. The full list is returned to the UI.

`list_games` reads the stored library without scanning, so startup is instant and works offline.

## Storage

SQLite at the OS app data directory (`co.repakhub.desktop/repakhub.db`). Migrations in `src-tauri/migrations` are compiled into the binary and run on startup.

`installed_version` and `update_available` are nullable on purpose: `NULL` means unknown. `version_source` records how a version was determined (e.g. `steam_buildid`).

## Adapters

Adapters only read launcher metadata. They never write to launcher directories. Steam updates are applied by Steam; RepakHub reports them. A shared `Source` trait arrives with the second adapter (see PLAN.md, M5).
