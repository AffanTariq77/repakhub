import { useEffect, useState } from "react";
import { Installation, Settings, getSettings, listGames, scanLibrary } from "./api";
import Dashboard from "./Dashboard";
import Library from "./Library";
import Onboarding from "./Onboarding";
import SettingsPage from "./SettingsPage";

const PAGES = [
  { id: "dashboard", label: "Dashboard", hint: "Overview and activity" },
  { id: "library", label: "My Library", hint: "All installed games" },
  { id: "downloads", label: "Downloads", hint: "Queue and progress" },
  { id: "updates", label: "Updates", hint: "Available updates" },
  { id: "dlc", label: "DLC & Add-ons", hint: "Additional content" },
  { id: "integrations", label: "Integrations", hint: "Connected sources" },
  { id: "settings", label: "Settings", hint: "Application preferences" },
] as const;

type PageId = (typeof PAGES)[number]["id"];

export default function App() {
  const [page, setPage] = useState<PageId>("dashboard");
  const [games, setGames] = useState<Installation[]>([]);
  const [scanning, setScanning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);

  useEffect(() => {
    getSettings().then(setSettings, (e) => setError(String(e)));
    listGames().then(setGames, (e) => setError(String(e)));
  }, []);

  const scan = async () => {
    setScanning(true);
    setError(null);
    try {
      setGames(await scanLibrary());
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
          <div className="mx-8 mb-6 rounded-lg border border-red-500/40 bg-red-500/10 px-4 py-3 text-sm text-red-300">
            {error}
          </div>
        )}

        <div className="px-8 pb-8">
          {page === "dashboard" && <Dashboard games={games} />}
          {page === "library" && <Library games={games} />}
          {page === "settings" && settings && <SettingsPage settings={settings} onSaved={setSettings} />}
          {!["dashboard", "library", "settings"].includes(page) && (
            <div className="rounded-xl border border-dashed border-line p-12 text-center text-muted">
              Not built yet. See docs/PLAN.md for when this lands.
            </div>
          )}
        </div>
      </main>
    </div>
  );
}
