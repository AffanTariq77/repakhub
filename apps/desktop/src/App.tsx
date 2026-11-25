import { useEffect, useRef, useState } from "react";
import {
  AvailableUpdate,
  CatalogEntry,
  DEFAULT_SETTINGS,
  Installation,
  JobEvent,
  Settings,
  getSettings,
  installGame,
  listGames,
  onJob,
  pauseJob,
  saveSettings,
  scanLibrary,
  updateGame,
} from "./api";
import Downloads from "./Downloads";
import Sources from "./Sources";
import { CoverArtContext } from "./games";
import Dashboard from "./Dashboard";
import Library from "./Library";
import Onboarding from "./Onboarding";
import SettingsPage from "./SettingsPage";
import Updates from "./Updates";

const PAGES = [
  { id: "dashboard", label: "Dashboard", hint: "Overview and activity" },
  { id: "library", label: "My Library", hint: "All installed games" },
  { id: "downloads", label: "Downloads", hint: "Queue and progress" },
  { id: "updates", label: "Updates", hint: "Available updates" },
  { id: "dlc", label: "DLC & Add-ons", hint: "Additional content" },
  { id: "integrations", label: "Sources", hint: "Authorised sources and what they offer" },
  { id: "settings", label: "Settings", hint: "Application preferences" },
] as const;

type PageId = (typeof PAGES)[number]["id"];

export default function App() {
  const [page, setPage] = useState<PageId>("dashboard");
  const [games, setGames] = useState<Installation[]>([]);
  const [scanning, setScanning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  const scanned = useRef(false); // a finished scan beats the slower startup list load
  const [jobs, setJobs] = useState<Record<string, JobEvent>>({});
  const [available, setAvailable] = useState<AvailableUpdate[]>([]);
  // How to restart each job (Resume / Retry): installs and updates are started again; downloaded parts are kept.
  const restart = useRef<Record<string, () => Promise<string>>>({});

  useEffect(() => {
    getSettings().then(setSettings, (e) => setError(String(e)));
    listGames().then((g) => scanned.current || setGames(g), (e) => setError(String(e)));
    const unlisten = onJob((e) => {
      setJobs((j) => ({ ...j, [e.id]: e }));
      if (e.phase === "done") {
        scanned.current = true;
        listGames().then((g) => {
          setGames(g);
          // Drop offers for games that are now up to date.
          setAvailable((a) => a.filter((u) => g.find((x) => x.id === u.installationId)?.updateAvailable));
        }, (err) => setError(String(err)));
      }
    });
    return () => {
      unlisten.then((stop) => stop());
    };
  }, []);

  const startJob = (start: () => Promise<string>) =>
    start().then(
      (id) => {
        restart.current[id] = start;
        setPage("downloads");
      },
      (e) => setError(String(e)),
    );
  const install = (e: CatalogEntry, parent: string | null) => startJob(() => installGame(e.source, e.gameId, parent));
  const update = (installationId: number) => startJob(() => updateGame(installationId));
  const pendingUpdates = games.filter((g) => g.updateAvailable).length;

  const resetSettings = async () => {
    try {
      setSettings(await saveSettings(DEFAULT_SETTINGS));
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  const scan = async () => {
    setScanning(true);
    setError(null);
    try {
      const result = await scanLibrary();
      scanned.current = true;
      setGames(result);
    } catch (e) {
      setError(String(e));
    } finally {
      setScanning(false);
    }
  };

  const current = PAGES.find((p) => p.id === page)!;

  if (!settings && !error) return null; // avoid flashing the shell before we know about onboarding
  if (settings && !settings.onboarded)
    return (
      <Onboarding
        onDone={(s) => {
          setSettings(s);
          setPage("library");
          scan();
        }}
      />
    );

  return (
    <CoverArtContext.Provider value={settings?.coverArt ?? false}>
    <div className="flex h-full">
      <nav className="w-60 shrink-0 border-r border-line bg-surface p-4 flex flex-col gap-1">
        <div className="px-3 pt-2 pb-6">
          <div className="text-lg font-bold tracking-wide bg-gradient-to-r from-accent to-accent-2 bg-clip-text text-transparent">
            REPAKHUB
          </div>
          <div className="text-xs text-muted">One library. Every game.</div>
        </div>
        {PAGES.map((p) => (
          <button
            key={p.id}
            onClick={() => setPage(p.id)}
            className={`text-left rounded-lg px-3 py-2 text-sm transition-colors ${
              page === p.id ? "bg-raised text-white" : "text-muted hover:text-white hover:bg-raised/60"
            }`}
          >
            {p.label}
            {p.id === "updates" && pendingUpdates > 0 && (
              <span className="ml-2 rounded-full bg-accent-2/20 px-2 text-xs text-accent-2">{pendingUpdates}</span>
            )}
          </button>
        ))}
      </nav>

      <main className="flex-1 overflow-y-auto">
        <header className="flex items-center justify-between px-8 pt-8 pb-6">
          <div>
            <h1 className="text-2xl font-semibold">{current.label}</h1>
            <p className="text-sm text-muted">{current.hint}</p>
          </div>
          <button
            onClick={scan}
            disabled={scanning}
            className="rounded-lg bg-accent px-4 py-2 text-sm font-medium hover:bg-accent/85 disabled:opacity-50"
          >
            {scanning ? "Scanning…" : "Scan for games"}
          </button>
        </header>

        {error && (
          <div className="mx-8 mb-6 flex items-center gap-4 rounded-lg border border-red-500/40 bg-red-500/10 px-4 py-3 text-sm text-red-300">
            <span className="flex-1">{error}</span>
            {!settings && (
              <button onClick={resetSettings} className="shrink-0 rounded-lg border border-red-400/50 px-3 py-1 hover:bg-red-500/20">
                Reset settings
              </button>
            )}
          </div>
        )}

        <div className="px-8 pb-8">
          {page === "dashboard" && <Dashboard games={games} />}
          {page === "library" && <Library games={games} onGamesChanged={setGames} />}
          {page === "updates" && (
            <Updates games={games} onGamesChanged={setGames} available={available} onAvailable={setAvailable} onUpdate={update} />
          )}
          {page === "downloads" && (
            <Downloads
              jobs={Object.values(jobs)}
              onPause={(id) => pauseJob(id).catch((e) => setError(String(e)))}
              onResume={(id) => restart.current[id]?.()}
              onDismiss={(id) => setJobs(({ [id]: _, ...rest }) => rest)}
            />
          )}
          {page === "integrations" && <Sources onInstall={install} />}
          {page === "settings" && settings && <SettingsPage settings={settings} onSaved={setSettings} />}
          {!["dashboard", "library", "updates", "downloads", "integrations", "settings"].includes(page) && (
            <div className="rounded-xl border border-dashed border-line p-12 text-center text-muted">
              Not built yet. See docs/PLAN.md for when this lands.
            </div>
          )}
        </div>
      </main>
    </div>
    </CoverArtContext.Provider>
  );
}
