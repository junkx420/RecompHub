import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../api";
import type { InstalledSummary, SteamStatus } from "../api/types";
import { useApp } from "../state";
import { ErrorBox } from "./ErrorBox";

/** Start menu / desktop shortcuts and the Steam library, for one installed app. */
export function IntegrationCard({ app }: { app: InstalledSummary }) {
  const { t } = useTranslation();
  const { settings, replaceInstalled } = useApp();
  const [desktop, setDesktop] = useState(settings?.desktop_shortcut ?? false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);

  const [attempt, setAttempt] = useState(0);
  const [steam, setSteam] = useState<{ key: number; status: SteamStatus } | null>(null);
  // Accounts the user ticked; null means "use the default selection".
  const [picked, setPicked] = useState<number[] | null>(null);

  useEffect(() => {
    api.steamStatus().then(
      (status) => setSteam({ key: attempt, status }),
      (err: unknown) => setError(err),
    );
  }, [attempt]);

  const status = steam?.key === attempt ? steam.status : null;
  const defaultPick =
    app.steam_users.length > 0
      ? app.steam_users
      : (status?.users.filter((u) => u.most_recent).map((u) => u.account_id) ?? []);
  const selected = picked ?? defaultPick;

  async function run(action: () => Promise<InstalledSummary>) {
    setBusy(true);
    setError(null);
    try {
      replaceInstalled(await action());
    } catch (err) {
      setError(err);
      setAttempt((n) => n + 1); // Steam may have been opened or closed meanwhile
    } finally {
      setBusy(false);
    }
  }

  function toggle(accountId: number) {
    setPicked(selected.includes(accountId) ? selected.filter((x) => x !== accountId) : [...selected, accountId]);
  }

  const addedNames = (status?.users ?? [])
    .filter((u) => app.steam_users.includes(u.account_id))
    .map((u) => u.persona_name || u.account_name);

  return (
    <div className="card integration-card">
      <h2>{t("integration.title")}</h2>

      <h3>{t("integration.shortcuts")}</h3>
      {app.shortcuts.length > 0 ? (
        <>
          <p className="muted small">{t("integration.shortcutCount", { count: app.shortcuts.length })}</p>
          <div className="row">
            <button type="button" className="button" disabled={busy} onClick={() => void run(() => api.shortcutRemove(app.id))}>
              {t("integration.removeShortcuts")}
            </button>
          </div>
        </>
      ) : (
        <p className="muted small">{t("integration.noShortcuts")}</p>
      )}
      <div className="row">
        <label className="check">
          <input type="checkbox" checked={desktop} onChange={(e) => setDesktop(e.target.checked)} />
          {t("integration.alsoDesktop")}
        </label>
        <button
          type="button"
          className="button"
          disabled={busy}
          onClick={() => void run(() => api.shortcutCreate(app.id, desktop))}
        >
          {app.shortcuts.length > 0 ? t("integration.recreateShortcuts") : t("integration.createShortcuts")}
        </button>
      </div>

      <h3>{t("integration.steam")}</h3>
      {!status ? (
        <p className="muted small">{t("common.loading")}</p>
      ) : !status.found ? (
        <p className="muted small">{t("integration.steamNotFound")}</p>
      ) : (
        <>
          {addedNames.length > 0 && (
            <p className="small">{t("integration.steamAdded", { names: addedNames.join(", ") })}</p>
          )}
          {status.users.length > 1 && (
            <fieldset className="steam-users">
              <legend className="small muted">{t("integration.steamAccounts")}</legend>
              {status.users.map((u) => (
                <label key={u.account_id} className="check">
                  <input type="checkbox" checked={selected.includes(u.account_id)} onChange={() => toggle(u.account_id)} />
                  {u.persona_name || u.account_name}
                  {u.most_recent && <span className="muted small"> · {t("integration.lastLogin")}</span>}
                </label>
              ))}
            </fieldset>
          )}
          {status.users.length === 0 && <p className="muted small">{t("integration.steamNoUsers")}</p>}
          {status.running && <p className="badge badge-warning">{t("integration.steamRunning")}</p>}
          <div className="row">
            <button
              type="button"
              className="button"
              disabled={busy || status.running || selected.length === 0}
              onClick={() => void run(() => api.steamAdd(app.id, selected))}
            >
              {app.steam_users.length > 0 ? t("integration.steamRefresh") : t("integration.steamAdd")}
            </button>
            {app.steam_users.length > 0 && (
              <button
                type="button"
                className="button"
                disabled={busy || status.running}
                onClick={() => void run(() => api.steamRemove(app.id))}
              >
                {t("integration.steamRemove")}
              </button>
            )}
            {status.running && (
              <button type="button" className="button ghost" onClick={() => setAttempt((n) => n + 1)}>
                {t("integration.checkAgain")}
              </button>
            )}
          </div>
        </>
      )}
      {error !== null && <ErrorBox error={error} />}
    </div>
  );
}
