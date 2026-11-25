import { useState } from "react";
import { Settings, saveSettings } from "./api";
import LocationPicker from "./LocationPicker";

export default function SettingsPage({ settings, onSaved }: { settings: Settings; onSaved: (s: Settings) => void }) {
  const [selected, setSelected] = useState(settings.scanLocations);
  const [coverArt, setCoverArt] = useState(settings.coverArt);
  const [status, setStatus] = useState<string | null>(null);
  const dirty =
    JSON.stringify([...selected].sort()) !== JSON.stringify(settings.scanLocations) || coverArt !== settings.coverArt;

  const save = async () => {
    try {
      onSaved(await saveSettings({ ...settings, scanLocations: selected, coverArt }));
      setStatus("Saved. Run a scan to pick up location changes.");
    } catch (e) {
      setStatus(String(e));
    }
  };

  return (
    <section className="max-w-2xl space-y-4">
      <div>
        <h2 className="text-lg font-medium">Scan locations</h2>
        <p className="text-sm text-muted">
          Searched up to two folders deep for game libraries. Default Steam locations are always included.
        </p>
      </div>
      <LocationPicker selected={selected} onChange={setSelected} />
      <div className="pt-4">
        <h2 className="text-lg font-medium">Privacy</h2>
        <label className="mt-2 flex items-start gap-3 text-sm">
          <input type="checkbox" checked={coverArt} onChange={(e) => setCoverArt(e.target.checked)} className="mt-0.5 accent-accent" />
          <span>
            Show Steam cover art
            <span className="block text-muted">
              Images load from Steam's servers, which can see which games' art is requested. Off means name tiles only.
            </span>
          </span>
        </label>
      </div>
      <div className="flex items-center gap-3">
        <button
          onClick={save}
          disabled={!dirty}
          className="rounded-lg bg-accent px-4 py-2 text-sm font-medium hover:bg-accent/85 disabled:opacity-40"
        >
          Save
        </button>
        {status && <span className="text-sm text-muted">{status}</span>}
      </div>
    </section>
  );
}
