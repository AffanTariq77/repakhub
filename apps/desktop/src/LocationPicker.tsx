import { open } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";
import { listDrives } from "./api";

/** Checkbox list of detected drives plus any folders the user added. `selected` is the source of truth. */
export default function LocationPicker({
  selected,
  onChange,
}: {
  selected: string[];
  onChange: (locations: string[]) => void;
}) {
  const [drives, setDrives] = useState<string[]>([]);
  useEffect(() => {
    listDrives().then(setDrives, () => setDrives([]));
  }, []);

  // Selected folders stay listed even if the drive is currently unplugged.
  const all = [...new Set([...drives, ...selected])];
  const toggle = (p: string) => onChange(selected.includes(p) ? selected.filter((x) => x !== p) : [...selected, p]);

  const addFolder = async () => {
    const picked = await open({ directory: true, multiple: true });
    if (picked) onChange([...new Set([...selected, ...(Array.isArray(picked) ? picked : [picked])])]);
  };

  return (
    <div className="space-y-3">
      <div className="divide-y divide-line rounded-xl border border-line bg-surface">
        {all.map((p) => (
          <label key={p} className="flex cursor-pointer items-center gap-3 px-4 py-3 hover:bg-raised/50">
            <input type="checkbox" checked={selected.includes(p)} onChange={() => toggle(p)} className="accent-accent" />
            <span className="flex-1 truncate font-mono text-sm" title={p}>
              {p}
            </span>
            <span className="text-xs text-muted">{drives.includes(p) ? "Drive" : "Folder"}</span>
          </label>
        ))}
        {!all.length && <div className="px-4 py-3 text-sm text-muted">No drives detected. Add a folder instead.</div>}
      </div>
      <button onClick={addFolder} className="rounded-lg border border-line px-3 py-1.5 text-sm hover:bg-raised">
        Add folder…
      </button>
    </div>
  );
}
