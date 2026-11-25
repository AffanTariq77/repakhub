import { useState } from "react";
import { AvailableUpdate, Installation, checkUpdates, formatBytes } from "./api";
import GameDetails from "./GameDetails";
import { LauncherButton, SOURCE_LABELS, Version, isManaged, sourceLabel } from "./games";

export default function Updates({
  games,
  onGamesChanged,
  available,
  onAvailable,
  onUpdate,
}: {
  games: Installation[];
  onGamesChanged: (games: Installation[]) => void;
  /** What the last signed-source check found, by installation id. */
  available: AvailableUpdate[];
  onAvailable: (updates: AvailableUpdate[]) => void;
  onUpdate: (installationId: number) => void;
}) {
  const [checking, setChecking] = useState(false);
  const managed = games.filter(isManaged);
  const check = () => {
    setChecking(true);
    checkUpdates()
      .then(onAvailable, (e) => setError(String(e)))
      .finally(() => setChecking(false));
  };
  // Keep only the id: the panel always shows the current scan's data, and closes if the game is gone.
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const selected = games.find((g) => g.id === selectedId) ?? null;
  const [error, setError] = useState<string | null>(null);

  const pending = games.filter((g) => g.updateAvailable);
  const unknownBySource = Object.entries(
    games.filter((g) => g.updateAvailable === null).reduce<Record<string, number>>((acc, g) => {
      acc[g.source] = (acc[g.source] ?? 0) + 1;
      return acc;
    }, {}),
  );

  return (
    <div className="space-y-6">
      {error && <div className="text-sm text-red-300">{error}</div>}

      {managed.length > 0 && (
        <div className="flex items-center gap-3 text-sm text-muted">
          <span className="flex-1">
            {managed.length} {managed.length === 1 ? "game" : "games"} installed from registered sources. Checking contacts
            those sources only.
          </span>
          <button onClick={check} disabled={checking} className="rounded-lg border border-line px-3 py-1.5 hover:bg-raised disabled:opacity-50">
            {checking ? "Checking…" : "Check for updates"}
          </button>
        </div>
      )}

      {pending.length ? (
        <div className="divide-y divide-line rounded-xl border border-line bg-surface">
          {pending.map((g) => (
            <div key={g.id} className="flex items-center gap-4 px-4 py-3">
              <button onClick={() => setSelectedId(g.id)} className="min-w-0 flex-1 text-left">
                <div className="truncate font-medium">{g.name}</div>
                <div className="text-xs text-muted">
                  {sourceLabel(g)} · <Version g={g} />
                </div>
              </button>
              {isManaged(g) ? <ManagedUpdate g={g} available={available} onUpdate={onUpdate} /> : <LauncherButton g={g} onError={setError} />}
            </div>
          ))}
        </div>
      ) : (
        <div className="rounded-xl border border-dashed border-line p-12 text-center text-muted">
          No pending updates reported by your launchers.
        </div>
      )}

      {unknownBySource.length > 0 && (
        <p className="text-sm text-muted">
          Update state is unknown for{" "}
          {unknownBySource.map(([source, n]) => `${n} ${SOURCE_LABELS[source] ?? source}`).join(" and ")}{" "}
          {unknownBySource.reduce((n, [, c]) => n + c, 0) === 1 ? "game" : "games"}: those launchers do not record
          pending updates locally, so RepakHub does not guess.
        </p>
      )}

      <p className="text-sm text-muted">
        Launcher games are updated by their own launcher: RepakHub shows what is pending and opens it. Games installed
        from registered sources update here, after their signed manifest and every file are verified.
      </p>

      {selected && (
        <GameDetails game={selected} onClose={() => setSelectedId(null)} onGamesChanged={onGamesChanged} />
      )}
    </div>
  );
}

function ManagedUpdate({ g, available, onUpdate }: { g: Installation; available: AvailableUpdate[]; onUpdate: (id: number) => void }) {
  const update = available.find((u) => u.installationId === g.id);
  if (!update) return <span className="text-xs text-muted">Check for updates to see details</span>;
  return (
    <button
      onClick={() => onUpdate(g.id)}
      title={update.changelog || undefined}
      className="rounded-lg bg-accent px-3 py-1.5 text-sm font-medium hover:bg-accent/85"
    >
      Update to {update.version} ({formatBytes(update.sizeBytes)})
    </button>
  );
}
