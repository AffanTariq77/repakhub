import { invoke } from "@tauri-apps/api/core";

/** Mirrors `Installation` in src-tauri/src/lib.rs. */
export type Installation = {
  id: number;
  source: string;
  sourceGameId: string;
  name: string;
  path: string;
  installedVersion: string | null; // null = unknown, never guessed
  versionSource: string | null;
  updateAvailable: boolean | null; // null = unknown
  sizeBytes: number | null;
  lastUpdated: number | null;
  scannedAt: number;
  /** App that manages this install: steam, galaxy, epic, heroic, legendary. null = none known. */
  launcher: string | null;
};

export const listGames = () => invoke<Installation[]>("list_games");
export const scanLibrary = () => invoke<Installation[]>("scan_library");

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
