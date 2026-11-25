import { useState } from "react";
import { Installation } from "./api";
import GameDetails from "./GameDetails";
import { LauncherButton, SOURCE_LABELS, Version, sourceLabel } from "./games";

export default function Updates({ games }: { games: Installation[] }) {
  const [selected, setSelected] = useState<Installation | null>(null);
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

      {pending.length ? (
        <div className="divide-y divide-line rounded-xl border border-line bg-surface">
          {pending.map((g) => (
            <div key={g.id} className="flex items-center gap-4 px-4 py-3">
              <button onClick={() => setSelected(g)} className="min-w-0 flex-1 text-left">
                <div className="truncate font-medium">{g.name}</div>
                <div className="text-xs text-muted">
                  {sourceLabel(g)} · <Version g={g} />
                </div>
              </button>
              <LauncherButton g={g} onError={setError} />
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
        Updates are installed by each game's own launcher. RepakHub shows what is pending and opens the right launcher.
      </p>

      {selected && <GameDetails game={selected} onClose={() => setSelected(null)} />}
    </div>
  );
}
