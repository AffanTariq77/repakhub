import { useState } from "react";
import { Settings, saveSettings } from "./api";
import LocationPicker from "./LocationPicker";

export default function SettingsPage({ settings, onSaved }: { settings: Settings; onSaved: (s: Settings) => void }) {
  const [selected, setSelected] = useState(settings.scanLocations);
  const [status, setStatus] = useState<string | null>(null);
  const dirty = JSON.stringify([...selected].sort()) !== JSON.stringify(settings.scanLocations);

  const save = async () => {
    try {
      onSaved(await saveSettings({ ...settings, scanLocations: selected }));
      setStatus("Saved. Run a scan to pick up changes.");
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
