#!/usr/bin/env node
// Maintains the RepakHub demo source (demo-source/). Node built-ins only.
//
//   node scripts/demo-source.mjs keygen          create the signing key (once), print the public key
//   node scripts/demo-source.mjs sign <version>  sign demo-source/files/<version> as the live catalog
//
// The private key lives outside the repository, in ~/.repakhub-keys/, readable only by you.
// Back it up: without it the demo source can never publish another version.

import { createHash, createPrivateKey, createPublicKey, generateKeyPairSync, sign } from "node:crypto";
import { chmodSync, existsSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join, relative, sep } from "node:path";

const KEY_ID = "repakhub-demo-2026";
const SOURCE = "repakhub-demo";
const GAME_ID = "repakhub-demo";
const NAME = "RepakHub Demo Pack";
const HOST = "raw.githubusercontent.com";
const BASE = `https://${HOST}/AffanTariq77/repakhub/main/demo-source/`;
const CHANGELOG = {
  "1.0.0": "First release of the demo pack.",
  "1.1.0": "Adds a third level and a music cue list; retires the palette file.",
};

const root = new URL("../demo-source/", import.meta.url).pathname;
const keyDir = join(homedir(), ".repakhub-keys");
const keyFile = join(keyDir, `${KEY_ID}.pem`);

function publicKeyBase64(privatePem) {
  // An Ed25519 SPKI DER is a fixed 12-byte header followed by the 32-byte raw key.
  const der = createPublicKey(createPrivateKey(privatePem)).export({ type: "spki", format: "der" });
  return der.subarray(der.length - 32).toString("base64");
}

function keygen() {
  if (existsSync(keyFile)) {
    console.log(`Key already exists: ${keyFile}`);
  } else {
    mkdirSync(keyDir, { recursive: true, mode: 0o700 });
    const { privateKey } = generateKeyPairSync("ed25519");
    writeFileSync(keyFile, privateKey.export({ type: "pkcs8", format: "pem" }), { mode: 0o600 });
    chmodSync(keyFile, 0o600);
    console.log(`Created ${keyFile} (mode 600). Back it up somewhere safe.`);
  }
  const pub = publicKeyBase64(readFileSync(keyFile, "utf8"));
  const fingerprint = createHash("sha256").update(Buffer.from(pub, "base64")).digest("hex").slice(0, 16);
  console.log(JSON.stringify({ id: KEY_ID, source: SOURCE, publicKey: pub, rightsHolder: false, catalog: BASE }, null, 2));
  console.log(`Fingerprint (SHA-256 of the public key, first 16 hex): ${fingerprint}`);
}

function files(dir) {
  return readdirSync(dir).flatMap((name) => {
    const p = join(dir, name);
    return statSync(p).isDirectory() ? files(p) : [p];
  });
}

function signVersion(version) {
  if (!CHANGELOG[version]) throw new Error(`unknown version ${version}; add it to CHANGELOG`);
  const dir = join(root, "files", version);
  const entries = files(dir)
    .sort()
    .map((p) => {
      const path = relative(dir, p).split(sep).join("/");
      const bytes = readFileSync(p);
      return {
        path,
        size: bytes.length,
        sha256: createHash("sha256").update(bytes).digest("hex"),
        url: `${BASE}files/${version}/${path}`,
      };
    });
  const manifest = {
    schema: 1,
    source: SOURCE,
    gameId: GAME_ID,
    name: NAME,
    version,
    changelog: CHANGELOG[version],
    hosts: [HOST],
    files: entries,
  };
  const payload = Buffer.from(JSON.stringify(manifest, null, 2));
  const signature = sign(null, payload, createPrivateKey(readFileSync(keyFile, "utf8")));
  const envelope = { keyId: KEY_ID, payload: payload.toString("base64"), signature: signature.toString("base64") };
  writeFileSync(join(root, `${GAME_ID}.json`), JSON.stringify(envelope, null, 2) + "\n");
  writeFileSync(join(root, "index.json"), JSON.stringify([GAME_ID]) + "\n");
  console.log(`Signed ${NAME} ${version}: ${entries.length} files. Commit demo-source/ and merge to main to publish.`);
}

const [command, arg] = process.argv.slice(2);
if (command === "keygen") keygen();
else if (command === "sign" && arg) signVersion(arg);
else {
  console.error("usage: node scripts/demo-source.mjs keygen | sign <version>");
  process.exit(1);
}
