import { invoke } from "@tauri-apps/api/core";
import type { AppInfo, LoadedCatalog, Settings } from "./types";

export interface Backend {
  appInfo(): Promise<AppInfo>;
  settingsGet(): Promise<Settings>;
  settingsSet(settings: Settings): Promise<Settings>;
  catalogGet(): Promise<LoadedCatalog>;
  catalogRefresh(): Promise<LoadedCatalog>;
}

const tauriBackend: Backend = {
  appInfo: () => invoke("app_info"),
  settingsGet: () => invoke("settings_get"),
  settingsSet: (settings) => invoke("settings_set", { settings }),
  catalogGet: () => invoke("catalog_get"),
  catalogRefresh: () => invoke("catalog_refresh"),
};

export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/**
 * Outside Tauri (plain `npm run dev` in a browser) a mock backend serves the
 * embedded catalog, so the UI can be developed and screenshotted without Rust.
 */
function lazyMock(): Backend {
  const load = () => import("./mock").then((m) => m.mockBackend);
  return {
    appInfo: () => load().then((b) => b.appInfo()),
    settingsGet: () => load().then((b) => b.settingsGet()),
    settingsSet: (s) => load().then((b) => b.settingsSet(s)),
    catalogGet: () => load().then((b) => b.catalogGet()),
    catalogRefresh: () => load().then((b) => b.catalogRefresh()),
  };
}

export const api: Backend = inTauri || !import.meta.env.DEV ? tauriBackend : lazyMock();
