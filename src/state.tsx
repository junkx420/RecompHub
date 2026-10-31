import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { api } from "./api";
import type {
  AppError,
  AppInfo,
  GameExited,
  InstallOptions,
  InstalledSummary,
  JobEvent,
  LoadedCatalog,
  Progress,
  Settings,
  Theme,
  UninstallOutcome,
} from "./api/types";
import i18n, { resolveLanguage } from "./i18n";

/** A running job for one app. `progress` is null until the first event. */
export interface JobState {
  job: number;
  progress: Progress | null;
}

interface AppContextValue {
  settings: Settings | null;
  info: AppInfo | null;
  catalog: LoadedCatalog | null;
  catalogError: unknown;
  catalogLoading: boolean;
  installed: InstalledSummary[];
  /** Running jobs by app id. */
  jobs: Record<string, JobState>;
  /** Last failed job per app id (cleared on the next start). */
  jobErrors: Record<string, AppError>;
  /** Latest release tag per installed app, from the last update check. */
  latest: Record<string, string>;
  checkingUpdates: boolean;
  /** Apps started from RecompHub that are still running. */
  running: Record<string, true>;
  /** Exit code of the last run per app id (null: unknown). */
  lastExit: Record<string, number | null>;
  play(id: string): Promise<void>;
  /** Put a record the backend returned into the library list. */
  replaceInstalled(app: InstalledSummary): void;
  saveSettings(next: Settings): Promise<void>;
  uninstallApp(id: string, keepSaves: boolean): Promise<UninstallOutcome>;
  /** Add or remove a custom catalog source, then reload the catalog. */
  changeSource(action: "add" | "remove", url: string): Promise<void>;
  reloadCatalog(): Promise<void>;
  reloadLibrary(): Promise<void>;
  checkUpdates(): Promise<void>;
  startInstall(id: string, options?: InstallOptions): Promise<void>;
  startUpdate(id: string): Promise<void>;
  startRestore(id: string): Promise<void>;
  cancelJob(id: string): Promise<void>;
}

const AppContext = createContext<AppContextValue | null>(null);

function applyTheme(theme: Theme) {
  const root = document.documentElement;
  if (theme === "system") root.removeAttribute("data-theme");
  else root.setAttribute("data-theme", theme);
}

function without<T>(record: Record<string, T>, key: string): Record<string, T> {
  if (!(key in record)) return record;
  return Object.fromEntries(Object.entries(record).filter(([k]) => k !== key));
}

