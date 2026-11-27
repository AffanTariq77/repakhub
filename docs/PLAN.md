# RepakHub delivery plan

Source: the RepakHub Product & Development Blueprint. This file is the working plan: what we build, in what order, and what "done" means. Update the status column as milestones land.

**Goal of v1.0:** RepakHub discovers installed games, keeps a reliable local library, identifies versions with visible confidence, and reports available updates. It is useful on its own, with no account and no server.

## Locked decisions

| Area | Decision |
| --- | --- |
| Desktop | Tauri 2, Rust core, React 19 + TypeScript, Tailwind CSS v4 |
| Storage | SQLite via SQLx, migrations embedded in the binary (`src-tauri/migrations`) |
| Repo | pnpm workspace monorepo. `apps/desktop` now, `apps/api` when the optional Services phase starts |
| Platforms | Windows and Linux are release targets and CI targets. macOS works for development |
| Branching | `dev` is the integration branch. `main` only receives tested releases. See `CONTRIBUTING.md` |
| Network | Core features work offline. Network use: optional cover art (Steam, GOG), authorised download sources, the Store page (GOG's public catalogue) and owned Steam games (Steam Web API, only with the user's own key) |

## Deliberate deviations from the blueprint

Each is deferred, not dropped. The trigger says when to add it.

| Deferred | Why | Add when |
| --- | --- | --- |
| shadcn/ui | Nothing yet needs a dialog, menu or popover | First complex component (Game Details, Settings) |
| `packages/shared-types`, `packages/ui` | One consumer. Types live in `apps/desktop/src/api.ts` and mirror the Rust structs | A second consumer exists (API or plugin SDK). Consider `ts-rs` generation then |
| Separate `games` table | Cross-source matching is by name and computed in the UI, so nothing needs storing | Users need to link or unlink copies by hand, or match on store ids |
| Router library | Desktop app, no deep links. Page is component state | Deep links or back/forward history are needed |
| NestJS API, PostgreSQL, Redis | Services are optional by principle. Nothing in the MVP needs them | Signed manifests need hosting beyond static files, or partner onboarding starts |
| Out-of-process plugin runtime | Two or three first-party adapters do not justify a sandboxed runtime yet | Week 8, or the first third-party adapter |

## Milestones

| # | Weeks | Milestone | Status |
| --- | --- | --- | --- |
| M0 | 1 | Foundation | Done |
| M1 | 2-3 | Game discovery and library | In progress: Steam, GOG, Epic and onboarding done |
| M2 | 4 | Version engine | Done |
| M3 | 5-6 | Updates and downloads | Done (demo live once on `main`) |
| M4 | 7 | DLC and backups | Done |
| M5 | 8 | Source adapter SDK and permissions | Done (third-party runtime designed, built when needed) |
| M6 | 9 | Hardening and test pass | Security review done; real-device testing needs you |
| M7 | 10 | Release v1.0 and pitch pack | Not started |

### M0 Foundation

- [x] Repository, pnpm workspace, `dev`/`main` branches
- [x] Tauri 2 + React + TypeScript app at `apps/desktop`
- [x] Rust backend with SQLite and embedded migrations
- [x] Tailwind dark theme (charcoal base, violet/blue accents)
- [x] Navigation shell: Dashboard, My Library, Downloads, Updates, DLC & Add-ons, Integrations, Settings
- [x] GitHub Actions: frontend build, Rust tests and Clippy on Ubuntu and Windows
- [x] Architecture and contribution docs
- [x] First green CI run

### M1 Game discovery and library

