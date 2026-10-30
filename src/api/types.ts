// Mirrors of the Rust types in crates/core. Keep in sync by hand.

export type LocalizedText = string | Record<string, string>;

export type Category = "game" | "emulator" | "tool";

export type EntrySource = { kind: "official" } | { kind: "custom"; url: string };

export interface KnownDump {
  sha1?: string;
  md5?: string;
  region: string;
  version: string;
}

export type GameData =
  | { mode: "none" }
  | { mode: "port_handles_it"; hint: LocalizedText }
  | {
      mode: "copy_file";
      target: string;
      accepted_hashes: KnownDump[];
      other_known_hashes: KnownDump[];
      hint?: LocalizedText;
    };

export interface CatalogEntry {
  id: string;
  name: string;
  category: Category;
  system: string;
  repo: string;
  asset_pattern: string;
  release_tag?: string;
  exe: string;
  preserve: string[];
  user_data_paths: string[];
  game_data: GameData;
  description: LocalizedText;
  credits: string[];
  license: string;
  homepage: string;
  icon_url?: string;
  source: EntrySource;
}

export type CatalogOrigin = "remote" | "cache" | "embedded";

export interface LoadedCatalog {
  entries: CatalogEntry[];
  origin: CatalogOrigin;
  fetched_at: string | null;
  warnings: string[];
}

export type Language = "system" | "de" | "en";
export type Theme = "system" | "light" | "dark";

export interface Settings {
  language: Language;
  theme: Theme;
  library_dir: string | null;
  catalog_url: string;
  check_updates: boolean;
  desktop_shortcut: boolean;
}

export interface AppInfo {
  version: string;
  data_dir: string;
  library_dir: string;
  default_library_dir: string;
}

export interface ReleaseInfo {
  tag: string;
  name: string | null;
  published_at: string | null;
  html_url: string;
  asset_name: string;
  asset_size: number;
  /** GitHub published a SHA-256 for the asset, so the download is verified. */
  verified: boolean;
}

export type Preflight =
  | { state: "ready"; target: string }
  | { state: "installed"; version: string; target: string }
  | { state: "foreign_content"; target: string };

export interface InstallOptions {
  replace_foreign?: boolean;
}

export type Progress =
  | { phase: "resolving" }
  | { phase: "downloading"; done: number; total: number }
  | { phase: "verifying" }
  | { phase: "extracting"; done: number; total: number }
  | { phase: "applying"; done: number; total: number }
  | { phase: "finalizing" };

export interface UpdateInfo {
  id: string;
  installed: string;
  latest: string;
}

export interface InstalledSummary {
  id: string;
  name: string;
  version: string;
  dir: string;
  exe: string;
  installed_at: string;
  updated_at: string | null;
  file_count: number;
  previous_version: string | null;
}

export type JobEvent =
  | { kind: "progress"; job: number; id: string; progress: Progress }
  | { kind: "finished"; job: number; id: string; app: InstalledSummary }
  | { kind: "failed"; job: number; id: string; error: AppError };

/** Serialized `AppError` from the backend. */
export interface AppError {
  code: string;
  params?: Record<string, unknown>;
}

export function isAppError(value: unknown): value is AppError {
  return typeof value === "object" && value !== null && typeof (value as AppError).code === "string";
}
