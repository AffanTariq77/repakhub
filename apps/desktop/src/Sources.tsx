import { open } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";
import { Catalog, CatalogEntry, formatBytes, listCatalog } from "./api";

export default function Sources({ onInstall }: { onInstall: (e: CatalogEntry, parent: string | null) => void }) {
  const [catalog, setCatalog] = useState<Catalog | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = () => {
    setError(null);
    listCatalog().then(setCatalog, (e) => setError(String(e)));
  };
  useEffect(load, []);

  const installElsewhere = async (e: CatalogEntry) => {
    const parent = await open({ directory: true, title: "Choose where to install" });
    if (typeof parent === "string") onInstall(e, parent);
  };

  return (
    <div className="space-y-6">
      <p className="text-sm text-muted">
        Games from authorised sources only: each install is checked against a signing key built into RepakHub, every file
        is verified before it is used, and nothing outside the source's listed servers is contacted.
      </p>

      {error && <div className="text-sm text-red-300">{error}</div>}
      {!catalog && !error && <div className="text-sm text-muted">Loading sources…</div>}

      {catalog && catalog.sources === 0 && (
        <div className="rounded-xl border border-dashed border-line p-12 text-center text-muted">
          No sources are registered yet. Sources are added under RepakHub's source policy, with evidence that they may
          distribute what they offer.
        </div>
      )}

      {catalog && catalog.entries.length > 0 && (
        <div className="divide-y divide-line rounded-xl border border-line bg-surface">
          {catalog.entries.map((e) => (
            <div key={`${e.source}/${e.gameId}`} className="flex items-start gap-4 px-4 py-3">
              <div className="min-w-0 flex-1">
                <div className="font-medium">{e.name}</div>
                <div className="text-xs text-muted">
                  {e.source} · version {e.version} · {formatBytes(e.sizeBytes)}
                </div>
                {e.changelog && <div className="mt-1 text-xs text-muted">{e.changelog}</div>}
              </div>
              {e.installed ? (
                <span className="text-sm text-emerald-400/80">Installed</span>
              ) : (
                <div className="flex shrink-0 gap-2">
                  <button onClick={() => installElsewhere(e)} className="rounded-lg border border-line px-3 py-1 text-sm hover:bg-raised">
                    Install to…
                  </button>
                  <button onClick={() => onInstall(e, null)} className="rounded-lg bg-accent px-3 py-1 text-sm hover:bg-accent/85">
                    Install
                  </button>
                </div>
              )}
            </div>
          ))}
        </div>
      )}

      {catalog?.errors.map((e) => (
        <div key={e} className="text-sm text-amber-300/90">
          {e}
        </div>
      ))}

      {catalog && catalog.sources > 0 && (
        <button onClick={load} className="rounded-lg border border-line px-3 py-1.5 text-sm hover:bg-raised">
          Refresh
        </button>
      )}
    </div>
  );
}