- [x] KeyValues (VDF/ACF) parser
- [x] Steam adapter: library folders (old and new formats), app manifests, build ID, size, "update pending" flag
- [x] Library persists across restarts, upserts by `(source, source_game_id)` so rescans never duplicate
- [x] Library UI: grid and list, search, "updates pending" filter. Dashboard counts
- [x] Onboarding: pick drives and folders to scan, stored in a `settings` table. Settings page edits the same list
- [x] Steam libraries inside chosen locations found up to two folders deep (`D:\SteamLibrary`, `D:\Games\Steam`)
- [x] Windows registry (`HKCU\Software\Valve\Steam\SteamPath`, machine-wide `InstallPath`) for Steam installed outside the default folders
- [x] Hide Steam runtimes and redistributables (Proton, Steam Linux Runtime, Steamworks Common Redistributables)
- [x] GOG adapter: `goggame-*.info` files in install folders (cross-platform), plus GOG Galaxy registry on Windows. DLC skipped via `rootGameId`. Version from registry `ver`, else build id, else unknown. Update state always unknown
- [x] GOG via Heroic's `gog_store/installed.json` (Linux native, Flatpak, Windows `%APPDATA%`), registry wins on Windows
- [x] GOG cover art: looked up in batches from GOG's public products API (`store::gog_covers`), same "Show cover art" consent as Steam
- [x] Owned games not installed, shown in My Library with a hand-off to the platform's app: Steam Web API (user's own key, SteamID from `loginusers.vdf`), GOG Galaxy's `galaxy-2.0.db`, Heroic's GOG and Epic library caches (`owned.rs`). Read on demand, never stored
- [x] Store page: search GOG's public catalogue, open results on gog.com (`store.rs`)
- [ ] Verify Galaxy and Heroic library formats against real installs (written from their known shapes, untested on a live machine)
- [ ] Epic owned games without Heroic (the Epic launcher keeps no readable owned list)
- [x] Epic adapter: launcher manifests (`*.item`) on Windows, Heroic/Legendary `installed.json`. Authoritative list, so uninstalls are removed even when the folder is gone
- [x] Game folders for games no launcher manages (`folders.rs`): folders chosen in Settings, each subfolder one game named after it, version from the largest executable that is not an installer or helper. Game folders are also searched by the launcher adapters, and a folder they recognise stays theirs. Not incremental: a scan reads only headers and resources of one executable per game, so rescans stay cheap; add an mtime cache if large collections are slow
- [x] Same game across sources: one library card, sources listed on it, "Also installed" in Game Details, counted once on the Dashboard; owned-but-not-installed copies share a card with a launcher button each. Matched by name without case, accents, trademark signs and punctuation (`gameKey`), exact matches only. Computed in the UI, so a wrong match never touches stored data; the list view and Updates stay per copy
- [ ] The same Steam app in two Steam installs (e.g. native + Flatpak) is stored once, last scan wins: needs installs keyed by path as well as id
- [x] Game Details screen: path, source, version with its source and confidence, update state explained, "Show folder", "Open in Steam / GOG Galaxy / Epic"
- [x] Updates screen: pending updates with launcher hand-off, and an honest count of games whose launcher does not report updates

Done when: a fresh install on Windows and Linux finds every Steam, GOG and Epic game on the test machines, with no duplicates after repeated scans.

### M2 Version engine

Detection strategies, highest confidence first:

1. Launcher metadata (Steam build ID, GOG build, Epic `AppVersionString`)
2. Authorised source manifests
3. Executable version resource (Windows PE `VS_VERSION_INFO`)
4. User confirmation

- [x] Confidence (`launcher`, `user`, `executable`, `unknown`) derived from the strategy that supplied the shown version, rather than stored, so it cannot go stale. `manifest` arrives with M3
- [x] Version comparison: numeric dotted versions compared part by part, anything else equality only; build ids never compared with version numbers (`version.rs`)
- [x] Conflicts (two comparable strategies disagree) flagged in lists and shown with every raw value in Game Details
- [x] Executable strategy: product version from the main executable's `VS_FIXEDFILEINFO` (GOG `playTasks`, Epic `LaunchExecutable`, Legendary `executable`), read cross-platform by a bounds-checked parser (`pe.rs`)
- [x] User confirmation: enter or clear a version in Game Details, stored apart from scans (`version_overrides`)
- [x] UI shows source and confidence everywhere a version appears (Game Details text, tooltip in lists)

Done when: every supported test build is identified correctly and anything else shows "Unknown".

### M3 Updates and downloads

