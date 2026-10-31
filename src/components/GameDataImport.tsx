import { open } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, inTauri } from "../api";
import type { GameDataStatus, InstalledSummary } from "../api/types";
import { ErrorBox } from "./ErrorBox";

async function pickRom(title: string): Promise<string | null> {
  if (inTauri) {
    const picked = await open({
      title,
      multiple: false,
      directory: false,
      filters: [{ name: "N64 ROM", extensions: ["z64", "n64", "v64"] }],
    });
    return typeof picked === "string" ? picked : null;
  }
  // Browser preview with the mock backend: type any path.
  return window.prompt(title, "C:\\Games\\ROMs\\game.z64");
}

/** Status of the user's dump for a `copy_file` entry, and the import button. */
export function GameDataImport({ app }: { app: InstalledSummary }) {
  const { t } = useTranslation();
  const [attempt, setAttempt] = useState(0);
  const [loaded, setLoaded] = useState<{ key: string; status: GameDataStatus | null; error: unknown } | null>(null);
  const [importing, setImporting] = useState(false);
  const [importError, setImportError] = useState<unknown>(null);

  // Tagged with what it was loaded for, so stale answers are ignored.
  const key = `${app.id}#${app.version}#${attempt}`;
  const current = loaded?.key === key ? loaded : null;
  const status = current?.status ?? null;

  useEffect(() => {
    api.gamedataStatus(app.id).then(
      (s) => setLoaded({ key, status: s, error: null }),
      (err: unknown) => setLoaded({ key, status: null, error: err }),
    );
  }, [app.id, key]);

  async function onImport() {
    setImportError(null);
    const path = await pickRom(t("gameData.pickTitle"));
    if (!path) return;
    setImporting(true);
    try {
      await api.gamedataImport(app.id, path);
      setAttempt((n) => n + 1);
    } catch (err) {
      setImportError(err);
    } finally {
      setImporting(false);
    }
  }

  if (current?.error) return <ErrorBox error={current.error} onRetry={() => setAttempt((n) => n + 1)} />;
  if (!status || (status.state !== "missing" && status.state !== "present")) return null;

  return (
    <div className="gamedata-status">
      {status.state === "missing" ? (
        <p className="badge badge-warning">{t("gameData.missing")}</p>
      ) : status.dump ? (
        <p className="badge badge-ok">{t("gameData.present", { dump: `${status.dump.region} ${status.dump.version}` })}</p>
      ) : (
        <p className="badge badge-warning">{t("gameData.presentUnknown")}</p>
      )}
      <button type="button" className="button" disabled={importing} onClick={() => void onImport()}>
        {importing ? t("gameData.checking") : status.state === "missing" ? t("gameData.choose") : t("gameData.replace")}
      </button>
      {importError !== null && <ErrorBox error={importError} />}
    </div>
  );
}
