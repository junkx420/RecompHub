import { open } from "@tauri-apps/plugin-dialog";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { inTauri } from "../api";
import type { Language, Settings as SettingsModel, Theme } from "../api/types";
import { CatalogStatus } from "../components/CatalogStatus";
import { formatError } from "../errors";
import { useApp } from "../state";

const LANGUAGES: Language[] = ["system", "de", "en"];
const THEMES: Theme[] = ["system", "light", "dark"];
const LANGUAGE_NAMES: Record<Exclude<Language, "system">, string> = { de: "Deutsch", en: "English" };

export function Settings() {
  const { t } = useTranslation();
  const { settings, info, catalog, catalogLoading, saveSettings, reloadCatalog } = useApp();
  // null = no local edit; show the saved value.
  const [draftUrl, setDraftUrl] = useState<string | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [savedUrl, setSavedUrl] = useState(false);

  if (!settings) return <p className="muted">{t("common.loading")}</p>;
  const catalogUrl = draftUrl ?? settings.catalog_url;

  async function update(patch: Partial<SettingsModel>) {
    if (!settings) return;
    setError(null);
    try {
      await saveSettings({ ...settings, ...patch });
    } catch (err) {
      setError(err);
      throw err;
    }
  }

  async function pickLibrary() {
    if (!inTauri) return;
    const dir = await open({ directory: true, multiple: false, title: t("settings.pickLibrary") });
    if (typeof dir === "string") await update({ library_dir: dir }).catch(() => undefined);
  }

  async function saveUrl() {
    setSavedUrl(false);
    try {
      await update({ catalog_url: catalogUrl.trim() });
      setDraftUrl(null);
      setSavedUrl(true);
      await reloadCatalog();
    } catch {
      // error is shown via state
    }
  }

  return (
    <section className="page settings">
      {error !== null && (
        <p className="error-inline" role="alert">
          {formatError(error, t)}
        </p>
      )}

      <div className="card">
        <h2>{t("settings.appearance")}</h2>
        <label className="field">
          <span>{t("settings.language")}</span>
          <select
            value={settings.language}
            onChange={(e) => void update({ language: e.target.value as Language }).catch(() => undefined)}
          >
            {LANGUAGES.map((l) => (
              <option key={l} value={l}>
                {l === "system" ? t("settings.system") : LANGUAGE_NAMES[l]}
              </option>
            ))}
          </select>
        </label>
        <label className="field">
          <span>{t("settings.theme")}</span>
          <select
            value={settings.theme}
            onChange={(e) => void update({ theme: e.target.value as Theme }).catch(() => undefined)}
          >
            {THEMES.map((th) => (
              <option key={th} value={th}>
                {t(`settings.${th}`)}
              </option>
            ))}
          </select>
        </label>
      </div>

      <div className="card">
        <h2>{t("settings.library")}</h2>
        <p className="path">{info?.library_dir}</p>
        <p className="muted small">{t("settings.libraryHint")}</p>
        <div className="row">
          <button type="button" className="button" onClick={() => void pickLibrary()} disabled={!inTauri}>
            {t("common.change")}
          </button>
          {settings.library_dir !== null && (
            <button
              type="button"
              className="button ghost"
              onClick={() => void update({ library_dir: null }).catch(() => undefined)}
            >
              {t("common.reset")}
            </button>
          )}
        </div>
      </div>

      <div className="card">
        <h2>{t("settings.behaviour")}</h2>
        <label className="check">
          <input
            type="checkbox"
            checked={settings.check_updates}
            onChange={(e) => void update({ check_updates: e.target.checked }).catch(() => undefined)}
          />
          {t("settings.checkUpdates")}
        </label>
        <label className="check">
          <input
            type="checkbox"
            checked={settings.desktop_shortcut}
            onChange={(e) => void update({ desktop_shortcut: e.target.checked }).catch(() => undefined)}
          />
          {t("settings.desktopShortcut")}
        </label>
        <p className="muted small">{t("settings.gamepadHint")}</p>
      </div>

      <div className="card">
        <h2>{t("settings.catalog")}</h2>
        <label className="field stacked">
          <span>{t("settings.catalogUrl")}</span>
          <input
            type="url"
            value={catalogUrl}
            spellCheck={false}
            onChange={(e) => {
              setDraftUrl(e.target.value);
              setSavedUrl(false);
            }}
          />
        </label>
        <div className="row">
          <button
            type="button"
            className="button primary"
            onClick={() => void saveUrl()}
            disabled={catalogUrl.trim() === settings.catalog_url}
          >
            {t("common.save")}
          </button>
          <button type="button" className="button" onClick={() => void reloadCatalog()} disabled={catalogLoading}>
            {t("settings.reloadCatalog")}
          </button>
          {savedUrl && <span className="muted">{t("common.saved")}</span>}
        </div>
        {catalog && <CatalogStatus catalog={catalog} />}
      </div>

      <div className="card">
        <h2>{t("settings.about")}</h2>
        <p>{info && t("settings.version", { version: info.version })}</p>
        <dl className="facts">
          <dt>{t("settings.dataDir")}</dt>
          <dd className="path">{info?.data_dir}</dd>
        </dl>
      </div>
    </section>
  );
}