Launcher-managed games (Steam, Epic) are updated by their launcher. RepakHub reports the pending update and hands off with "Open in launcher". RepakHub only downloads from sources in the register in [SOURCE_POLICY.md](SOURCE_POLICY.md), and its technical enforcement rules are requirements for this milestone:

- [x] Update manifest schema (JSON), signed with Ed25519 (`manifest.rs`, [manifests.md](manifests.md))
- [x] Signing keys compiled into the binary (`src-tauri/keys/trusted.json`, empty until a source is registered). No runtime key or source registration
- [x] HTTPS only, from hosts listed in the signed manifest, checked on every redirect hop. No BitTorrent, magnet or peer-to-peer transport
- [x] Reject manifests that add or replace executables unless signed by the rights holder's key: by name in the manifest, by content (MZ, ELF, Mach-O, `#!`) after download
- [x] Reference source for demos: `demo-source/` (MIT pack written for the purpose), served from `main` via raw.githubusercontent.com, signed with a key kept outside the repo (`scripts/demo-source.mjs`), registered in the source policy
- [x] Update check: fetch manifest, verify signature, compare versions, show changelog and size (Updates page, `updates.rs`)
- [x] Download engine: parallel workers, pause (cancel) and resume via HTTP range from `.part` files, retry with backoff, disk-space check (`downloads.rs`). UI and commands with the apply step
- [x] SHA-256 verification per file before anything leaves the staging folder; size enforced while downloading. Staging cleanup after apply comes with the apply step
- [x] Apply: back up replaced and removed files, move staged files in, verify, roll back everything on failure, keep the previous version's backup (`apply.rs`)
- [x] Install from a source's catalog, RepakHub-managed installs discovered by their `.repakhub/install.json` record (`managed.rs`), Downloads page with progress, pause and resume
- [x] In-app notification: pending-update badge on the Updates nav item
- [x] Operating-system notifications: one per newly pending update, remembered across restarts so each is announced once (`tauri-plugin-notification`, only notify and permission calls allowed)

Done when: the "Invalid manifest", "Corrupted download" and "Interrupted download" acceptance tests pass.

### M4 DLC and backups

