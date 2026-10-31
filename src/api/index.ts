import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  AppError,
  AppInfo,
  GameDataStatus,
  GameExited,
  InstallOptions,
  KnownDump,
  InstalledSummary,
  JobEvent,
  LoadedCatalog,
  Preflight,
  Progress,
  ReleaseInfo,
  Settings,
  SteamStatus,
  UpdateInfo,
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
  /** Update to the latest release; a job like `install`. */
  update(id: string): Promise<number>;
  /** Swap back to the version before the last update; a job like `install`. */
  restorePrevious(id: string): Promise<number>;
  jobCancel(job: number): Promise<void>;
  libraryList(): Promise<InstalledSummary[]>;
  updatesCheck(): Promise<UpdateInfo[]>;
  gamedataStatus(id: string): Promise<GameDataStatus>;
  /** Check the user's dump at `path` and copy it into the game folder. */
  gamedataImport(id: string, path: string): Promise<KnownDump>;
  /** Start the game; `onGameExited` reports when it ends. */
  launch(id: string): Promise<void>;
  runningList(): Promise<string[]>;
  openLog(id: string): Promise<void>;
  logTail(id: string): Promise<string>;
  onGameExited(handler: (event: GameExited) => void): Promise<() => void>;
  shortcutCreate(id: string, desktop: boolean): Promise<InstalledSummary>;
  shortcutRemove(id: string): Promise<InstalledSummary>;
  steamStatus(): Promise<SteamStatus>;
  /** Steam must be closed. */
  steamAdd(id: string, userIds: number[]): Promise<InstalledSummary>;
  steamRemove(id: string): Promise<InstalledSummary>;
  /** Fetches and checks the catalog at `url` before saving it. */
  sourceAdd(url: string): Promise<Settings>;
  sourceRemove(url: string): Promise<Settings>;
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
  update: (id) => invoke("update", { id }),
  restorePrevious: (id) => invoke("restore_previous", { id }),
  jobCancel: (job) => invoke("job_cancel", { job }),
  libraryList: () => invoke("library_list"),
  updatesCheck: () => invoke("updates_check"),
  gamedataStatus: (id) => invoke("gamedata_status", { id }),
  gamedataImport: (id, path) => invoke("gamedata_import", { id, path }),
  launch: (id) => invoke("launch", { id }),
  runningList: () => invoke("running_list"),
  openLog: (id) => invoke("open_log", { id }),
  logTail: (id) => invoke("log_tail", { id }),
  onGameExited: (handler) => listen<GameExited>("game:exited", (e) => handler(e.payload)),
  shortcutCreate: (id, desktop) => invoke("shortcut_create", { id, desktop }),
  shortcutRemove: (id) => invoke("shortcut_remove", { id }),
  steamStatus: () => invoke("steam_status"),
  steamAdd: (id, userIds) => invoke("steam_add", { id, userIds }),
  steamRemove: (id) => invoke("steam_remove", { id }),
  sourceAdd: (url) => invoke("source_add", { url }),
  sourceRemove: (url) => invoke("source_remove", { url }),
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
    update: (id) => load().then((b) => b.update(id)),
    restorePrevious: (id) => load().then((b) => b.restorePrevious(id)),
    jobCancel: (job) => load().then((b) => b.jobCancel(job)),
    libraryList: () => load().then((b) => b.libraryList()),
    updatesCheck: () => load().then((b) => b.updatesCheck()),
    gamedataStatus: (id) => load().then((b) => b.gamedataStatus(id)),
    gamedataImport: (id, p) => load().then((b) => b.gamedataImport(id, p)),
    launch: (id) => load().then((b) => b.launch(id)),
    runningList: () => load().then((b) => b.runningList()),
    openLog: (id) => load().then((b) => b.openLog(id)),
    logTail: (id) => load().then((b) => b.logTail(id)),
    onGameExited: (h) => load().then((b) => b.onGameExited(h)),
    shortcutCreate: (id, d) => load().then((b) => b.shortcutCreate(id, d)),
    shortcutRemove: (id) => load().then((b) => b.shortcutRemove(id)),
    steamStatus: () => load().then((b) => b.steamStatus()),
    steamAdd: (id, u) => load().then((b) => b.steamAdd(id, u)),
    steamRemove: (id) => load().then((b) => b.steamRemove(id)),
    sourceAdd: (url) => load().then((b) => b.sourceAdd(url)),
    sourceRemove: (url) => load().then((b) => b.sourceRemove(url)),
    onJobEvent: (h) => load().then((b) => b.onJobEvent(h)),
  };
}

export const api: Backend = inTauri || !import.meta.env.DEV ? tauriBackend : lazyMock();