export function AppProvider({ children }: { children: ReactNode }) {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [catalog, setCatalog] = useState<LoadedCatalog | null>(null);
  const [catalogError, setCatalogError] = useState<unknown>(null);
  const [catalogLoading, setCatalogLoading] = useState(true);
  const [installed, setInstalled] = useState<InstalledSummary[]>([]);
  const [jobs, setJobs] = useState<Record<string, JobState>>({});
  const [jobErrors, setJobErrors] = useState<Record<string, AppError>>({});
  const endedJobs = useRef(new Set<number>());
  const [running, setRunning] = useState<Record<string, true>>({});
  const [lastExit, setLastExit] = useState<Record<string, number | null>>({});
  const [latest, setLatest] = useState<Record<string, string>>({});
  const [checkingUpdates, setCheckingUpdates] = useState(false);

  const checkUpdates = useCallback(async () => {
    setCheckingUpdates(true);
    try {
      const [apps, updates] = await Promise.all([api.libraryList(), api.updatesCheck()]);
      setInstalled(apps);
      // Apps without an update are current: their latest tag is their own.
      const next: Record<string, string> = Object.fromEntries(apps.map((a) => [a.id, a.version]));
      for (const u of updates) next[u.id] = u.latest;
      setLatest(next);
    } finally {
      setCheckingUpdates(false);
    }
  }, []);

  const reloadCatalog = useCallback(async () => {
    setCatalogLoading(true);
    setCatalogError(null);
    try {
      setCatalog(await api.catalogRefresh());
    } catch (err) {
      setCatalogError(err);
    } finally {
      setCatalogLoading(false);
    }
  }, []);

  const reloadLibrary = useCallback(async () => {
    setInstalled(await api.libraryList());
  }, []);

  useEffect(() => {
    void api.settingsGet().then(setSettings);
    void api.appInfo().then(setInfo);
    void api.libraryList().then(setInstalled);
    void api.runningList().then((ids) => setRunning(Object.fromEntries(ids.map((id) => [id, true as const]))));
    api
      .catalogGet()
      .then(setCatalog, setCatalogError)
      .finally(() => setCatalogLoading(false));
  }, []);

  useEffect(() => {
    const handle = (event: JobEvent) => {
      const { id, job } = event;
      if (event.kind === "progress") {
        setJobs((prev) => ({ ...prev, [id]: { job, progress: event.progress } }));
        return;
      }
      endedJobs.current.add(job);
      setJobs((prev) => (prev[id]?.job === job ? without(prev, id) : prev));
      if (event.kind === "finished") {
        setInstalled((prev) =>
          [...prev.filter((a) => a.id !== id), event.app].sort((a, b) => a.name.localeCompare(b.name)),
        );
      } else if (event.error.code !== "cancelled") {
        setJobErrors((prev) => ({ ...prev, [id]: event.error }));
      }
    };
    const onExit = ({ id, code }: GameExited) => {
      setRunning((prev) => without(prev, id));
      setLastExit((prev) => ({ ...prev, [id]: code }));
    };
    const unsubscribers: (() => void)[] = [];
    let disposed = false;
    for (const subscribe of [api.onJobEvent(handle), api.onGameExited(onExit)]) {
      void subscribe.then((u) => {
        if (disposed) u();
        else unsubscribers.push(u);
      });
    }
    return () => {
      disposed = true;
      unsubscribers.forEach((u) => u());
    };
  }, []);

  const uninstallApp = useCallback(async (id: string, keepSaves: boolean) => {
    const outcome = await api.uninstall(id, keepSaves);
    setInstalled((prev) => prev.filter((a) => a.id !== id));
    setLatest((prev) => without(prev, id));
    setLastExit((prev) => without(prev, id));
    setJobErrors((prev) => without(prev, id));
    return outcome;
  }, []);

  const changeSource = useCallback(
    async (action: "add" | "remove", url: string) => {
      setSettings(action === "add" ? await api.sourceAdd(url) : await api.sourceRemove(url));
      await reloadCatalog();
    },
    [reloadCatalog],
  );

  const replaceInstalled = useCallback((app: InstalledSummary) => {
    setInstalled((prev) => prev.map((a) => (a.id === app.id ? app : a)));
  }, []);

  const play = useCallback(async (id: string) => {
    setLastExit((prev) => without(prev, id));
    await api.launch(id);
    setRunning((prev) => ({ ...prev, [id]: true }));
  }, []);

  // One automatic update check per app start, if the user wants it.
  const autoChecked = useRef(false);
  useEffect(() => {
    if (!settings?.check_updates || autoChecked.current) return;
    autoChecked.current = true;
    checkUpdates().catch((err: unknown) => console.warn("update check failed", err));
  }, [settings, checkUpdates]);

  useEffect(() => {
    if (!settings) return;
    applyTheme(settings.theme);
    const lang = resolveLanguage(settings.language);
    document.documentElement.lang = lang;
    void i18n.changeLanguage(lang);
  }, [settings]);

  const saveSettings = useCallback(async (next: Settings) => {
    const saved = await api.settingsSet(next);
    setSettings(saved);
    setInfo(await api.appInfo());
  }, []);

  const startJob = useCallback(async (id: string, start: () => Promise<number>) => {
    setJobErrors((prev) => without(prev, id));
    const job = await start();
    // Events can beat this reply; never resurrect a job that already ended.
    if (endedJobs.current.has(job)) return;
    setJobs((prev) => (prev[id] ? prev : { ...prev, [id]: { job, progress: null } }));
  }, []);

  const startInstall = useCallback(
    (id: string, options?: InstallOptions) => startJob(id, () => api.install(id, options)),
    [startJob],
  );
  const startUpdate = useCallback((id: string) => startJob(id, () => api.update(id)), [startJob]);
  const startRestore = useCallback((id: string) => startJob(id, () => api.restorePrevious(id)), [startJob]);

  const cancelJob = useCallback(
    async (id: string) => {
      const running = jobs[id];
      if (running) await api.jobCancel(running.job);
    },
    [jobs],
  );

  const value = useMemo<AppContextValue>(
    () => ({
      settings,
      info,
      catalog,
      catalogError,
      catalogLoading,
      installed,
      jobs,
      jobErrors,
      latest,
      checkingUpdates,
      running,
      lastExit,
      play,
      replaceInstalled,
      uninstallApp,
      changeSource,
      saveSettings,
      reloadCatalog,
      reloadLibrary,
      checkUpdates,
      startInstall,
      startUpdate,
      startRestore,
      cancelJob,
    }),
    [
      running,
      lastExit,
      play,
      replaceInstalled,
      uninstallApp,
      changeSource,
      settings,
      info,
      catalog,
      catalogError,
      catalogLoading,
      installed,
      jobs,
      jobErrors,
      latest,
      checkingUpdates,
      saveSettings,
      reloadCatalog,
      reloadLibrary,
      checkUpdates,
      startInstall,
      startUpdate,
      startRestore,
      cancelJob,
    ],
  );

  return <AppContext.Provider value={value}>{children}</AppContext.Provider>;
}

// eslint-disable-next-line react-refresh/only-export-components
export function useApp(): AppContextValue {
  const ctx = useContext(AppContext);
  if (!ctx) throw new Error("useApp outside AppProvider");
  return ctx;
}
