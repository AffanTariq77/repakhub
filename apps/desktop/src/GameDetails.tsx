import { useEffect, useState } from "react";
import { Installation, formatBytes, setVersionOverride } from "./api";
import Backups from "./Backups";
import {
  Cover,
  LauncherButton,
  UpdateStatus,
  dlcExplanation,
  dlcLabel,
  formatVersion,
  launcherAction,
  Version,
  showFolder,
  sourceLabel,
  updateExplanation,
  versionExplanation,
} from "./games";

function Row({ label, children, hint }: { label: string; children: React.ReactNode; hint?: string }) {
  return (
    <div className="grid grid-cols-[9rem_1fr] gap-3 py-3">
      <div className="text-sm text-muted">{label}</div>
      <div className="min-w-0 text-sm">
        {children}
        {hint && <div className="mt-1 text-xs text-muted">{hint}</div>}
      </div>
    </div>
  );
}

/** Every strategy's raw value, so disagreement is visible rather than silently resolved. */
function VersionSources({ g }: { g: Installation }) {
  const rows: [string, string | null][] = [
    // When the launcher has a version it always wins, so the shown source is the launcher's.
    ["Launcher", g.launcherVersion && formatVersion(g.launcherVersion, g.versionSource)],
    ["You", g.userVersion],
    ["Executable", g.exeVersion],
  ];
  const known = rows.filter(([, v]) => v);
  if (known.length < 2 && !g.versionConflict) return null;
  return (
    <div className={`mt-2 rounded-lg border p-2 text-xs ${g.versionConflict ? "border-amber-400/40 bg-amber-400/5" : "border-line"}`}>
      {g.versionConflict && <div className="mb-1 text-amber-300">Sources disagree. RepakHub shows the most trusted one.</div>}
      {known.map(([label, v]) => (
        <div key={label} className="flex justify-between gap-4 text-muted">
          <span>{label}</span>
          <span className="font-mono text-zinc-200">{v}</span>
        </div>
      ))}
    </div>
  );
}

/** User confirmation: the last-resort strategy when no launcher records a version. */
function ManualVersion({ g, onSaved, onError }: { g: Installation; onSaved: (games: Installation[]) => void; onError: (e: string) => void }) {
  const [value, setValue] = useState(g.userVersion ?? "");
  if (g.versionConfidence === "launcher" && !g.userVersion) return null;
  const save = (v: string | null) => setVersionOverride(g, v).then(onSaved, (e) => onError(String(e)));
  return (
    <form
      className="mt-2 flex items-center gap-2"
      onSubmit={(e) => {
        e.preventDefault();
        save(value.trim() || null);
      }}
    >
      <input
        value={value}
        onChange={(e) => setValue(e.target.value)}
        placeholder="Enter the installed version"
        aria-label="Installed version"
        className="w-48 rounded-lg border border-line bg-bg px-2 py-1 text-xs outline-none focus:border-accent"
      />
      <button type="submit" className="rounded-lg border border-line px-2 py-1 text-xs hover:bg-raised">
        Save
      </button>
      {g.userVersion && (
        <button type="button" onClick={() => save(null)} className="text-xs text-muted hover:text-white">
          Clear
        </button>
      )}
    </form>
  );
}

export default function GameDetails({
  game: g,
  alsoInstalled = [],
  onSelect,
  onClose,
  onGamesChanged,
}: {
  game: Installation;
  /** Copies of the same game from other sources (see `gameKey`). */
  alsoInstalled?: Installation[];
  onSelect?: (id: number) => void;
  onClose: () => void;
  onGamesChanged: (games: Installation[]) => void;
}) {
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div className="fixed inset-0 z-10 grid place-items-center bg-black/60 p-6" onClick={onClose}>
      <div
        role="dialog"
        aria-modal="true"
        aria-label={g.name}
        className="max-h-full w-full max-w-2xl overflow-y-auto rounded-2xl border border-line bg-surface"
        onClick={(e) => e.stopPropagation()}
      >
        <Cover g={g} />
        <div className="p-6">
          <div className="flex items-start justify-between gap-4">
            <h2 className="text-xl font-semibold">{g.name}</h2>
            <button onClick={onClose} aria-label="Close" className="text-muted hover:text-white">
              ✕
            </button>
          </div>

          <div className="mt-4 divide-y divide-line">
            <Row label="Source">{sourceLabel(g)}</Row>
            {alsoInstalled.length > 0 && (
              <Row label="Also installed" hint="Matched by name. Each copy keeps its own version, updates and backups.">
                <div className="flex flex-wrap gap-2">
                  {alsoInstalled.map((o) => (
                    <button
                      key={o.id}
                      onClick={() => onSelect?.(o.id)}
                      title={o.path}
                      className="rounded-lg border border-line px-3 py-1 text-xs hover:border-accent/60"
                    >
                      {sourceLabel(o)}
                    </button>
                  ))}
                </div>
              </Row>
            )}
            <Row label="Version" hint={versionExplanation(g)}>
              <Version g={g} />
              <VersionSources g={g} />
              <ManualVersion key={g.id} g={g} onSaved={onGamesChanged} onError={setError} />
            </Row>
            <Row label="Update" hint={updateExplanation(g)}>
              <UpdateStatus g={g} />
            </Row>
            <Row label="Install folder">
              <div className="flex items-center gap-3">
                <span className="truncate font-mono text-xs" title={g.path}>
                  {g.path}
                </span>
                <button
                  onClick={() => showFolder(g).catch((e) => setError(String(e)))}
                  className="shrink-0 rounded-lg border border-line px-2 py-1 text-xs hover:bg-raised"
                >
                  Show folder
                </button>
              </div>
            </Row>
            {g.dlc.length > 0 && (
              <Row label="DLC & add-ons" hint={dlcExplanation(g)}>
                <ul className="space-y-0.5">
                  {g.dlc.map((d) => (
                    <li key={d.id}>{dlcLabel(d)}</li>
                  ))}
                </ul>
              </Row>
            )}
            <Row label="Saves & config" hint="Backups are plain folders in RepakHub's data folder.">
              <Backups key={g.id} g={g} />
            </Row>
            <Row label="Size">{g.sizeBytes ? formatBytes(g.sizeBytes) : "Unknown"}</Row>
            {g.lastUpdated && (
              <Row label="Last updated">{new Date(g.lastUpdated * 1000).toLocaleDateString("en-GB")}</Row>
            )}
          </div>

          {error && <div className="mt-4 text-sm text-red-300">{error}</div>}

          <div className="mt-6 flex items-center justify-end gap-4">
            {!launcherAction(g) && (
              <span className="text-sm text-muted">Manage this game in the launcher that installed it, such as Heroic.</span>
            )}
            <LauncherButton g={g} onError={setError} />
          </div>
        </div>
      </div>
    </div>
  );
}
