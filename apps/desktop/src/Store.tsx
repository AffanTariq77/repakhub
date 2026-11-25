import { openUrl } from "@tauri-apps/plugin-opener";
import { FormEvent, useContext, useEffect, useState } from "react";
import { StoreGame, searchGog } from "./api";
import { CoverArtContext } from "./games";

export default function Store() {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<StoreGame[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const coverArt = useContext(CoverArtContext);

  const search = (q: string) => {
    setBusy(true);
    setError(null);
    searchGog(q)
      .then(setResults, (e) => setError(String(e)))
      .finally(() => setBusy(false));
  };
  useEffect(() => search(""), []);

  const submit = (e: FormEvent) => {
    e.preventDefault();
    search(query);
  };

  return (
    <div className="space-y-5">
      <p className="text-sm text-muted">
        Listings come from GOG's public catalogue, so this page contacts gog.com. Buy and download on GOG: RepakHub
        then finds the game when you scan.
      </p>
      <form onSubmit={submit} className="flex gap-3">
        <input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search GOG"
          className="w-72 rounded-lg border border-line bg-surface px-3 py-2 text-sm outline-none focus:border-accent"
        />
        <button disabled={busy} className="rounded-lg bg-accent px-4 py-2 text-sm font-medium hover:bg-accent/85 disabled:opacity-50">
          {busy ? "Searching…" : "Search"}
        </button>
      </form>

      {error && <div className="text-sm text-red-300">{error}</div>}
      {results && !results.length && <div className="text-sm text-muted">No games match.</div>}

      <div className="grid grid-cols-[repeat(auto-fill,minmax(240px,1fr))] gap-4">
        {results?.map((g) => (
          <div key={g.id} className="overflow-hidden rounded-xl border border-line bg-surface">
            {coverArt && g.cover ? (
              <img src={g.cover} alt="" loading="lazy" className="aspect-[460/215] w-full object-cover" />
            ) : (
              <div className="aspect-[460/215] grid place-items-center bg-gradient-to-br from-raised to-surface p-4 text-center font-medium">
                {g.title}
              </div>
            )}
            <div className="flex items-center justify-between gap-2 p-3">
              <div className="min-w-0">
                <div className="truncate font-medium" title={g.title}>
                  {g.title}
                </div>
                <div className="text-xs text-muted">
                  {g.price ?? "Free or not for sale"}
                  {g.discount && <span className="ml-2 text-accent-2">{g.discount}</span>}
                </div>
              </div>
              <button
                onClick={() => openUrl(g.storeLink).catch((e) => setError(String(e)))}
                className="shrink-0 rounded-lg border border-line px-3 py-1.5 text-sm hover:bg-raised"
              >
                Open on GOG
              </button>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
