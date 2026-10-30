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
  InstallOptions,
  InstalledSummary,
  JobEvent,
  LoadedCatalog,
  Progress,
  Settings,
  Theme,
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
  saveSettings(next: Settings): Promise<void>;
  reloadCatalog(): Promise<void>;
  reloadLibrary(): Promise<void>;
  startInstall(id: string, options?: InstallOptions): Promise<void>;
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
    let unsubscribe: (() => void) | undefined;
    let disposed = false;
    void api.onJobEvent(handle).then((u) => {
      if (disposed) u();
      else unsubscribe = u;
    });
    return () => {
      disposed = true;
      unsubscribe?.();
    };
  }, []);

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

  const startInstall = useCallback(async (id: string, options?: InstallOptions) => {
    setJobErrors((prev) => without(prev, id));
    const job = await api.install(id, options);
    // Events can beat this reply; never resurrect a job that already ended.
    if (endedJobs.current.has(job)) return;
    setJobs((prev) => (prev[id] ? prev : { ...prev, [id]: { job, progress: null } }));
  }, []);

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
      saveSettings,
      reloadCatalog,
      reloadLibrary,
      startInstall,
      cancelJob,
    }),
    [
      settings,
      info,
      catalog,
      catalogError,
      catalogLoading,
      installed,
      jobs,
      jobErrors,
      saveSettings,
      reloadCatalog,
      reloadLibrary,
      startInstall,
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
