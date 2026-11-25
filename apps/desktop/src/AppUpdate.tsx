import { relaunch } from "@tauri-apps/plugin-process";
import { Update, check } from "@tauri-apps/plugin-updater";
import { useEffect, useState } from "react";
import { formatBytes } from "./api";

/**
 * Offers a newer RepakHub when one is published. The updater plugin downloads it from GitHub and
 * verifies its signature against the public key built into this app before installing anything.
 * A failed check (offline, GitHub down) stays silent: local-first means no nagging.
 */
export default function AppUpdate({ enabled }: { enabled: boolean }) {
  const [update, setUpdate] = useState<Update | null>(null);
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (enabled) check().then(setUpdate, () => {});
  }, [enabled]);

  if (!update) return null;

  const install = async () => {
    setError(null);
    let total = 0;
    let done = 0;
    try {
      await update.downloadAndInstall((e) => {
        if (e.event === "Started") total = e.data.contentLength ?? 0;
        if (e.event === "Progress") done += e.data.chunkLength;
        setProgress({ done, total });
      });
      await relaunch();
    } catch (e) {
      setProgress(null);
      setError(String(e));
    }
  };

  return (
    <div className="mx-8 mb-6 flex items-center gap-4 rounded-lg border border-accent/40 bg-accent/10 px-4 py-3 text-sm">
      <div className="flex-1">
        <span className="font-medium">RepakHub {update.version} is available.</span>
        {update.body && <span className="block text-xs text-muted">{update.body.split("\n")[0]}</span>}
        {progress && (
          <span className="block text-xs text-muted">
            Downloading {formatBytes(progress.done)}
            {progress.total > 0 && ` of ${formatBytes(progress.total)}`}… RepakHub restarts when it is installed.
          </span>
        )}
        {error && <span className="block text-xs text-red-300">{error}</span>}
      </div>
      {!progress && (
        <>
          <button onClick={() => setUpdate(null)} className="text-xs text-muted hover:text-white">
            Later
          </button>
          <button onClick={install} className="rounded-lg bg-accent px-3 py-1.5 text-sm font-medium hover:bg-accent/85">
            Install and restart
          </button>
        </>
      )}
    </div>
  );
}
