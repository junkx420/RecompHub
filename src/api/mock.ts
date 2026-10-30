import catalogJson from "../../catalog/catalog.json";
import type { Backend } from "./index";
import type { CatalogEntry, InstalledSummary, JobEvent, LoadedCatalog, Progress, Settings } from "./types";

const LIBRARY = "C:\\Users\\dev\\AppData\\Local\\RecompHub\\apps";
/** Every mock release has this tag; the pre-installed app is older. */
const LATEST = "v1.1.0";

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

function summary(entry: CatalogEntry, version: string, previous: string | null): InstalledSummary {
  return {
    id: entry.id,
    name: entry.name,
    version,
    dir: `${LIBRARY}\\${entry.id}`,
    exe: entry.exe,
    installed_at: "2026-09-01T12:00:00Z",
    updated_at: previous ? new Date().toISOString() : null,
    file_count: 120,
    previous_version: previous,
  };
}

const installed = new Map<string, InstalledSummary>();
{
  // One outdated install, so the update flow can be tried in the browser.
  const banjo = catalog().entries.find((e) => e.id === "banjorecomp");
  if (banjo) installed.set(banjo.id, summary(banjo, "v1.0.0", null));
}
const handlers = new Set<(event: JobEvent) => void>();
const cancelled = new Set<number>();
let nextJob = 1;

function emit(event: JobEvent) {
  handlers.forEach((h) => h(event));
}

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

type Kind = "install" | "update" | "restore";

/** Walk through the phases of a real job with fake numbers. */
async function simulate(job: number, entry: CatalogEntry, kind: Kind) {
  const id = entry.id;
  const step = async (progress: Progress, ms: number) => {
    if (cancelled.has(job)) throw new Error("cancelled");
    emit({ kind: "progress", job, id, progress });
    await sleep(ms);
  };
  try {
    await step({ phase: "resolving" }, 300);
    if (kind !== "restore") {
      const total = 48 * 1024 * 1024;
      for (let done = 0; done <= total; done += total / 40) {
        await step({ phase: "downloading", done, total }, 60);
      }
      await step({ phase: "verifying" }, 300);
      for (let done = 0; done <= 120; done += 20) await step({ phase: "extracting", done, total: 120 }, 80);
    }
    if (kind !== "install") {
      for (let done = 0; done <= 80; done += 10) await step({ phase: "applying", done, total: 80 }, 60);
    }
    await step({ phase: "finalizing" }, 200);
  } catch {
    emit({ kind: "failed", job, id, error: { code: "cancelled" } });
    return;
  }
  const current = installed.get(id);
  const app =
    kind === "install"
      ? summary(entry, LATEST, null)
      : kind === "update"
        ? summary(entry, LATEST, current?.version ?? null)
        : summary(entry, current?.previous_version ?? LATEST, current?.version ?? null);
  installed.set(id, app);
  emit({ kind: "finished", job, id, app });
}

function start(id: string, kind: Kind): number {
  const entry = catalog().entries.find((e) => e.id === id);
  if (!entry) throw { code: "unknown_app", params: { id } };
  if (kind !== "install" && !installed.has(id)) throw { code: "not_installed", params: { id } };
  const job = nextJob++;
  void simulate(job, entry, kind);
  return job;
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
    tag: LATEST,
    name: null,
    published_at: "2026-10-15T12:00:00Z",
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
  install: async (id) => start(id, "install"),
  update: async (id) => start(id, "update"),
  restorePrevious: async (id) => {
    if (!installed.get(id)?.previous_version) throw { code: "no_previous_version", params: { id } };
    return start(id, "restore");
  },
  jobCancel: async (job) => {
    cancelled.add(job);
  },
  libraryList: async () => [...installed.values()],
  updatesCheck: async () =>
    [...installed.values()]
      .filter((a) => a.version !== LATEST)
      .map((a) => ({ id: a.id, installed: a.version, latest: LATEST })),
  onJobEvent: async (handler) => {
    handlers.add(handler);
    return () => handlers.delete(handler);
  },
};
