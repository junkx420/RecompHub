import catalogJson from "../../catalog/catalog.json";
import type { Backend } from "./index";
import type { CatalogEntry, LoadedCatalog, Settings } from "./types";

let settings: Settings = {
  language: "system",
  theme: "system",
  library_dir: null,
  catalog_url: "https://raw.githubusercontent.com/junkx420/ReCompX-Store/main/catalog/catalog.json",
  check_updates: true,
  desktop_shortcut: false,
};

function catalog(): LoadedCatalog {
  const raw = catalogJson.entries as unknown as Partial<CatalogEntry>[];
  const entries = raw.map(
    (e) => ({ preserve: [], user_data_paths: [], credits: [], ...e, source: { kind: "official" } }) as CatalogEntry,
  );
  return { entries, origin: "embedded", fetched_at: null, warnings: [] };
}

export const mockBackend: Backend = {
  appInfo: async () => ({
    version: "0.0.0-dev",
    data_dir: "C:\\Users\\dev\\AppData\\Local\\RecompHub",
    library_dir: settings.library_dir ?? "C:\\Users\\dev\\AppData\\Local\\RecompHub\\apps",
    default_library_dir: "C:\\Users\\dev\\AppData\\Local\\RecompHub\\apps",
  }),
  settingsGet: async () => settings,
  settingsSet: async (next) => {
    settings = next;
    return settings;
  },
  catalogGet: async () => catalog(),
  catalogRefresh: async () => catalog(),
};
