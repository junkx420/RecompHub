import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { inTauri } from "../api";
import { EntryIcon } from "../components/EntryIcon";
import { ErrorBox } from "../components/ErrorBox";
import { ProgressView } from "../components/InstallCard";
import { PlayButton, RunInfo } from "../components/Play";
import { useApp } from "../state";
import { outdated } from "../updates";

export function Library({ onOpen }: { onOpen: (id: string) => void }) {
  const { t, i18n } = useTranslation();
  const {
    installed,
    catalog,
    jobs,
    jobErrors,
    latest,
    lastExit,
    checkingUpdates,
    checkUpdates,
    startUpdate,
    cancelJob,
  } = useApp();
  const [error, setError] = useState<unknown>(null);

  async function run(action: () => Promise<void>) {
    setError(null);
    try {
      await action();
    } catch (err) {
      setError(err);
    }
  }

  if (installed.length === 0) {
    return (
      <section className="page">
        <div className="empty-state">
          <p>{t("library.empty")}</p>
          <p className="muted">{t("library.emptyHint")}</p>
        </div>
      </section>
    );
  }

  const updatable = outdated(installed, latest);
  const idle = updatable.filter((a) => !jobs[a.id]);

  return (
    <section className="page">
      <div className="toolbar library-toolbar">
        <button type="button" className="button" disabled={checkingUpdates} onClick={() => void run(checkUpdates)}>
          {checkingUpdates ? t("update.checking") : t("update.check")}
        </button>
        {idle.length > 1 && (
          <button
            type="button"
            className="button primary"
            onClick={() => void run(async () => void (await Promise.all(idle.map((a) => startUpdate(a.id)))))}
          >
            {t("update.updateAll", { count: idle.length })}
          </button>
        )}
        <span className="muted small">
          {updatable.length > 0 ? t("update.available", { count: updatable.length }) : null}
        </span>
      </div>
      {error !== null && <ErrorBox error={error} />}
      <ul className="library-list">
        {installed.map((app) => {
          const entry = catalog?.entries.find((e) => e.id === app.id);
          const date = new Date(app.installed_at).toLocaleDateString(i18n.language, { dateStyle: "medium" });
          const job = jobs[app.id];
          const newer = latest[app.id] !== undefined && latest[app.id] !== app.version ? latest[app.id] : null;
          return (
            <li key={app.id} className="card library-item">
              {entry && <EntryIcon entry={entry} size={56} />}
              <div className="library-main">
                <button type="button" className="link library-name" onClick={() => onOpen(app.id)}>
                  {app.name}
                </button>
                <span className="muted small">
                  {t("library.version", { version: app.version })} · {t("library.installedOn", { date })}
                  {newer && (
                    <span className="badge badge-accent">{t("update.badge", { version: newer })}</span>
                  )}
                </span>
                <code className="small">{app.dir}</code>
                {job && <ProgressView progress={job.progress} />}
                {jobErrors[app.id] && !job && <ErrorBox error={jobErrors[app.id]} />}
                {lastExit[app.id] !== undefined && lastExit[app.id] !== 0 && <RunInfo app={app} />}
              </div>
              <div className="library-actions">
                {job ? (
                  <button type="button" className="button" onClick={() => void cancelJob(app.id)}>
                    {t("install.cancel")}
                  </button>
                ) : (
                  <>
                    <PlayButton app={app} onError={setError} />
                    {newer && (
                      <button type="button" className="button" onClick={() => void run(() => startUpdate(app.id))}>
                        {t("update.update")}
                      </button>
                    )}
                  </>
                )}
                {inTauri && (
                  <button type="button" className="button" onClick={() => void revealItemInDir(app.dir)}>
                    {t("install.openFolder")}
                  </button>
                )}
              </div>
            </li>
          );
        })}
      </ul>
    </section>
  );
}
