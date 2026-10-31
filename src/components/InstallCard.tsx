import { ask } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, inTauri } from "../api";
import type { CatalogEntry, Progress, ReleaseInfo } from "../api/types";
import { formatBytes } from "../format";
import { useApp } from "../state";
import { ErrorBox } from "./ErrorBox";
import { PlayButton, RunInfo } from "./Play";

async function confirmDialog(message: string, title: string): Promise<boolean> {
  if (inTauri) return ask(message, { title, kind: "warning" });
  return window.confirm(message);
}

export function ProgressView({ progress }: { progress: Progress | null }) {
  const { t, i18n } = useTranslation();
  if (!progress) return <p className="muted">{t("install.phase.resolving")}</p>;
  const counted =
    progress.phase === "downloading" || progress.phase === "extracting" || progress.phase === "applying";
  const ratio = counted && progress.total > 0 ? Math.min(1, progress.done / progress.total) : null;
  let detail = "";
  if (progress.phase === "downloading") {
    detail = t("install.bytes", {
      done: formatBytes(progress.done, i18n.language),
      total: formatBytes(progress.total, i18n.language),
    });
  } else if (progress.phase === "extracting" || progress.phase === "applying") {
    detail = t("install.files", { done: progress.done, total: progress.total });
  }
  return (
    <div className="progress-block">
      <div className="row spread">
        <span>{t(`install.phase.${progress.phase}`)}</span>
        <span className="muted small">{detail}</span>
      </div>
      <div
        className={`progress ${ratio === null ? "indeterminate" : ""}`}
        role="progressbar"
        aria-label={t(`install.phase.${progress.phase}`)}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={ratio === null ? undefined : Math.round(ratio * 100)}
      >
        <div className="progress-fill" style={ratio === null ? undefined : { width: `${ratio * 100}%` }} />
      </div>
    </div>
  );
}

export function InstallCard({ entry }: { entry: CatalogEntry }) {
  const { t, i18n } = useTranslation();
  const { installed, jobs, jobErrors, startInstall, startUpdate, startRestore, cancelJob } = useApp();
  const [attempt, setAttempt] = useState(0);
  const [loaded, setLoaded] = useState<{ key: string; release: ReleaseInfo | null; error: unknown } | null>(null);
  const [actionError, setActionError] = useState<unknown>(null);
  const [starting, setStarting] = useState(false);

  const app = installed.find((a) => a.id === entry.id);
  const job = jobs[entry.id];
  const jobError = jobErrors[entry.id];

  // Results are tagged with what they were loaded for, so a stale answer
  // (other entry, earlier attempt) is simply not shown.
  const releaseKey = `${entry.id}#${attempt}`;
  const current = loaded?.key === releaseKey ? loaded : null;
  const release = current?.release ?? null;
  const releaseError = current?.error ?? null;

  useEffect(() => {
    api.releaseInfo(entry.id).then(
      (info) => setLoaded({ key: releaseKey, release: info, error: null }),
      (err: unknown) => setLoaded({ key: releaseKey, release: null, error: err }),
    );
  }, [entry.id, releaseKey]);

  async function run(action: () => Promise<void>) {
    setActionError(null);
    try {
      await action();
    } catch (err) {
      setActionError(err);
    }
  }

  async function onRestore(version: string) {
    const ok = await confirmDialog(t("update.restoreConfirm", { version }), t("update.restoreTitle"));
    if (ok) await run(() => startRestore(entry.id));
  }

  async function onInstall() {
    setActionError(null);
    setStarting(true);
    try {
      if (entry.source.kind === "custom") {
        const ok = await confirmDialog(t("sources.installConfirm", { url: entry.source.url }), t("badge.unverified"));
        if (!ok) return;
      }
      const preflight = await api.installPreflight(entry.id);
      let replaceForeign = false;
      if (preflight.state === "foreign_content") {
        replaceForeign = await confirmDialog(
          t("install.foreignContent", { path: preflight.target }),
          t("install.foreignTitle"),
        );
        if (!replaceForeign) return;
      }
      await startInstall(entry.id, { replace_foreign: replaceForeign });
    } catch (err) {
      setActionError(err);
    } finally {
      setStarting(false);
    }
  }

  const published = release?.published_at
    ? new Date(release.published_at).toLocaleDateString(i18n.language, { dateStyle: "medium" })
    : null;

  return (
    <div className="card install-card">
      <h2>{t("install.title")}</h2>
      {release && (
        <p className="muted">
          {t("install.latest", { version: release.tag })}
          {published && ` · ${published}`}
          {` · ${formatBytes(release.asset_size, i18n.language)}`}
          {!release.verified && (
            <span className="badge badge-warning" title={t("install.unverifiedHint")}>
              {t("install.unverified")}
            </span>
          )}
        </p>
      )}
      {!current && <p className="muted">{t("common.loading")}</p>}
      {entry.source.kind === "custom" && !app && (
        <p className="badge badge-warning">{t("sources.notice", { url: entry.source.url })}</p>
      )}
      {releaseError !== null && <ErrorBox error={releaseError} onRetry={() => setAttempt((n) => n + 1)} />}

      {job ? (
        <>
          <ProgressView progress={job.progress} />
          <button type="button" className="button" onClick={() => void cancelJob(entry.id)}>
            {t("install.cancel")}
          </button>
        </>
      ) : app ? (
        <>
          <p>{t("install.installed", { version: app.version })}</p>
          <p className="muted small">
            <code>{app.dir}</code>
          </p>
          <div className="row">
            <PlayButton app={app} onError={setActionError} />
            {release && release.tag !== app.version && (
              <button type="button" className="button" onClick={() => void run(() => startUpdate(entry.id))}>
                {t("update.updateTo", { version: release.tag })}
              </button>
            )}
            {app.previous_version && (
              <button type="button" className="button" onClick={() => void onRestore(app.previous_version ?? "")}>
                {t("update.restore", { version: app.previous_version })}
              </button>
            )}
            {inTauri && (
              <button type="button" className="button" onClick={() => void revealItemInDir(app.dir)}>
                {t("install.openFolder")}
              </button>
            )}
          </div>
          {release && release.tag !== app.version && <p className="muted small">{t("update.keepsData")}</p>}
          <RunInfo app={app} />
        </>
      ) : (
        <button
          type="button"
          className="button primary"
          disabled={starting || release === null}
          onClick={() => void onInstall()}
        >
          {t("install.install")}
        </button>
      )}
      {actionError !== null && <ErrorBox error={actionError} />}
      {jobError && !job && <ErrorBox error={jobError} onRetry={() => void onInstall()} />}
    </div>
  );
}
