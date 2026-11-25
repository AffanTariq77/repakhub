import { confirm, open } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";
import {
  BackupState,
  Installation,
  createBackup,
  deleteBackup,
  formatBytes,
  getBackups,
  restoreBackup,
  setBackupPaths,
} from "./api";

/** Save and config backups for one game: the user picks paths, RepakHub snapshots and restores them. */
export default function Backups({ g }: { g: Installation }) {
  const [state, setState] = useState<BackupState | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    getBackups(g).then(setState, (e) => setError(String(e)));
  }, [g.source, g.sourceGameId]);

  const run = (label: string, action: () => Promise<BackupState>) => {
    setBusy(label);
    setError(null);
    action()
      .then(setState, (e) => setError(String(e)))
      .finally(() => setBusy(null));
  };

  const add = async (directory: boolean) => {
    const picked = await open({ directory, multiple: true, title: directory ? "Choose save or config folders" : "Choose files" });
    if (!picked || !state) return;
    const more = Array.isArray(picked) ? picked : [picked];
    run("Saving", () => setBackupPaths(g, [...state.paths, ...more]));
  };

  const restore = async (id: string, when: string) => {
    const ok = await confirm(
      `Restore the backup from ${when}? Current files at these paths are replaced. RepakHub backs them up first, so this can be undone.`,
      { title: "Restore backup", kind: "warning" },
    );
    if (ok) run("Restoring", () => restoreBackup(g, id));
  };

  if (!state) return error ? <div className="text-xs text-red-300">{error}</div> : null;
  const date = (ms: number) => new Date(ms).toLocaleString("en-GB", { dateStyle: "medium", timeStyle: "short" });

  return (
    <div className="space-y-3">
      {state.paths.length === 0 ? (
        <div className="text-xs text-muted">Choose the folders or files where this game keeps saves and settings.</div>
      ) : (
        <ul className="space-y-1">
          {state.paths.map((p) => (
            <li key={p} className="flex items-center gap-2 text-xs">
              <span className="flex-1 truncate font-mono" title={p}>
                {p}
              </span>
              <button
                onClick={() => run("Saving", () => setBackupPaths(g, state.paths.filter((x) => x !== p)))}
                className="text-muted hover:text-white"
                aria-label={`Stop backing up ${p}`}
              >
                Remove
              </button>
            </li>
          ))}
        </ul>
      )}

      <div className="flex flex-wrap gap-2">
        <button onClick={() => add(true)} className="rounded-lg border border-line px-2 py-1 text-xs hover:bg-raised">
          Add folder…
        </button>
        <button onClick={() => add(false)} className="rounded-lg border border-line px-2 py-1 text-xs hover:bg-raised">
          Add file…
        </button>
        <button
          onClick={() => run("Backing up", () => createBackup(g))}
          disabled={!state.paths.length || !!busy}
          className="rounded-lg bg-accent px-2 py-1 text-xs font-medium hover:bg-accent/85 disabled:opacity-40"
        >
          {busy ?? "Back up now"}
        </button>
      </div>

      {error && <div className="text-xs text-red-300">{error}</div>}

      {state.backups.length > 0 && (
        <ul className="divide-y divide-line rounded-lg border border-line">
          {state.backups.map((b) => (
            <li key={b.id} className="flex items-center gap-3 px-2 py-1.5 text-xs">
              <span className="flex-1">
                {date(b.createdAt)} · {formatBytes(b.sizeBytes)}
                {b.note && <span className="block text-muted">{b.note}</span>}
              </span>
              <button onClick={() => restore(b.id, date(b.createdAt))} disabled={!!busy} className="hover:text-accent disabled:opacity-40">
                Restore
              </button>
              <button
                onClick={() => run("Deleting", () => deleteBackup(g, b.id))}
                disabled={!!busy}
                className="text-muted hover:text-red-300 disabled:opacity-40"
              >
                Delete
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
