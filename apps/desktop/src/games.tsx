import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
import { createContext, useContext, useEffect, useState } from "react";
import { Installation, gogCovers } from "./api";

export const SOURCE_LABELS: Record<string, string> = { steam: "Steam", gog: "GOG", epic: "Epic" };

/** What covers and launcher hand-offs need, so owned-but-not-installed games can use them too. */
type GameRef = Pick<Installation, "source" | "sourceGameId" | "name" | "launcher">;

export const dlcLabel = (d: Installation["dlc"][number]) => d.name ?? `DLC ${d.id}`;

/** Installed state comes from the launcher; ownership is the platform's to decide, never inferred from files. */
export const dlcExplanation = (g: Installation) =>
  `Installed according to ${sourceLabel(g)}'s own records. Ownership is checked by the platform, not by RepakHub.`;

/**
 * Pending updates not yet announced, given the keys announced last time. Returns them and the keys to keep:
 * only games still pending, so a game that is updated and later gets another update is announced again.
 */
export function freshUpdates(games: Installation[], announced: string[]) {
  const pending = games.filter((g) => g.updateAvailable);
  const key = (g: Installation) => `${g.source}:${g.sourceGameId}`;
  return { fresh: pending.filter((g) => !announced.includes(key(g))), keep: pending.map(key) };
}

/** Games RepakHub installed itself update in-app, not through a launcher. */
export const isManaged = (g: Installation) => g.launcher === "repakhub";
export const sourceLabel = (g: Installation) => SOURCE_LABELS[g.source] ?? g.source;

/** Where each `versionSource` comes from. */
const VERSION_SOURCES: Record<string, string> = {
  manifest: "Version from the signed manifest RepakHub installed it from",
  user: "Version you entered",
  executable: "Product version embedded in the game's executable",
  steam_buildid: "Steam build ID, from Steam's app manifest",
  gog_version: "Version recorded by GOG Galaxy",
  gog_buildid: "GOG build ID, from the game's GOG metadata",
  epic_manifest: "Version from the Epic Games Launcher manifest",
  legendary: "Version from Heroic / Legendary's install list",
};

const CONFIDENCE: Record<Installation["versionConfidence"], string> = {
  manifest: "signed manifest (highest)",
  launcher: "launcher metadata (highest)",
  user: "entered by you",
  executable: "executable only (an executable's version is not always the full game version)",
  unknown: "unknown",
};

export const versionExplanation = (g: Installation) =>
  g.versionSource
    ? `${VERSION_SOURCES[g.versionSource] ?? g.versionSource}. Confidence: ${CONFIDENCE[g.versionConfidence]}.` +
      (g.versionConflict ? " Sources disagree: see Game Details." : "")
    : "No version is recorded for this game, so RepakHub shows Unknown rather than guessing.";

/** Version text; the tooltip carries its source and confidence wherever a version is shown. */
export const formatVersion = (version: string, source: string | null) =>
  source?.endsWith("_buildid") ? `Build ${version}` : version;

export function Version({ g }: { g: Installation }) {
  if (!g.installedVersion)
    return (
      <span className="text-amber-300/80" title={versionExplanation(g)}>
        Version unknown
      </span>
    );
  return (
    <span title={versionExplanation(g)} className={g.versionConflict ? "text-amber-300/90" : undefined}>
      {formatVersion(g.installedVersion, g.versionSource)}
      {g.versionConflict && " ⚠"}
    </span>
  );
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

// ponytail: art from Steam's and GOG's servers, falls back to a name tile offline; read the launchers' local
// image caches when offline art matters.
const steamCover = (id: string) => `https://cdn.cloudflare.steamstatic.com/steam/apps/${encodeURIComponent(id)}/header.jpg`;

// GOG image names need a lookup. Ids asked for in the same render go to GOG in one request; answers are
// kept for the session, and a failed lookup is forgotten so a later render tries again.
const gogCache = new Map<string, Promise<string | null>>();
let gogBatch: string[] | null = null;
let gogBatchDone: Promise<Record<string, string>>;
function gogCover(id: string) {
  let cover = gogCache.get(id);
  if (!cover) {
    if (!gogBatch) {
      const ids: string[] = (gogBatch = []);
      gogBatchDone = new Promise((r) => setTimeout(r)).then(() => {
        gogBatch = null;
        return gogCovers(ids);
      });
    }
    gogBatch.push(id);
    cover = gogBatchDone.then(
      (covers) => covers[id] ?? null,
      () => (gogCache.delete(id), null),
    );
    gogCache.set(id, cover);
  }
  return cover;
}

function useGogCover(id: string | null) {
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    setUrl(null);
    if (!id) return;
    let live = true;
    gogCover(id).then((u) => live && setUrl(u));
    return () => {
      live = false;
    };
  }, [id]);
  return url;
}

/** Whether the user agreed to load cover art from Steam's and GOG's servers (Settings.coverArt). */
export const CoverArtContext = createContext(false);

export function Cover({ g }: { g: GameRef }) {
  const [failed, setFailed] = useState(false);
  const enabled = useContext(CoverArtContext);
  const gog = useGogCover(enabled && g.source === "gog" ? g.sourceGameId : null);
  const url = !enabled ? null : g.source === "steam" ? steamCover(g.sourceGameId) : g.source === "gog" ? gog : null;
  if (!url || failed)
    return (
      <div className="aspect-[460/215] grid place-items-center bg-gradient-to-br from-raised to-surface p-4 text-center font-medium">
        {g.name}
      </div>
    );
  return <img src={url} alt="" loading="lazy" onError={() => setFailed(true)} className="aspect-[460/215] w-full object-cover" />;
}

/**
 * Hand-off to the app that manages this install (which is what installs updates), chosen by the recorded
 * launcher, never guessed from the store. Ids are URL-encoded: they come from files on disk.
 * Heroic and Legendary installs get no hand-off yet.
 */
export function launcherAction(g: GameRef): { label: string; url: string } | null {
  const id = encodeURIComponent(g.sourceGameId);
  if (g.launcher === "steam") return { label: "Open in Steam", url: `steam://nav/games/details/${id}` };
  if (g.launcher === "galaxy") return { label: "Open in GOG Galaxy", url: `goggalaxy://openGameView/${id}` };
  if (g.launcher === "epic")
    return { label: "Launch with Epic Games Launcher", url: `com.epicgames.launcher://apps/${id}?action=launch` };
  return null;
}

export function LauncherButton({ g, onError }: { g: GameRef; onError: (e: string) => void }) {
  const [busy, setBusy] = useState(false);
  const action = launcherAction(g);
  if (!action) return null;
  const open = () => {
    setBusy(true); // a second click must not launch the game twice
    openUrl(action.url)
      .catch((e) => onError(String(e)))
      .finally(() => setTimeout(() => setBusy(false), 2000));
  };
  return (
    <button
      onClick={open}
      disabled={busy}
      className="rounded-lg bg-accent px-3 py-1.5 text-sm font-medium hover:bg-accent/85 disabled:opacity-50"
    >
      {action.label}
    </button>
  );
}

export const showFolder = (g: Installation) => revealItemInDir(g.path);
