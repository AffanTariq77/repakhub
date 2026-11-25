import { useState } from "react";
import { Installation } from "./api";
import GameDetails from "./GameDetails";
import { dlcLabel, sourceLabel } from "./games";

export default function DlcPage({
  games,
  onGamesChanged,
}: {
  games: Installation[];
  onGamesChanged: (games: Installation[]) => void;
}) {
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const selected = games.find((g) => g.id === selectedId) ?? null;
  const withDlc = games.filter((g) => g.dlc.length > 0);
  const total = withDlc.reduce((n, g) => n + g.dlc.length, 0);

  return (
    <div className="space-y-6">
      <p className="text-sm text-muted">
        Add-ons each launcher reports as installed. Ownership is checked by the platform, not by RepakHub, and files on
        disk are never taken as proof of ownership. Steam's local files do not hold DLC names, so those show their id.
      </p>

      {withDlc.length === 0 ? (
        <div className="rounded-xl border border-dashed border-line p-12 text-center text-muted">
          No installed DLC reported by your launchers.
        </div>
      ) : (
        <>
          <div className="text-sm text-muted">
            {total} {total === 1 ? "add-on" : "add-ons"} across {withDlc.length} {withDlc.length === 1 ? "game" : "games"}
          </div>
          <div className="divide-y divide-line rounded-xl border border-line bg-surface">
            {withDlc.map((g) => (
              <div key={g.id} className="px-4 py-3">
                <button onClick={() => setSelectedId(g.id)} className="text-left font-medium hover:text-accent">
                  {g.name}
                </button>
                <span className="ml-2 text-xs text-muted">{sourceLabel(g)}</span>
                <ul className="mt-1 grid grid-cols-[repeat(auto-fill,minmax(220px,1fr))] gap-x-4 text-sm text-muted">
                  {g.dlc.map((d) => (
                    <li key={d.id} className="truncate" title={d.id}>
                      {dlcLabel(d)}
                    </li>
                  ))}
                </ul>
              </div>
            ))}
          </div>
        </>
      )}

      {selected && <GameDetails game={selected} onClose={() => setSelectedId(null)} onGamesChanged={onGamesChanged} />}
    </div>
  );
}
