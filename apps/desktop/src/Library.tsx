import { useEffect, useState } from "react";
import { Installation, Owned, formatBytes, listOwned } from "./api";
import GameDetails from "./GameDetails";
import { Cover, LauncherButton, SOURCE_LABELS, UpdateStatus, Version, gameKey, groupByGame, sourceLabel } from "./games";

export default function Library({
  games,
  onGamesChanged,
}: {
  games: Installation[];
  onGamesChanged: (games: Installation[]) => void;
}) {
  const [query, setQuery] = useState("");
  const [onlyUpdates, setOnlyUpdates] = useState(false);
  const [view, setView] = useState<"grid" | "list">("grid");
  // Keep only the id: the panel always shows the current scan's data, and closes if the game is gone.
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const selected = games.find((g) => g.id === selectedId) ?? null;
  const alsoInstalled = selected ? games.filter((g) => g.id !== selected.id && gameKey(g.name) === gameKey(selected.name)) : [];
  const [owned, setOwned] = useState<Owned | null>(null);
  const [launchError, setLaunchError] = useState<string | null>(null);
  useEffect(() => {
    listOwned().then(setOwned, (e) => setOwned({ games: [], errors: [String(e)] }));
  }, []);
  const notInstalled = (owned?.games ?? []).filter(
    (o) =>
      !games.some((g) => g.source === o.source && g.sourceGameId === o.sourceGameId) &&
      o.name.toLowerCase().includes(query.toLowerCase()),
  );
  const messages = [...(owned?.errors ?? []), ...(launchError ? [launchError] : [])].map((m) => (
    <div key={m} className="text-sm text-amber-300/90">
      {m}
    </div>
  ));

  const shown = games.filter(
    (g) => g.name.toLowerCase().includes(query.toLowerCase()) && (!onlyUpdates || g.updateAvailable),
  );

  if (!games.length && !owned?.games.length)
    return (
      <div className="space-y-3">
        <div className="rounded-xl border border-dashed border-line p-12 text-center text-muted">
          No games yet. Press <span className="text-white">Scan for games</span> to find your Steam, GOG and Epic games.
          <span className="mt-2 block text-sm">
            Games you own but have not installed appear here too: add a Steam Web API key in Settings, or sign in to GOG
            Galaxy or Heroic.
          </span>
        </div>
        {messages}
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
          {/* One card per game: copies from several sources share it, and Game Details lists them all. */}
          {groupByGame(shown).map(([g, ...others]) => (
            <button
              key={g.id}
              onClick={() => setSelectedId(g.id)}
              className="overflow-hidden rounded-xl border border-line bg-surface text-left transition-colors hover:border-accent/60"
            >
              <Cover g={g} />
              <div className="p-3">
                <div className="truncate font-medium" title={g.name}>
                  {g.name}
                </div>
                <div className="mt-1 flex justify-between gap-2 text-xs text-muted">
                  {others.length ? <span>{[g, ...others].map(sourceLabel).join(" · ")}</span> : <Version g={g} />}
                  {[g, ...others].some((x) => x.updateAvailable) && <span className="text-accent-2">Update pending</span>}
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
                onClick={() => setSelectedId(g.id)}
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

      {notInstalled.length > 0 && !onlyUpdates && (
        <section className="space-y-3 pt-4">
          <div>
            <h2 className="text-lg font-medium">
              Owned, not installed <span className="text-sm text-muted">{notInstalled.length}</span>
            </h2>
            <p className="text-xs text-muted">As each platform reports it. Install from the platform's own app.</p>
          </div>
          <div className="grid grid-cols-[repeat(auto-fill,minmax(240px,1fr))] gap-4">
            {groupByGame(notInstalled).map((copies) => (
              <div key={`${copies[0].source}/${copies[0].sourceGameId}`} className="overflow-hidden rounded-xl border border-line bg-surface">
                <Cover g={copies[0]} />
                <div className="space-y-2 p-3">
                  <div className="truncate font-medium" title={copies[0].name}>
                    {copies[0].name}
                  </div>
                  {copies.map((o) => (
                    <div key={`${o.source}/${o.sourceGameId}`} className="flex items-center justify-between gap-2">
                      <div className="text-xs text-muted">
                        {SOURCE_LABELS[o.source] ?? o.source}
                        {o.launcher === "heroic" && " · in Heroic"}
                      </div>
                      <LauncherButton g={o} onError={setLaunchError} />
                    </div>
                  ))}
                </div>
              </div>
            ))}
          </div>
        </section>
      )}
      {messages}

      {selected && (
        <GameDetails
          game={selected}
          alsoInstalled={alsoInstalled}
          onSelect={setSelectedId}
          onClose={() => setSelectedId(null)}
          onGamesChanged={onGamesChanged}
        />
      )}
    </div>
  );
}
