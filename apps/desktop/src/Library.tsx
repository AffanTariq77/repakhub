import { useState } from "react";
import { Installation, formatBytes } from "./api";

// ponytail: Steam CDN art, falls back to a name tile offline; read Steam's local librarycache when offline art matters.
const coverUrl = (g: Installation) =>
  g.source === "steam" ? `https://cdn.cloudflare.steamstatic.com/steam/apps/${g.sourceGameId}/header.jpg` : null;

const SOURCE_LABELS: Record<string, string> = { steam: "Steam", gog: "GOG", epic: "Epic" };

function Version({ g }: { g: Installation }) {
  if (!g.installedVersion) return <span className="text-amber-300/80">Version unknown</span>;
  return <span>{g.versionSource?.endsWith("_buildid") ? `Build ${g.installedVersion}` : g.installedVersion}</span>;
}

function Cover({ g }: { g: Installation }) {
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

export default function Library({ games }: { games: Installation[] }) {
  const [query, setQuery] = useState("");
  const [onlyUpdates, setOnlyUpdates] = useState(false);
  const [view, setView] = useState<"grid" | "list">("grid");

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
            <div key={g.id} className="overflow-hidden rounded-xl border border-line bg-surface">
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
            </div>
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
              <tr key={g.id} className="border-b border-line/60" title={g.path}>
                <td className="py-2.5">{g.name}</td>
                <td className="text-muted">{SOURCE_LABELS[g.source] ?? g.source}</td>
                <td className="text-muted">
                  <Version g={g} />
                </td>
                <td className="text-muted">{g.sizeBytes ? formatBytes(g.sizeBytes) : "Unknown"}</td>
                <td>
                  {g.updateAvailable === null ? (
                    <span className="text-muted">Unknown</span>
                  ) : g.updateAvailable ? (
                    <span className="text-accent-2">Update pending</span>
                  ) : (
                    <span className="text-emerald-400/80">Up to date</span>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
