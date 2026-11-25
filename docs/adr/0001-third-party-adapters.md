# ADR 0001: Third-party source adapters

Status: accepted (design). Not built yet: no third-party adapter exists to justify the runtime. Build it when the first partner adapter is ready.

## Context

Built-in adapters (Steam, GOG, Epic, RepakHub installs) implement the `Source` interface in `apps/desktop/src-tauri/src/sources.rs` and are reviewed RepakHub code. Partners and the community will want adapters for other launchers and stores. Those adapters must not get unrestricted access to the user's machine (blueprint, "Plugin security"; [SOURCE_POLICY.md](../SOURCE_POLICY.md)).

Requirements:

1. An adapter can only read what its manifest declares, and only contact hosts it declares.
2. It cannot run arbitrary native code in RepakHub's process.
3. Packages are signed; RepakHub refuses unsigned or tampered ones.
4. The same package works on Windows, Linux and macOS.
5. The user sees the declared access before enabling it, and can switch it off.

## Options

| Option | Isolation | Cross-platform | Verdict |
| --- | --- | --- | --- |
| Native libraries (dll/so/dylib) loaded in-process | None: full process access | Per-platform builds | Rejected (fails 1, 2) |
| Separate native process over stdio | Only with OS sandboxes: AppContainer, App Sandbox, seccomp/landlock, each different and partial | Per-platform builds and sandboxes | Rejected: three sandboxes to get right, easy to get wrong |
| Embedded script engine (Lua, JS) | Good if the host exposes no I/O | One package | Viable, but untyped and slower for parsing launcher files |
| **WebAssembly components (Wasmtime, WASI)** | Strong by default: no file system or network unless the host grants it | One `.wasm` for every OS | **Chosen** |

## Decision

Third-party adapters are **WebAssembly components** run by Wasmtime, implementing a RepakHub WIT interface that mirrors `Source`:

```wit
package repakhub:source@1.0.0;

interface adapter {
  record dlc { id: string, name: option<string> }
  record game {
    source-game-id: string, name: string, path: string,
    version: option<string>, version-source: option<string>,
    update-available: option<bool>, dlc: list<dlc>,
  }
  info: func() -> tuple<string, string>;            // name, description
  scan: func(locations: list<string>) -> list<game>;
}
```

- **File access:** the host preopens only the folders the manifest declares (resolved per OS, read-only) plus the user's scan locations if declared. Nothing else is visible to the component.
- **Network:** no WASI sockets. A host function `fetch(url) -> bytes` checks the URL against the manifest's `hosts`, requires HTTPS, caps the size, and follows redirects only to listed hosts (the rules `downloads.rs` already enforces).
- **Output is validated by the host** like built-in results: paths must stay inside declared folders, ids and versions are length-limited, nothing is trusted because an adapter said it.
- **Packages:** `.rhpkg` = zip of `adapter.json` (id, name, version, declared `reads` and `hosts`), `adapter.wasm`, and `signature` (Ed25519 over the SHA-256 of both files) by a key in a compiled-in adapter key list, managed like `keys/trusted.json`.
- **Resource limits:** fuel or epoch interruption caps CPU per scan; memory capped; a failing adapter is disabled for the session, not fatal.
- **User control:** shown in Integrations with its declared access, off until the user switches it on.

## Consequences

- Adds Wasmtime (a large dependency) when built. Accepted for the isolation it gives.
- Adapter authors write Rust (or any language targeting WASI components) against the WIT file, which becomes `packages/plugin-sdk` at that point.
- Built-in adapters stay native Rust behind the same `Source` interface; they are reviewed code with the same declared-access display.
