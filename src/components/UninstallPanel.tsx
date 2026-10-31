import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { InstalledSummary, UninstallOutcome } from "../api/types";
import { useApp } from "../state";
import { ErrorBox } from "./ErrorBox";

/**
 * "Uninstall…" opens an inline confirmation instead of a dialog, so the
 * choice about saves is visible and works with a controller.
 */
export function UninstallPanel({
  app,
  onDone,
}: {
  app: InstalledSummary;
  onDone(outcome: UninstallOutcome): void;
}) {
  const { t } = useTranslation();
  const { uninstallApp, running, jobs } = useApp();
  const [open, setOpen] = useState(false);
  const [deleteSaves, setDeleteSaves] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const blocked = running[app.id] === true || jobs[app.id] !== undefined;

  async function confirm() {
    setBusy(true);
    setError(null);
    try {
      onDone(await uninstallApp(app.id, !deleteSaves));
    } catch (err) {
      setError(err);
      setBusy(false);
    }
  }

  if (!open) {
    return (
      <button type="button" className="button ghost small" disabled={blocked} onClick={() => setOpen(true)}>
        {t("uninstall.open")}
      </button>
    );
  }

  return (
    <div className="uninstall-panel" role="group" aria-label={t("uninstall.title", { name: app.name })}>
      <p>
        <strong>{t("uninstall.title", { name: app.name })}</strong>
      </p>
      <p className="small">{deleteSaves ? t("uninstall.deleteHint") : t("uninstall.keepHint")}</p>
      <label className="check">
        <input type="checkbox" checked={deleteSaves} onChange={(e) => setDeleteSaves(e.target.checked)} />
        {t("uninstall.deleteSaves")}
      </label>
      <div className="row">
        <button type="button" className="button danger" disabled={busy || blocked} onClick={() => void confirm()}>
          {busy ? t("uninstall.working") : t("uninstall.confirm")}
        </button>
        <button type="button" className="button ghost" disabled={busy} onClick={() => setOpen(false)}>
          {t("uninstall.cancel")}
        </button>
      </div>
      {error !== null && <ErrorBox error={error} />}
    </div>
  );
}
