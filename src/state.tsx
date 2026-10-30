import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { api } from "./api";
import type { AppInfo, LoadedCatalog, Settings, Theme } from "./api/types";
import i18n, { resolveLanguage } from "./i18n";

interface AppContextValue {
  settings: Settings | null;
  info: AppInfo | null;
  catalog: LoadedCatalog | null;
  catalogError: unknown;
  catalogLoading: boolean;
  saveSettings(next: Settings): Promise<void>;
  reloadCatalog(): Promise<void>;
}

const AppContext = createContext<AppContextValue | null>(null);

function applyTheme(theme: Theme) {
  const root = document.documentElement;
  if (theme === "system") root.removeAttribute("data-theme");
  else root.setAttribute("data-theme", theme);
}

export function AppProvider({ children }: { children: ReactNode }) {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [catalog, setCatalog] = useState<LoadedCatalog | null>(null);
  const [catalogError, setCatalogError] = useState<unknown>(null);
  const [catalogLoading, setCatalogLoading] = useState(true);

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

  useEffect(() => {
    void api.settingsGet().then(setSettings);
    void api.appInfo().then(setInfo);
    api
      .catalogGet()
      .then(setCatalog, setCatalogError)
      .finally(() => setCatalogLoading(false));
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

  const value = useMemo<AppContextValue>(
    () => ({
      settings,
      info,
      catalog,
      catalogError,
      catalogLoading,
      saveSettings,
      reloadCatalog,
    }),
    [settings, info, catalog, catalogError, catalogLoading, saveSettings, reloadCatalog],
  );

  return <AppContext.Provider value={value}>{children}</AppContext.Provider>;
}

// eslint-disable-next-line react-refresh/only-export-components
export function useApp(): AppContextValue {
  const ctx = useContext(AppContext);
  if (!ctx) throw new Error("useApp outside AppProvider");
  return ctx;
}
