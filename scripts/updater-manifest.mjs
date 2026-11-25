#!/usr/bin/env node
// Writes latest.json, the feed RepakHub's updater reads from the newest GitHub release.
//
//   node scripts/updater-manifest.mjs <installers dir> <version> <tag> <owner/repo> > latest.json
//
// Expects each updater artifact next to its `.sig` (from `tauri signer sign`). Node built-ins only.

import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";

const PLATFORMS = [
  ["darwin-aarch64", (n) => n.endsWith(".app.tar.gz")],
  ["windows-x86_64", (n) => n.endsWith("-setup.exe")],
  ["linux-x86_64", (n) => n.endsWith(".AppImage")],
];

export function manifest(dir, version, tag, repo, notes = "", now = new Date()) {
  const files = (function walk(d) {
    return readdirSync(d).flatMap((n) => (statSync(join(d, n)).isDirectory() ? walk(join(d, n)) : [join(d, n)]));
  })(dir);
  const platforms = {};
  for (const [platform, matches] of PLATFORMS) {
    const artifact = files.find((f) => matches(f.split("/").pop()));
    if (!artifact) continue;
    const sig = `${artifact}.sig`;
    if (!files.includes(sig)) throw new Error(`${artifact} has no signature (${sig})`);
    const name = artifact.split("/").pop();
    platforms[platform] = {
      signature: readFileSync(sig, "utf8").trim(),
      url: `https://github.com/${repo}/releases/download/${encodeURIComponent(tag)}/${encodeURIComponent(name)}`,
    };
  }
  if (!Object.keys(platforms).length) throw new Error(`no updater artifacts in ${dir}`);
  return { version, notes, pub_date: now.toISOString(), platforms };
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const [dir, version, tag, repo] = process.argv.slice(2);
  if (!dir || !version || !tag || !repo) {
    console.error("usage: node scripts/updater-manifest.mjs <dir> <version> <tag> <owner/repo>");
    process.exit(1);
  }
  const m = manifest(dir, version, tag, repo, `RepakHub ${version}. See the release notes on GitHub.`);
  process.stdout.write(JSON.stringify(m, null, 2) + "\n");
  console.error(`latest.json: ${Object.keys(m.platforms).join(", ")}`);
}
