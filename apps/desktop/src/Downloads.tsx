import { JobEvent, formatBytes } from "./api";

const PHASE: Record<JobEvent["phase"], string> = {
  checking: "Checking the signed manifest",
  downloading: "Downloading",
  applying: "Verifying and applying",
  done: "Installed",
  paused: "Paused",
  failed: "Failed",
};

export default function Downloads({
  jobs,
  onPause,
  onResume,
  onDismiss,
}: {
  jobs: JobEvent[];
  onPause: (id: string) => void;
  onResume: (id: string) => void;
  onDismiss: (id: string) => void;
}) {
  if (!jobs.length)
    return (
      <div className="rounded-xl border border-dashed border-line p-12 text-center text-muted">
        No downloads. Installs and updates from registered sources appear here.
      </div>
    );

  return (
    <div className="divide-y divide-line rounded-xl border border-line bg-surface">
      {jobs.map((j) => {
        const pct = j.total ? Math.min(100, Math.round((j.done / j.total) * 100)) : j.phase === "done" ? 100 : 0;
        const active = j.phase === "checking" || j.phase === "downloading" || j.phase === "applying";
        return (
          <div key={j.id} className="space-y-2 px-4 py-3">
            <div className="flex items-center gap-4">
              <div className="min-w-0 flex-1">
                <div className="truncate font-medium">{j.name}</div>
                <div className={`text-xs ${j.phase === "failed" ? "text-red-300" : "text-muted"}`}>
                  {PHASE[j.phase]}
                  {j.phase === "downloading" && j.total > 0 && ` · ${formatBytes(j.done)} of ${formatBytes(j.total)}`}
                  {j.error && `: ${j.error}`}
                </div>
              </div>
              {j.phase === "downloading" && (
                <button onClick={() => onPause(j.id)} className="rounded-lg border border-line px-3 py-1 text-sm hover:bg-raised">
                  Pause
                </button>
              )}
              {(j.phase === "paused" || j.phase === "failed") && (
                <button onClick={() => onResume(j.id)} className="rounded-lg bg-accent px-3 py-1 text-sm hover:bg-accent/85">
                  {j.phase === "paused" ? "Resume" : "Retry"}
                </button>
              )}
              {!active && (
                <button onClick={() => onDismiss(j.id)} aria-label="Dismiss" className="text-muted hover:text-white">
                  ✕
                </button>
              )}
            </div>
            {j.phase !== "failed" && (
              <div className="h-1.5 overflow-hidden rounded-full bg-raised" role="progressbar" aria-valuenow={pct} aria-valuemin={0} aria-valuemax={100}>
                <div className="h-full bg-gradient-to-r from-accent to-accent-2 transition-all" style={{ width: `${pct}%` }} />
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}
