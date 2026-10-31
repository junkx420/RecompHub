import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { inTauri } from "../api";
import { formatBytes } from "../format";
import { useApp } from "../state";
import { ErrorBox } from "./ErrorBox";

type Phase =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "current" }
  | { kind: "available"; update: Update }
  | { kind: "downloading"; done: number; total: number | null };

/** Check for a new RecompHub release, install it and restart. */
export function AppUpdate() {
  const { t, i18n } = useTranslation();
  const { info } = useApp();
  const [phase, setPhase] = useState<Phase>({ kind: "idle" });
  const [error, setError] = useState<unknown>(null);

  if (!inTauri || !info?.self_update) {
    return <p className="muted small">{t("appUpdate.unavailable")}</p>;
  }

  async function onCheck() {
    setError(null);
    setPhase({ kind: "checking" });
    try {
      const update = await check();
      setPhase(update ? { kind: "available", update } : { kind: "current" });
    } catch (err) {
      setError(err);
      setPhase({ kind: "idle" });
    }
  }

  async function onInstall(update: Update) {
    setError(null);
    let done = 0;
    let total: number | null = null;
    setPhase({ kind: "downloading", done, total });
    try {
      await update.downloadAndInstall((event) => {
        if (event.event === "Started") total = event.data.contentLength ?? null;
        if (event.event === "Progress") done += event.data.chunkLength;
        setPhase({ kind: "downloading", done, total });
      });
      await relaunch();
    } catch (err) {
      setError(err);
      setPhase({ kind: "available", update });
    }
  }

  return (
    <div className="app-update">
      {phase.kind === "available" ? (
        <>
          <p>{t("appUpdate.available", { version: phase.update.version })}</p>
          {phase.update.body && <pre className="log-tail">{phase.update.body}</pre>}
          <button type="button" className="button primary" onClick={() => void onInstall(phase.update)}>
            {t("appUpdate.install")}
          </button>
        </>
      ) : phase.kind === "downloading" ? (
        <p>
          {t("appUpdate.downloading")}{" "}
          <span className="muted small">
            {formatBytes(phase.done, i18n.language)}
            {phase.total !== null && ` / ${formatBytes(phase.total, i18n.language)}`}
          </span>
        </p>
      ) : (
        <div className="row">
          <button type="button" className="button" disabled={phase.kind === "checking"} onClick={() => void onCheck()}>
            {phase.kind === "checking" ? t("update.checking") : t("appUpdate.check")}
          </button>
          {phase.kind === "current" && <span className="muted small">{t("appUpdate.current")}</span>}
        </div>
      )}
      {error !== null && <ErrorBox error={error} />}
    </div>
  );
}
