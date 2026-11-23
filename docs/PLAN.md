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
| Network | Core features work offline. The only network use today is optional Steam cover art |

## Deliberate deviations from the blueprint

Each is deferred, not dropped. The trigger says when to add it.

| Deferred | Why | Add when |
| --- | --- | --- |
| shadcn/ui | Nothing yet needs a dialog, menu or popover | First complex component (Game Details, Settings) |
| `packages/shared-types`, `packages/ui` | One consumer. Types live in `apps/desktop/src/api.ts` and mirror the Rust structs | A second consumer exists (API or plugin SDK). Consider `ts-rs` generation then |
| Separate `games` table | Only one source, so no cross-source matching yet | GOG or Epic adapter lands and the same title can appear twice |
| Router library | Desktop app, no deep links. Page is component state | Deep links or back/forward history are needed |
| NestJS API, PostgreSQL, Redis | Services are optional by principle. Nothing in the MVP needs them | Signed manifests need hosting beyond static files, or partner onboarding starts |
| Out-of-process plugin runtime | Two or three first-party adapters do not justify a sandboxed runtime yet | Week 8, or the first third-party adapter |

## Milestones

| # | Weeks | Milestone | Status |
| --- | --- | --- | --- |
| M0 | 1 | Foundation | Done |
| M1 | 2-3 | Game discovery and library | In progress: Steam and onboarding done |
| M2 | 4 | Version engine | Not started |
| M3 | 5-6 | Updates and downloads | Not started |
| M4 | 7 | DLC and backups | Not started |
| M5 | 8 | Source adapter SDK and permissions | Not started |
| M6 | 9 | Hardening and test pass | Not started |
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
- [ ] Windows registry `SteamPath` for Steam installed outside the defaults and outside chosen locations
- [ ] Hide Steam runtimes and redistributables (Proton, Steam Linux Runtime, Steamworks Common Redistributables)
- [ ] GOG adapter: `goggame-*.info` files in install folders (cross-platform), plus GOG Galaxy registry on Windows
- [ ] Epic adapter: launcher manifests (`*.item`) on Windows, Heroic/Legendary `installed.json` on Linux
- [ ] Folder scan for unknown games: user-selected folders, executable fingerprinting, incremental
- [ ] Introduce `games` table and cross-source matching
- [ ] Game Details screen: path, source, version, version confidence, "Open in Steam/GOG/Epic"

Done when: a fresh install on Windows and Linux finds every Steam, GOG and Epic game on the test machines, with no duplicates after repeated scans.

### M2 Version engine

Detection strategies, highest confidence first:

1. Launcher metadata (Steam build ID, GOG build, Epic `AppVersionString`)
2. Authorised source manifests
3. Executable version resource (Windows PE `VS_VERSION_INFO`)
4. User confirmation

- [ ] Store `version_confidence` (`launcher`, `manifest`, `executable`, `user`, `unknown`) alongside `version_source`
- [ ] Version comparison: semver where it parses, build ID equality otherwise, never guess ordering between incomparable schemes
- [ ] Conflicts (two strategies disagree) shown to the user, not silently resolved
- [ ] UI shows confidence everywhere a version appears

Done when: every supported test build is identified correctly and anything else shows "Unknown".

### M3 Updates and downloads

Launcher-managed games (Steam, Epic) are updated by their launcher. RepakHub reports the pending update and hands off with "Open in launcher". RepakHub only downloads from sources in the register in [SOURCE_POLICY.md](SOURCE_POLICY.md), and its technical enforcement rules are requirements for this milestone:

- [ ] Update manifest schema (JSON), signed with Ed25519
- [ ] Signing keys compiled into the binary (`src-tauri/keys/`). No runtime key or source registration by adapters or plugins
- [ ] HTTPS only, from hosts listed in the signed manifest. No BitTorrent, magnet or peer-to-peer transport
- [ ] Reject manifests that add or replace executables unless signed by the rights holder's key
- [ ] Reference source for demos: redistributable open-source games whose licences permit it, manifests on static hosting
- [ ] Update check: fetch manifest, verify signature, compare versions, show changelog and size
- [ ] Download manager: queue, concurrent downloads, pause/resume via HTTP range, retry with backoff, disk-space check
- [ ] SHA-256 verification per file or chunk, staging directory, cleanup of partial files
- [ ] Apply: back up replaced files, apply, verify, roll back on failure
- [ ] Notifications for available updates

Done when: the "Invalid manifest", "Corrupted download" and "Interrupted download" acceptance tests pass.

### M4 DLC and backups

- [ ] DLC metadata per source (Steam manifest `InstalledDepots`/DLC info, GOG, Epic)
- [ ] DLC installation state. Ownership only from the platform, never inferred from files on disk
- [ ] Save and config backup: user-chosen paths per game, zip archives, restore

### M5 Source adapter SDK

- [ ] Rust `Source` trait with capability methods (discover, version, updates, DLC), refactor Steam/GOG/Epic onto it
- [ ] Adapter manifest: id, version, declared permissions (filesystem paths, network hosts)
- [ ] Integrations screen: enable, disable, view permissions
- [ ] Decide third-party adapter runtime (separate process over stdio, or WASM). Signed packages required

### M6 Hardening

- [ ] Full acceptance test pass (below) on Windows 11 and Ubuntu LTS
- [ ] Large and interrupted download tests
- [ ] Migration test: upgrade a v0 database to current
- [ ] Security review: adapter permissions, manifest verification, path traversal on apply

### M7 Release

- [ ] Signed installers (MSI/NSIS, AppImage/deb), Tauri updater for RepakHub itself
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
