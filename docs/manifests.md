# Update manifests

How an authorised source tells RepakHub that a new version of a game exists. Only sources in the register in [SOURCE_POLICY.md](SOURCE_POLICY.md) can be trusted, and only with a key compiled into the app (`apps/desktop/src-tauri/keys/trusted.json`).

## Envelope

What the source serves over HTTPS:

```json
{
  "keyId": "example-2026",
  "payload": "<base64 of the manifest JSON below>",
  "signature": "<base64 Ed25519 signature over the decoded payload bytes>"
}
```

The signature covers the exact payload bytes, so whitespace and key order in the manifest do not matter.

## Manifest (schema 1)

```json
{
  "schema": 1,
  "source": "example-source",
  "gameId": "example-game",
  "name": "Example Game",
  "version": "1.1.0",
  "changelog": "Fixes a crash on startup.",
  "hosts": ["downloads.example.org"],
  "files": [
    {
      "path": "data/levels.pak",
      "size": 104857600,
      "sha256": "<64 lowercase hex characters>",
      "url": "https://downloads.example.org/example-game/1.1.0/data/levels.pak"
    }
  ]
}
```

`files` is the complete file set of that version. RepakHub downloads only the files whose hash differs from what is installed.

## Rules RepakHub enforces

A manifest is rejected, and nothing is downloaded, if any of these fail:

- The key id is not compiled into the app, or the signature does not verify.
- `schema` is not `1`, `source` differs from the key's source, or there are unknown fields.
- A `path` is not a plain relative `/`-separated path (no root, drive, `..`, `.` or backslash), is not portable (Windows-invalid characters such as `:`, trailing dots or spaces, reserved names like `CON` or `NUL`), or two paths collide ignoring case.
- A `sha256` is not 64 lowercase hex characters.
- A `url` is not `https`, its host is not in `hosts`, or it has a port or credentials.
- A file is an executable (by extension, or by content once downloaded: `MZ`, ELF, Mach-O, `#!`) and the key is not marked `rightsHolder`.

There are no scripts, hooks or post-install steps, by design.
