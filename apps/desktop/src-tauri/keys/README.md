# Trusted manifest keys

`trusted.json` lists the Ed25519 public keys RepakHub accepts update manifests from. It is compiled into the app; nothing can add a key at runtime.

```json
[
  {
    "id": "example-2026",
    "source": "example-source",
    "publicKey": "<base64 of the 32-byte Ed25519 public key>",
    "rightsHolder": false
  }
]
```

- `source` must match the `source` field of every manifest signed with this key.
- `rightsHolder: true` only for the publisher's own key. Only those manifests may add or replace executables.

Adding a key requires a row in the source register in `docs/SOURCE_POLICY.md` with the authorisation evidence, in the same pull request, approved by the code owner. Private keys never enter this repository.
