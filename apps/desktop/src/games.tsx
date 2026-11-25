import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
import { useState } from "react";
import { Installation } from "./api";

export const SOURCE_LABELS: Record<string, string> = { steam: "Steam", gog: "GOG", epic: "Epic" };
export const sourceLabel = (g: Installation) => SOURCE_LABELS[g.source] ?? g.source;

/** Where each `versionSource` comes from. All current sources are launcher metadata (highest confidence). */
const VERSION_SOURCES: Record<string, string> = {
  steam_buildid: "Steam build ID, from Steam's app manifest",
  gog_version: "Version recorded by GOG Galaxy",
  gog_buildid: "GOG build ID, from the game's GOG metadata",
  epic_manifest: "Version from the Epic Games Launcher manifest",
  legendary: "Version from Heroic / Legendary's install list",
};

export const versionExplanation = (g: Installation) =>
  g.versionSource
    ? `${VERSION_SOURCES[g.versionSource] ?? g.versionSource}. Confidence: launcher metadata.`
    : "The launcher does not record a version for this game, so RepakHub shows Unknown rather than guessing.";

export function Version({ g }: { g: Installation }) {
  if (!g.installedVersion) return <span className="text-amber-300/80">Version unknown</span>;
  return <span>{g.versionSource?.endsWith("_buildid") ? `Build ${g.installedVersion}` : g.installedVersion}</span>;
}

export function UpdateStatus({ g }: { g: Installation }) {
  if (g.updateAvailable === null) return <span className="text-muted">Unknown</span>;
  return g.updateAvailable ? (
    <span className="text-accent-2">Update pending</span>
  ) : (
    <span className="text-emerald-400/80">Up to date</span>
  );
}

export const updateExplanation = (g: Installation) =>
  g.updateAvailable === null
    ? `${sourceLabel(g)} does not record pending updates locally, so this is unknown.`
    : `As reported by ${sourceLabel(g)}.`;

// ponytail: Steam CDN art, falls back to a name tile offline; read Steam's local librarycache when offline art matters.
const coverUrl = (g: Installation) =>
  g.source === "steam"
    ? `https://cdn.cloudflare.steamstatic.com/steam/apps/${encodeURIComponent(g.sourceGameId)}/header.jpg`
    : null;

export function Cover({ g }: { g: Installation }) {
  const [failed, setFailed] = useState(false);
  const url = coverUrl(g);
  if (!url || failed)
    return (
      <div className="aspect-[460/215] grid place-items-center bg-gradient-to-br from-raised to-surface p-4 text-center font-medium">
        {g.name}
      </div>
    );
  return <img src={url} alt="" onError={() => setFailed(true)} className="aspect-[460/215] w-full object-cover" />;
}

// GOG Galaxy and the Epic Games Launcher only exist on Windows (and macOS); on Linux these games come via Heroic.
const isWindows = typeof navigator !== "undefined" && navigator.userAgent.includes("Windows");

/** Hand-off to the game's own launcher, which is what installs updates. Ids are URL-encoded: they come from files on disk. */
export function launcherAction(g: Installation): { label: string; url: string } | null {
  const id = encodeURIComponent(g.sourceGameId);
  if (g.source === "steam") return { label: "Open in Steam", url: `steam://nav/games/details/${id}` };
  if (g.source === "gog" && isWindows) return { label: "Open in GOG Galaxy", url: `goggalaxy://openGameView/${id}` };
  if (g.source === "epic" && isWindows)
    return { label: "Launch with Epic Games Launcher", url: `com.epicgames.launcher://apps/${id}?action=launch` };
  return null;
}

export function LauncherButton({ g, onError }: { g: Installation; onError: (e: string) => void }) {
  const action = launcherAction(g);
  if (!action) return null;
  return (
    <button
      onClick={() => openUrl(action.url).catch((e) => onError(String(e)))}
      className="rounded-lg bg-accent px-3 py-1.5 text-sm font-medium hover:bg-accent/85"
    >
      {action.label}
    </button>
  );
}

export const showFolder = (g: Installation) => revealItemInDir(g.path);
