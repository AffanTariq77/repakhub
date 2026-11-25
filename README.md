# RepakHub

One library. Every game. Updates without the hassle.

RepakHub is a local-first desktop app that finds the games installed across your launchers and drives, shows which version you have, and tells you when a legitimate update exists. It works without an account or a server.

Status: early development. Discovers Steam, GOG and Epic games (including Heroic and Legendary installs), shows versions and pending updates, and hands off to each launcher. See [docs/PLAN.md](docs/PLAN.md) for the roadmap.

## Run it

Prerequisites: Node 22+, pnpm 10, Rust stable, and the [Tauri system dependencies](https://v2.tauri.app/start/prerequisites/) for your OS.

```sh
pnpm install
pnpm dev        # launches the desktop app with hot reload
pnpm check      # frontend typecheck/build + frontend tests + Rust tests
pnpm build      # release installer for the current OS
```

Installed builds update themselves from GitHub releases (signed; see docs/architecture.md). Releases: see CONTRIBUTING.md.

## Layout

```
apps/desktop/            Tauri app
  src/                   React UI (App shell, Dashboard, Library, api.ts types)
  src-tauri/src/         Rust core: lib.rs (commands, DB), steam.rs, vdf.rs
  src-tauri/migrations/  SQLite schema, embedded at compile time
docs/                    Plan and architecture notes
```

## Principles

Local-first, source-independent, authorised sources only, transparent about what changes, Windows and Linux from day one. RepakHub does not download, distribute or modify game files from unauthorised sources, and does not circumvent DRM. What counts as an authorised source is defined in [docs/SOURCE_POLICY.md](docs/SOURCE_POLICY.md).
