# Source policy

This policy decides which sources RepakHub may integrate with, and what RepakHub may download. It applies to first-party code, adapters, plugins, partners and marketing. Changes to this file need owner approval (see `.github/CODEOWNERS`).

## Two kinds of integration

| Kind | What it does | Bar to clear |
| --- | --- | --- |
| **Read-only discovery** | Reads launcher metadata already on the user's disk to list games, versions and pending updates | The source's files are read, never modified. No network calls to the source beyond its public, documented endpoints |
| **Download source** | Supplies files that RepakHub downloads and installs | Must be an authorised source, below, with a signing key pinned in the RepakHub binary |

Discovering what is already on a user's own machine is always allowed. Fetching content is where the rules below apply.

## Authorised sources

A download source is authorised only if at least one of these is true, and the evidence is recorded in the source register below:

1. **Rights holder.** The publisher or developer of the content.
2. **Licensed distributor.** A store or platform with distribution rights from the rights holder, plus a written agreement with RepakHub covering data access, distribution and branding.
3. **Redistributable licence.** The content's licence explicitly permits redistribution (for example GPL, MIT, CC BY), and RepakHub distributes it under those terms.

## Never authorised

Regardless of what any party claims:

- Repackers and repack groups, including FitGirl and DODI
- Warez sites, scene releases, cracked or DRM-stripped builds
- Torrent indexes, magnet links, and peer-to-peer distribution of commercial content
- Anything that modifies, patches or replaces game executables without the rights holder's signature
- Anything that circumvents DRM, licence checks or entitlement checks

## Technical enforcement

These rules are part of the update and download engine (PLAN.md, M3), not optional guidance:

1. **Pinned keys.** RepakHub only acts on update manifests signed with an Ed25519 key compiled into the binary (`apps/desktop/src-tauri/keys/trusted.json`; format in [manifests.md](manifests.md)). Keys are added only for sources in the register. Adapters and plugins cannot add keys or download sources at runtime.
2. **HTTPS only.** Files are fetched over HTTPS from hosts listed in the signed manifest. No BitTorrent, magnet, or peer-to-peer transport will be implemented.
3. **No executable patching.** A manifest that adds or replaces executable files (`.exe`, `.dll`, `.so`, ELF binaries, scripts) is rejected unless the manifest is signed by the rights holder's own key.
4. **Verify before apply.** Every file is SHA-256 verified against the signed manifest before anything on disk changes.
5. **Ownership from the platform.** DLC and game ownership comes from the platform's entitlement data, never from the presence of files on disk.

## Branding and claims

- No partner names or logos appear anywhere (app, site, docs, demos, pitch decks) without written permission.
- Never state or imply that any party endorses RepakHub until they have agreed in writing.
- Marketing, issues and community channels must not reference unauthorised sources. Such posts are removed.

## Adding a download source

1. Open an issue with the source, the authorisation basis (1, 2 or 3 above) and the evidence.
2. The owner verifies the evidence and, for basis 2, the signed agreement.
3. Add a row to the register below in the same pull request that pins the key.
4. The pull request needs owner approval (CODEOWNERS).

## Takedown and abuse

Report infringement or a policy breach to the project owner via a GitHub issue marked `policy`, or the contact address on the website once published. Reports are acknowledged within 5 working days. Offending plugins, adapters or content are disabled or removed.

## Source register

| Source | Basis | Evidence | Key fingerprint | Added |
| --- | --- | --- | --- | --- |
| RepakHub demo pack (`repakhub-demo`) | 3. Redistributable licence: content written by RepakHub contributors, MIT licensed (`demo-source/LICENSE`) | Files and licence in `demo-source/` of this repository | `repakhub-demo-2026`, SHA-256 of public key `8914d310a4aafa5c…` | 2026-11-25 |
