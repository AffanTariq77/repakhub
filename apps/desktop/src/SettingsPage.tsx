import { openUrl } from "@tauri-apps/plugin-opener";
import { useState } from "react";
import { Settings, saveSettings } from "./api";
import LocationPicker from "./LocationPicker";

export default function SettingsPage({ settings, onSaved }: { settings: Settings; onSaved: (s: Settings) => void }) {
  const [selected, setSelected] = useState(settings.scanLocations);
  const [coverArt, setCoverArt] = useState(settings.coverArt);
  const [steamApiKey, setSteamApiKey] = useState(settings.steamApiKey);
  const [steamId, setSteamId] = useState(settings.steamId);
  const [status, setStatus] = useState<string | null>(null);
  const dirty =
    JSON.stringify([...selected].sort()) !== JSON.stringify(settings.scanLocations) ||
    coverArt !== settings.coverArt ||
    steamApiKey !== settings.steamApiKey ||
    steamId !== settings.steamId;

  const save = async () => {
    try {
      onSaved(await saveSettings({ ...settings, scanLocations: selected, coverArt, steamApiKey, steamId }));
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
            Show cover art
            <span className="block text-muted">
              Images load from Steam's and GOG's servers, which can see which games' art is requested. Off means name
              tiles only.
            </span>
          </span>
        </label>
      </div>
      <div className="space-y-2 pt-4">
        <h2 className="text-lg font-medium">Owned Steam games</h2>
        <p className="text-sm text-muted">
          To list Steam games you own but have not installed, RepakHub asks Steam's Web API using your own key. The key
          is stored on this computer only. Your profile's game details must be public.{" "}
          <button onClick={() => openUrl("https://steamcommunity.com/dev/apikey")} className="text-accent underline">
            Get a key
          </button>
        </p>
        <input
          type="password"
          value={steamApiKey}
          onChange={(e) => setSteamApiKey(e.target.value)}
          placeholder="Steam Web API key"
          autoComplete="off"
          className="w-96 rounded-lg border border-line bg-surface px-3 py-2 text-sm outline-none focus:border-accent"
        />
        <input
          value={steamId}
          onChange={(e) => setSteamId(e.target.value)}
          placeholder="SteamID64 (blank: the account signed in to Steam)"
          className="block w-96 rounded-lg border border-line bg-surface px-3 py-2 text-sm outline-none focus:border-accent"
        />
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
