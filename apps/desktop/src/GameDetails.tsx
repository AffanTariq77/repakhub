import { useEffect, useState } from "react";
import { Installation, formatBytes } from "./api";
import {
  Cover,
  LauncherButton,
  UpdateStatus,
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

export default function GameDetails({ game: g, onClose }: { game: Installation; onClose: () => void }) {
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
            <Row label="Version" hint={versionExplanation(g)}>
              <Version g={g} />
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
            <Row label="Size">{g.sizeBytes ? formatBytes(g.sizeBytes) : "Unknown"}</Row>
            {g.lastUpdated && (
              <Row label="Last updated">{new Date(g.lastUpdated * 1000).toLocaleDateString("en-GB")}</Row>
            )}
          </div>

          {error && <div className="mt-4 text-sm text-red-300">{error}</div>}

          <div className="mt-6 flex justify-end">
            <LauncherButton g={g} onError={setError} />
          </div>
        </div>
      </div>
    </div>
  );
}
