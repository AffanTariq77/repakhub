# RepakHub demo source

A registered source (see the register in [docs/SOURCE_POLICY.md](../docs/SOURCE_POLICY.md)) that exists to demonstrate signed installs and updates. It distributes a tiny MIT-licensed content pack written for this purpose. It is not a game.

The app reads it from `main` over HTTPS: `https://raw.githubusercontent.com/AffanTariq77/repakhub/main/demo-source/`.

- `index.json`: game ids in the catalog.
- `repakhub-demo.json`: the signed manifest envelope for the current version.
- `files/<version>/`: the content of each version.

## Publishing a version

The signing key is not in the repository. It lives at `~/.repakhub-keys/repakhub-demo-2026.pem` on the maintainer's machine.

```sh
node scripts/demo-source.mjs sign 1.1.0   # rewrites index.json and repakhub-demo.json
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml committed_demo_catalog   # offline check
```

Commit, merge to `main`, then confirm with `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml -- --ignored live_demo_install`.

1.0.0 is published first so an install can then be updated to 1.1.0, which changes a file, adds one and removes one.
