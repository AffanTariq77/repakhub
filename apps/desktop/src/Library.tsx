import { useState } from "react";
import { Installation, formatBytes } from "./api";
import GameDetails from "./GameDetails";
import { Cover, UpdateStatus, Version, sourceLabel } from "./games";

export default function Library({ games }: { games: Installation[] }) {
  const [query, setQuery] = useState("");
  const [onlyUpdates, setOnlyUpdates] = useState(false);
  const [view, setView] = useState<"grid" | "list">("grid");
  const [selected, setSelected] = useState<Installation | null>(null);

  const shown = games.filter(
    (g) => g.name.toLowerCase().includes(query.toLowerCase()) && (!onlyUpdates || g.updateAvailable),
  );

  if (!games.length)
    return (
      <div className="rounded-xl border border-dashed border-line p-12 text-center text-muted">
        No games yet. Press <span className="text-white">Scan for games</span> to find your Steam, GOG and Epic games.
      </div>
    );

  return (
    <div className="space-y-5">
      <div className="flex flex-wrap items-center gap-3">
        <input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search your library"
          className="w-72 rounded-lg border border-line bg-surface px-3 py-2 text-sm outline-none focus:border-accent"
        />
        <label className="flex items-center gap-2 text-sm text-muted">
          <input type="checkbox" checked={onlyUpdates} onChange={(e) => setOnlyUpdates(e.target.checked)} />
          Updates pending only
        </label>
        <div className="ml-auto flex rounded-lg border border-line text-sm">
          {(["grid", "list"] as const).map((v) => (
            <button
              key={v}
              onClick={() => setView(v)}
              className={`px-3 py-1.5 capitalize ${view === v ? "bg-raised text-white" : "text-muted"}`}
            >
              {v}
            </button>
          ))}
        </div>
      </div>

      {view === "grid" ? (
        <div className="grid grid-cols-[repeat(auto-fill,minmax(240px,1fr))] gap-4">
          {shown.map((g) => (
            <button
              key={g.id}
              onClick={() => setSelected(g)}
              className="overflow-hidden rounded-xl border border-line bg-surface text-left transition-colors hover:border-accent/60"
            >
              <Cover g={g} />
              <div className="p-3">
                <div className="truncate font-medium" title={g.name}>
                  {g.name}
                </div>
                <div className="mt-1 flex justify-between text-xs text-muted">
                  <Version g={g} />
                  {g.updateAvailable && <span className="text-accent-2">Update pending</span>}
                </div>
              </div>
            </button>
          ))}
        </div>
      ) : (
        <table className="w-full text-sm">
          <thead className="text-left text-muted">
            <tr className="border-b border-line">
              <th className="py-2 font-normal">Name</th>
              <th className="font-normal">Source</th>
              <th className="font-normal">Version</th>
              <th className="font-normal">Size</th>
              <th className="font-normal">Status</th>
            </tr>
          </thead>
          <tbody>
            {shown.map((g) => (
              <tr
                key={g.id}
                onClick={() => setSelected(g)}
                className="cursor-pointer border-b border-line/60 hover:bg-raised/50"
                title={g.path}
              >
                <td className="py-2.5">{g.name}</td>
                <td className="text-muted">{sourceLabel(g)}</td>
                <td className="text-muted">
                  <Version g={g} />
                </td>
                <td className="text-muted">{g.sizeBytes ? formatBytes(g.sizeBytes) : "Unknown"}</td>
                <td>
                  <UpdateStatus g={g} />
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      {selected && <GameDetails game={selected} onClose={() => setSelected(null)} />}
    </div>
  );
}
