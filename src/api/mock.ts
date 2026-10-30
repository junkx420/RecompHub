import catalogJson from "../../catalog/catalog.json";
import type { Backend } from "./index";
import type { CatalogEntry, InstalledSummary, JobEvent, LoadedCatalog, Settings } from "./types";

const LIBRARY = "C:\\Users\\dev\\AppData\\Local\\RecompHub\\apps";

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

const installed = new Map<string, InstalledSummary>();
const handlers = new Set<(event: JobEvent) => void>();
const cancelled = new Set<number>();
let nextJob = 1;

function emit(event: JobEvent) {
  handlers.forEach((h) => h(event));
}

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

/** Walk through the phases of a real install with fake numbers. */
async function simulate(job: number, entry: CatalogEntry) {
  const id = entry.id;
  const total = 48 * 1024 * 1024;
  emit({ kind: "progress", job, id, progress: { phase: "resolving" } });
  await sleep(300);
  for (let done = 0; done <= total; done += total / 40) {
    if (cancelled.has(job)) {
      emit({ kind: "failed", job, id, error: { code: "cancelled" } });
      return;
    }
    emit({ kind: "progress", job, id, progress: { phase: "downloading", done, total } });
    await sleep(60);
  }
  emit({ kind: "progress", job, id, progress: { phase: "verifying" } });
  await sleep(300);
  for (let done = 0; done <= 120; done += 20) {
    emit({ kind: "progress", job, id, progress: { phase: "extracting", done, total: 120 } });
    await sleep(80);
  }
  emit({ kind: "progress", job, id, progress: { phase: "finalizing" } });
  await sleep(200);
  const app: InstalledSummary = {
    id,
    name: entry.name,
    version: "v1.0.0",
    dir: `${LIBRARY}\\${id}`,
    exe: entry.exe,
    installed_at: new Date().toISOString(),
    file_count: 120,
  };
  installed.set(id, app);
  emit({ kind: "finished", job, id, app });
}

export const mockBackend: Backend = {
  appInfo: async () => ({
    version: "0.0.0-dev",
    data_dir: "C:\\Users\\dev\\AppData\\Local\\RecompHub",
    library_dir: settings.library_dir ?? LIBRARY,
    default_library_dir: LIBRARY,
  }),
  settingsGet: async () => settings,
  settingsSet: async (next) => {
    settings = next;
    return settings;
  },
  catalogGet: async () => catalog(),
  catalogRefresh: async () => catalog(),
  releaseInfo: async (id) => ({
    tag: "v1.0.0",
    name: null,
    published_at: "2026-01-15T12:00:00Z",
    html_url: `https://github.com/${catalog().entries.find((e) => e.id === id)?.repo ?? "o/r"}/releases`,
    asset_name: `${id}-Windows.zip`,
    asset_size: 48 * 1024 * 1024,
    verified: true,
  }),
  installPreflight: async (id) => {
    const app = installed.get(id);
    return app
      ? { state: "installed", version: app.version, target: app.dir }
      : { state: "ready", target: `${LIBRARY}\\${id}` };
  },
  install: async (id) => {
    const entry = catalog().entries.find((e) => e.id === id);
    if (!entry) throw { code: "unknown_app", params: { id } };
    const job = nextJob++;
    void simulate(job, entry);
    return job;
  },
  jobCancel: async (job) => {
    cancelled.add(job);
  },
  libraryList: async () => [...installed.values()],
  onJobEvent: async (handler) => {
    handlers.add(handler);
    return () => handlers.delete(handler);
  },
};