- [x] DLC metadata per source: Steam `InstalledDepots` `dlcappid` (ids only; Steam's text files hold no names), GOG DLC info files (`rootGameId`), Epic manifests (`MainGameAppName`) and Legendary `is_dlc` entries
- [x] DLC installation state, as each launcher records it, in Game Details and the DLC & Add-ons page. Ownership is never inferred: the UI says it is the platform's to check
- [x] Save and config backup: user-chosen files and folders per game, snapshots as plain folders in the app data folder (not zip: no new dependency, openable in a file manager), restore with an automatic backup of the current state first (`backups.rs`)

### M5 Source adapter SDK

- [x] Rust `Source` trait (`sources.rs`) with Steam, GOG, Epic and RepakHub installs behind it. One `scan` capability: adapters already return versions, DLC and update state together, and installing updates belongs to the signed-source engine
- [x] Declared access per adapter: what it reads and which hosts it may contact (launcher adapters: none). Online features listed with their hosts and when they are used
- [x] Integrations screen: switch adapters on or off (their games leave the library), view declared access
- [x] Third-party adapter runtime decided: WebAssembly components on Wasmtime, preopened read-only folders, host-checked HTTPS fetch, signed `.rhpkg` packages ([ADR 0001](adr/0001-third-party-adapters.md)). Build when the first partner adapter exists

### M6 Hardening

- [ ] Full acceptance test pass (below) on Windows 11 and Ubuntu LTS, following [real-device-testing.md](real-device-testing.md)
- [x] Large and interrupted download tests (`downloads.rs`): a connection dropped mid-file resumes from where it stopped, pause keeps the partial file and resumes from it, a corrupted partial file fails verification and starts over, a 32 MB file streams to disk with steady progress, slow transfers continue and only stalls time out. Found and fixed: the HTTP client limited the whole body to 60 seconds, so any file taking longer to download always failed; it now times out only after 60 seconds with no data
- [x] Migration test: a database stopped at every point in the migration history, filled as that version would, upgrades with its data and rescans without duplicates; shipped migrations pinned by hash so an edit fails CI, and Windows CRLF checkouts pinned (`lib.rs` tests)
- [x] Security review: adapter permissions, manifest verification, path traversal on apply, webview, local data, dependencies, CI ([report](security/review-2026-11.md)). Fixed: Content Security Policy, owner-only app data, read-only pinned CI
- [x] Steam API key in the OS keychain (`secrets.rs`); dependency audits (`cargo audit`, `pnpm audit`) in CI on every push and weekly, required to merge

### M7 Release

- [x] Release workflow: `v*` tag builds Windows (MSI, NSIS), Linux (deb, rpm, AppImage) and macOS (dmg, Apple silicon) installers and publishes a GitHub pre-release. App icon
- [x] RepakHub updates itself: Tauri updater with its own signing key (kept off the repo; used only by the release `publish` job), `latest.json` on the newest GitHub release, banner with Install and restart, startup check switchable in Settings. macOS (app archive), Windows (NSIS) and Linux (AppImage); deb/rpm update through the package
- [x] macOS signing and notarisation in the release workflow, on once the Apple secrets are set ([code-signing.md](code-signing.md))
- [ ] Apple Developer ID certificate and secrets (owner)
- [ ] Windows code signing: choose a certificate provider, then wire `signCommand` ([code-signing.md](code-signing.md))
- [ ] User docs, demo recording, landing page, partner proposal

## Acceptance criteria for v1.0

| Test | Expected |
| --- | --- |
| Scan selected drives | Supported installed games detected |
| Rescan | No duplicate entries (covered by `lib.rs` test) |
| Restart | Library and settings preserved |
| Version detection | Supported builds identified correctly |
| Unknown version | Shows "Unknown", never a guess |
| Update available | Correct update information shown |
| Interrupted download | Resumes safely where the source supports ranges |
| Invalid manifest | Update rejected |
| Corrupted download | Fails verification, nothing applied |
| Restore backup | Selected data recovered |
| Windows and Linux | Core features work on both |

## Risks

| Risk | Mitigation |
| --- | --- |
| Partner choice undermines "authorised sources only" | See below |
| Version formats differ per game | Multiple strategies, visible confidence, "Unknown" is a valid answer |
| Launcher formats change | Parsers tolerate unknown keys, malformed files are skipped not fatal, tests use real-shaped fixtures |
| Malicious adapters | No third-party code until M5 sandboxing and signing exist |
| Linux variety (native, Flatpak, Proton) | All three Steam roots probed, CI on Ubuntu, manual test on a Flatpak install |
| Partner rejection | The product stands alone on discovery and version tracking |

### Partnership targets

The blueprint names FitGirl and DODI as possible partners. Both publish repacks of commercial games without publisher licences, so they cannot authorise distribution of that content. Integrating them would contradict product principle 4 (authorised sources only), and would expose RepakHub to DMCA takedowns of the repository and release hosting, and to removal from distribution channels. It would also make legitimate partners (GOG, publishers) unlikely to engage.

Recommended outreach list instead: GOG, itch.io, Humble, DRM-free and indie publishers, open-source game projects.

Safeguards, so this cannot drift back in:

- [x] [SOURCE_POLICY.md](SOURCE_POLICY.md): what counts as authorised, what never does, source register
- [x] `.github/CODEOWNERS` and pull request checklist on adapters, downloads, manifests and keys
- [x] Technical enforcement written into M3 (pinned keys, HTTPS only, no torrents, no executable patching)
- [x] Branch protection on `dev` and `main`: pull request, code-owner review, both CI jobs green, no force-push. Admin bypass on while solo
- [ ] Remove FitGirl and DODI from the blueprint before it is shared with investors or partners
- [ ] Name and trademark check before public launch: "repack" is piracy-scene terminology
- [ ] Terms of use banning unauthorised sources, and a published takedown contact, before v1.0
- [ ] Legal review before v1.0, focused on the plugin marketplace
