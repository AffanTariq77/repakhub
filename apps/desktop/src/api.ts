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
  /** DLC the launcher reports as installed. name is null when the launcher's files do not hold it. */
  dlc: { id: string; name: string | null }[];
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
  /** Load cover art from Steam's and GOG's image servers. */
  coverArt: boolean;
  /** The user's own Steam Web API key, for listing owned Steam games. Empty = off. */
  steamApiKey: string;
  /** SteamID64. Empty = the account signed in to Steam on this machine. */
  steamId: string;  /** Check GitHub for a newer RepakHub on startup. */
  checkAppUpdates: boolean;
};

export const DEFAULT_SETTINGS: Settings = { onboarded: false, scanLocations: [], coverArt: true, steamApiKey: "", steamId: "", checkAppUpdates: true };

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

/** A game a platform says the user owns. Mirrors `OwnedGame` in src-tauri/src/owned.rs. */
export type OwnedGame = { source: string; sourceGameId: string; name: string; launcher: "steam" | "galaxy" | "heroic" };
export type Owned = { games: OwnedGame[]; errors: string[] };
export const listOwned = () => invoke<Owned>("list_owned");

/** A GOG store listing. Mirrors `StoreGame` in src-tauri/src/store.rs. */
export type StoreGame = { id: string; title: string; storeLink: string; cover: string | null; price: string | null; discount: string | null };
export const searchGog = (query: string) => invoke<StoreGame[]>("search_gog", { query });

export const listCatalog = () => invoke<Catalog>("list_catalog");
export const checkUpdates = () => invoke<AvailableUpdate[]>("check_updates");
export const installGame = (source: string, gameId: string, parent: string | null) =>
  invoke<string>("install_game", { source, gameId, parent });
export const updateGame = (installationId: number) => invoke<string>("update_game", { installationId });
export const pauseJob = (id: string) => invoke<void>("pause_job", { id });
export const onJob = (handler: (e: JobEvent) => void) => listen<JobEvent>("repakhub://job", (e) => handler(e.payload));

export type BackupInfo = { id: string; createdAt: number; sizeBytes: number; paths: string[]; note: string | null };
/** A game's chosen save/config paths and its backups, newest first. */
export type BackupState = { paths: string[]; backups: BackupInfo[] };

const gameKey = (g: Installation) => ({ source: g.source, sourceGameId: g.sourceGameId });
export const getBackups = (g: Installation) => invoke<BackupState>("get_backups", gameKey(g));
export const setBackupPaths = (g: Installation, paths: string[]) => invoke<BackupState>("set_backup_paths", { ...gameKey(g), paths });
export const createBackup = (g: Installation) => invoke<BackupState>("create_backup", gameKey(g));
export const restoreBackup = (g: Installation, id: string) => invoke<BackupState>("restore_backup", { ...gameKey(g), id });
export const deleteBackup = (g: Installation, id: string) => invoke<BackupState>("delete_backup", { ...gameKey(g), id });

/** A way RepakHub finds games, with what it may access. */
export type Adapter = {
  id: string;
  name: string;
  description: string;
  reads: string[];
  network: string[];
  enabled: boolean;
};
export type OnlineFeature = { name: string; hosts: string[]; when: string };
export type Integrations = { adapters: Adapter[]; online: OnlineFeature[] };

export const listIntegrations = () => invoke<Integrations>("list_integrations");
export const setAdapterEnabled = (id: string, enabled: boolean) => invoke<Integrations>("set_adapter_enabled", { id, enabled });
