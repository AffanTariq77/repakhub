# Contributing

## Branches

- `dev`: integration branch. All feature work branches from and merges back into `dev`.
- `main`: stable releases only. Merged from `dev` when a release is tested and approved, then tagged `vX.Y.Z`.
- Feature branches: `feat/<short-name>`, `fix/<short-name>`.

A change is ready to merge into `dev` when `pnpm check` passes locally and CI is green.

## Releases

1. Open a pull request from `dev` into `main` titled `Release vX.Y.Z`, after bumping the version in `apps/desktop/package.json`, `apps/desktop/src-tauri/Cargo.toml` and `tauri.conf.json`.
2. Merge it once CI is green.
3. Tag the merge commit on `main` and push the tag: `git tag vX.Y.Z && git push origin vX.Y.Z`.
4. The Release workflow builds the Windows and Linux installers and publishes them as a GitHub release.

## Commits

Short imperative subject (`Add GOG adapter`), body explaining why when it is not obvious.

## Code

- Rust owns filesystem, database and network work. The UI calls it through Tauri commands in `src-tauri/src/lib.rs`.
- Types shared with the UI live in `apps/desktop/src/api.ts` and must match the Rust structs (`serde(rename_all = "camelCase")`).
- Schema changes go in a new numbered file under `src-tauri/migrations`. Never edit a migration that has shipped.
- Unknown is a valid answer. Never display a guessed version or update state.
- Adapters, downloads, manifests and signing keys follow [docs/SOURCE_POLICY.md](docs/SOURCE_POLICY.md). These paths need owner review (`.github/CODEOWNERS`).
- No secrets in the repo. Signing keys and tokens go in CI secrets.
