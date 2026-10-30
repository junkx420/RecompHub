import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  AppError,
  AppInfo,
  InstallOptions,
  InstalledSummary,
  JobEvent,
  LoadedCatalog,
  Preflight,
  Progress,
  ReleaseInfo,
  Settings,
} from "./types";

export interface Backend {
  appInfo(): Promise<AppInfo>;
  settingsGet(): Promise<Settings>;
  settingsSet(settings: Settings): Promise<Settings>;
  catalogGet(): Promise<LoadedCatalog>;
  catalogRefresh(): Promise<LoadedCatalog>;
  releaseInfo(id: string): Promise<ReleaseInfo>;
  installPreflight(id: string): Promise<Preflight>;
  /** Starts the job and resolves with its id; the outcome arrives as events. */
  install(id: string, options?: InstallOptions): Promise<number>;
  jobCancel(job: number): Promise<void>;
  libraryList(): Promise<InstalledSummary[]>;
  /** Subscribe to job events; resolves with an unsubscribe function. */
  onJobEvent(handler: (event: JobEvent) => void): Promise<() => void>;
}

async function listenJobs(handler: (event: JobEvent) => void): Promise<() => void> {
  const unlisten = await Promise.all([
    listen<{ job: number; id: string; progress: Progress }>("job:progress", (e) =>
      handler({ kind: "progress", ...e.payload }),
    ),
    listen<{ job: number; id: string; app: InstalledSummary }>("job:finished", (e) =>
      handler({ kind: "finished", ...e.payload }),
    ),
    listen<{ job: number; id: string; error: AppError }>("job:failed", (e) =>
      handler({ kind: "failed", ...e.payload }),
    ),
  ]);
  return () => unlisten.forEach((u) => u());
}

const tauriBackend: Backend = {
  appInfo: () => invoke("app_info"),
  settingsGet: () => invoke("settings_get"),
  settingsSet: (settings) => invoke("settings_set", { settings }),
  catalogGet: () => invoke("catalog_get"),
  catalogRefresh: () => invoke("catalog_refresh"),
  releaseInfo: (id) => invoke("release_info", { id }),
  installPreflight: (id) => invoke("install_preflight", { id }),
  install: (id, options) => invoke("install", { id, options: options ?? null }),
  jobCancel: (job) => invoke("job_cancel", { job }),
  libraryList: () => invoke("library_list"),
  onJobEvent: listenJobs,
};

export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/**
 * Outside Tauri (plain `npm run dev` in a browser) a mock backend serves the
 * embedded catalog and simulates installs, so the UI can be developed and
 * screenshotted without Rust.
 */
function lazyMock(): Backend {
  const load = () => import("./mock").then((m) => m.mockBackend);
  return {
    appInfo: () => load().then((b) => b.appInfo()),
    settingsGet: () => load().then((b) => b.settingsGet()),
    settingsSet: (s) => load().then((b) => b.settingsSet(s)),
    catalogGet: () => load().then((b) => b.catalogGet()),
    catalogRefresh: () => load().then((b) => b.catalogRefresh()),
    releaseInfo: (id) => load().then((b) => b.releaseInfo(id)),
    installPreflight: (id) => load().then((b) => b.installPreflight(id)),
    install: (id, o) => load().then((b) => b.install(id, o)),
    jobCancel: (job) => load().then((b) => b.jobCancel(job)),
    libraryList: () => load().then((b) => b.libraryList()),
    onJobEvent: (h) => load().then((b) => b.onJobEvent(h)),
  };
}

export const api: Backend = inTauri || !import.meta.env.DEV ? tauriBackend : lazyMock();
