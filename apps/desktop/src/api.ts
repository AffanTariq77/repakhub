import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/** Mirrors `Installation` in src-tauri/src/lib.rs. */
export type Installation = {
  id: number;
  source: string;
  sourceGameId: string;
  name: string;
  path: string;
  /** Resolved version: launcher first, then the user's, then the executable's. null = unknown, never guessed. */
  installedVersion: string | null;
  versionSource: string | null;
  versionConfidence: "manifest" | "launcher" | "user" | "executable" | "unknown";
  /** Two comparable sources disagree; every raw value is below. */
  versionConflict: boolean;
  launcherVersion: string | null;
  exeVersion: string | null;
  userVersion: string | null;
  updateAvailable: boolean | null; // null = unknown
  sizeBytes: number | null;
  lastUpdated: number | null;
  scannedAt: number;
  /** App that manages this install: steam, galaxy, epic, heroic, legendary. null = none known. */
  launcher: string | null;
};

export const listGames = () => invoke<Installation[]>("list_games");
export const scanLibrary = () => invoke<Installation[]>("scan_library");
/** Set, or with null clear, the version the user entered. Returns the updated library. */
export const setVersionOverride = (g: Installation, version: string | null) =>
  invoke<Installation[]>("set_version_override", { source: g.source, sourceGameId: g.sourceGameId, version });

export function formatBytes(n: number) {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let i = 0;
  while (n >= 1024 && i < units.length - 1) (n /= 1024), i++;
  return `${n.toFixed(i ? 1 : 0)} ${units[i]}`;
}

/** Mirrors `Settings` in src-tauri/src/settings.rs. */
export type Settings = {
  onboarded: boolean;
  scanLocations: string[];
  /** Load Steam cover art from Steam's image servers. */
  coverArt: boolean;
};

export const DEFAULT_SETTINGS: Settings = { onboarded: false, scanLocations: [], coverArt: true };

export const getSettings = () => invoke<Settings>("get_settings");
export const saveSettings = (settings: Settings) => invoke<Settings>("save_settings", { settings });
export const listDrives = () => invoke<string[]>("list_drives");

/** A game an authorised source offers; only listed when its signed manifest verified. */
export type CatalogEntry = {
  source: string;
  gameId: string;
  name: string;
  version: string;
  changelog: string;
  sizeBytes: number;
  installed: boolean;
};
export type Catalog = { entries: CatalogEntry[]; errors: string[]; sources: number };

export type AvailableUpdate = { installationId: number; version: string; changelog: string; sizeBytes: number };

export type JobEvent = {
  id: string;
  name: string;
  phase: "checking" | "downloading" | "applying" | "done" | "paused" | "failed";
  done: number;
  total: number;
  error: string | null;
};

export const listCatalog = () => invoke<Catalog>("list_catalog");
export const checkUpdates = () => invoke<AvailableUpdate[]>("check_updates");
export const installGame = (source: string, gameId: string, parent: string | null) =>
  invoke<string>("install_game", { source, gameId, parent });
export const updateGame = (installationId: number) => invoke<string>("update_game", { installationId });
export const pauseJob = (id: string) => invoke<void>("pause_job", { id });
export const onJob = (handler: (e: JobEvent) => void) => listen<JobEvent>("repakhub://job", (e) => handler(e.payload));
