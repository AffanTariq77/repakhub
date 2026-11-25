import { useState } from "react";
import { Settings, saveSettings } from "./api";
import LocationPicker from "./LocationPicker";

export default function Onboarding({ onDone }: { onDone: (s: Settings) => void }) {
  const [selected, setSelected] = useState<string[]>([]);
  const [coverArt, setCoverArt] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const finish = async (scanLocations: string[]) => {
    try {
      onDone(await saveSettings({ onboarded: true, scanLocations, coverArt }));
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div className="flex h-full items-center justify-center overflow-y-auto p-8">
      <div className="w-full max-w-xl space-y-8">
        <div>
          <div className="text-sm font-bold tracking-wide bg-gradient-to-r from-accent to-accent-2 bg-clip-text text-transparent">
            REPAKHUB
          </div>
          <h1 className="mt-2 text-3xl font-semibold">Where are your games?</h1>
          <p className="mt-2 text-muted">
            Pick the drives or folders that hold your games. Steam in its default location is found automatically.
          </p>
        </div>

        <LocationPicker selected={selected} onChange={setSelected} />

        <div className="rounded-xl border border-line bg-surface/50 p-4 text-sm text-muted">
          RepakHub only reads launcher metadata in these locations. It does not modify game files, upload anything, or
          need an account. You can change this later in Settings.
        </div>

        <label className="flex items-start gap-3 text-sm">
          <input type="checkbox" checked={coverArt} onChange={(e) => setCoverArt(e.target.checked)} className="mt-0.5 accent-accent" />
          <span>
            Show Steam cover art
            <span className="block text-muted">
              Images load from Steam's servers, which can see which games' art is requested. Off means name tiles only.
            </span>
          </span>
        </label>

        {error && <div className="text-sm text-red-300">{error}</div>}

        <div className="flex items-center justify-end gap-3">
          <button onClick={() => finish([])} className="px-4 py-2 text-sm text-muted hover:text-white">
            Skip
          </button>
          <button
            onClick={() => finish(selected)}
            className="rounded-lg bg-accent px-5 py-2 text-sm font-medium hover:bg-accent/85"
          >
            Scan for games
          </button>
        </div>
      </div>
    </div>
  );
}
