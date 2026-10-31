import { useState } from "react";
import { useTranslation } from "react-i18next";
import { api, inTauri } from "../api";
import type { InstalledSummary } from "../api/types";
import { useApp } from "../state";
import { ErrorBox } from "./ErrorBox";

/** Start button; disabled while the game runs or a job works on it. */
export function PlayButton({ app, onError }: { app: InstalledSummary; onError(err: unknown): void }) {
  const { t } = useTranslation();
  const { running, jobs, play } = useApp();
  const [starting, setStarting] = useState(false);
  const isRunning = running[app.id] === true;

  async function start() {
    setStarting(true);
    try {
      await play(app.id);
    } catch (err) {
      onError(err);
    } finally {
      setStarting(false);
    }
  }

  return (
    <button
      type="button"
      className="button primary"
      disabled={isRunning || starting || jobs[app.id] !== undefined}
      onClick={() => void start()}
    >
      {isRunning ? t("play.running") : t("play.play")}
    </button>
  );
}

/** How the last run ended, and the game's log on demand. */
export function RunInfo({ app }: { app: InstalledSummary }) {
  const { t } = useTranslation();
  const { lastExit } = useApp();
  const [log, setLog] = useState<string | null>(null);
  const [error, setError] = useState<unknown>(null);
  const exit = lastExit[app.id];
  const crashed = exit !== undefined && exit !== 0;

  async function toggleLog() {
    setError(null);
    if (log !== null) {
      setLog(null);
      return;
    }
    try {
      setLog(await api.logTail(app.id));
    } catch (err) {
      setError(err);
    }
  }

  async function openLog() {
    try {
      await api.openLog(app.id);
    } catch (err) {
      setError(err);
    }
  }

  return (
    <div className="run-info">
      {crashed && <p className="error-inline small">{t("play.crashed", { code: exit ?? "?" })}</p>}
      <div className="row">
        <button type="button" className="button ghost small" onClick={() => void toggleLog()}>
          {log === null ? t("play.showLog") : t("play.hideLog")}
        </button>
        {inTauri && log !== null && (
          <button type="button" className="button ghost small" onClick={() => void openLog()}>
            {t("play.openLog")}
          </button>
        )}
      </div>
      {log !== null && <pre className="log-tail">{log || t("play.emptyLog")}</pre>}
      {error !== null && <ErrorBox error={error} />}
    </div>
  );
}
