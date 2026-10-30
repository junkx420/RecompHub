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

/** Serialized `AppError` from the backend. */
export interface AppError {
  code: string;
  params?: Record<string, unknown>;
}

export function isAppError(value: unknown): value is AppError {
  return typeof value === "object" && value !== null && typeof (value as AppError).code === "string";
}
