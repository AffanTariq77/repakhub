import { useEffect, useState } from "react";
import { Integrations as IntegrationsData, listIntegrations, setAdapterEnabled } from "./api";

/** Launcher adapters (switchable, with what each reads) and online features (what contacts the network, and when). */
export default function Integrations({ onChanged }: { onChanged: () => void }) {
  const [data, setData] = useState<IntegrationsData | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    listIntegrations().then(setData, (e) => setError(String(e)));
  }, []);

  const toggle = (id: string, enabled: boolean) =>
    setAdapterEnabled(id, enabled).then(
      (d) => {
        setData(d);
        onChanged(); // rescan so switched-off adapters' games leave the library
      },
      (e) => setError(String(e)),
    );

  if (!data) return error ? <div className="text-sm text-red-300">{error}</div> : null;

  return (
    <div className="space-y-6">
      <section className="space-y-3">
        <h2 className="text-lg font-medium">Launchers</h2>
        <div className="grid gap-3 md:grid-cols-2">
          {data.adapters.map((a) => (
            <div key={a.id} className={`rounded-xl border border-line bg-surface p-4 ${a.enabled ? "" : "opacity-60"}`}>
              <div className="flex items-start gap-3">
                <div className="flex-1">
                  <div className="font-medium">{a.name}</div>
                  <div className="text-xs text-muted">{a.description}</div>
                </div>
                <label className="flex items-center gap-2 text-xs text-muted">
                  <input
                    type="checkbox"
                    checked={a.enabled}
                    onChange={(e) => toggle(a.id, e.target.checked)}
                    className="accent-accent"
                    aria-label={`Use ${a.name}`}
                  />
                  {a.enabled ? "On" : "Off"}
                </label>
              </div>
              <details className="mt-3 text-xs">
                <summary className="cursor-pointer text-muted hover:text-white">What it can access</summary>
                <ul className="mt-2 list-disc space-y-1 pl-4 text-muted">
                  {a.reads.map((r) => (
                    <li key={r}>Reads {r}</li>
                  ))}
                  {a.network.length === 0 ? (
                    <li>Never uses the network</li>
                  ) : (
                    a.network.map((h) => <li key={h}>Contacts {h}</li>)
                  )}
                  <li>Changes nothing outside RepakHub's own folders</li>
                </ul>
              </details>
            </div>
          ))}
        </div>
        {error && <div className="text-sm text-red-300">{error}</div>}
      </section>

      <section className="space-y-3">
        <h2 className="text-lg font-medium">Online features</h2>
        <p className="text-sm text-muted">Scanning never uses the network. These features do, only when you use them.</p>
        <ul className="divide-y divide-line rounded-xl border border-line bg-surface text-sm">
          {data.online.map((f) => (
            <li key={f.name} className="px-4 py-2">
              <span className="font-medium">{f.name}</span>
              <span className="text-muted"> · {f.when}</span>
              <div className="font-mono text-xs text-muted">{f.hosts.join(", ")}</div>
            </li>
          ))}
        </ul>
      </section>
    </div>
  );
}
