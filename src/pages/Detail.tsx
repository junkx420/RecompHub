import { openUrl } from "@tauri-apps/plugin-opener";
import { useTranslation } from "react-i18next";
import { inTauri } from "../api";
import type { CatalogEntry, KnownDump } from "../api/types";
import { EntryIcon } from "../components/EntryIcon";
import { SourceBadge } from "../components/SourceBadge";
import { localized } from "../i18n";
import { useApp } from "../state";

function openExternal(url: string) {
  if (inTauri) void openUrl(url);
  else window.open(url, "_blank", "noopener,noreferrer");
}

function dumpLabel(d: KnownDump) {
  return `${d.region} ${d.version}`;
}

function GameDataInfo({ entry }: { entry: CatalogEntry }) {
  const { t, i18n } = useTranslation();
  const gd = entry.game_data;
  return (
    <div className="card">
      <h2>{t("detail.gameData")}</h2>
      {gd.mode === "none" && <p>{t("gameData.none")}</p>}
      {gd.mode === "port_handles_it" && (
        <>
          <p>{t("gameData.portHandlesIt")}</p>
          <p className="hint">{localized(gd.hint, i18n.language)}</p>
        </>
      )}
      {gd.mode === "copy_file" && (
        <>
          <p>{t("gameData.copyFile")}</p>
          {gd.hint && <p className="hint">{localized(gd.hint, i18n.language)}</p>}
          <p className="muted">{t("gameData.accepted", { list: gd.accepted_hashes.map(dumpLabel).join(", ") })}</p>
        </>
      )}
      {gd.mode !== "none" && <p className="muted small">{t("gameData.legal")}</p>}
    </div>
  );
}

export function Detail({ id, onBack }: { id: string; onBack: () => void }) {
  const { t, i18n } = useTranslation();
  const { catalog } = useApp();
  const entry = catalog?.entries.find((e) => e.id === id);

  return (
    <section className="page detail">
      <button type="button" className="button ghost back" onClick={onBack}>
        ← {t("common.back")}
      </button>
      {!entry ? (
        <p>{t("detail.notFound")}</p>
      ) : (
        <>
          <header className="detail-header">
            <EntryIcon entry={entry} size={112} />
            <div>
              <h1>{entry.name}</h1>
              <div className="tile-meta">
                <span className="badge">{entry.system.toUpperCase()}</span>
                <span className="muted">{t(`category.${entry.category}`)}</span>
                <SourceBadge source={entry.source} />
              </div>
            </div>
          </header>

          <div className="detail-grid">
            <div className="card">
              <h2>{t("detail.description")}</h2>
              <p>{localized(entry.description, i18n.language)}</p>
            </div>
            <GameDataInfo entry={entry} />
            <div className="card">
              <h2>{t("detail.credits")}</h2>
              <ul className="plain">
                {entry.credits.map((c) => (
                  <li key={c}>{c}</li>
                ))}
              </ul>
              <dl className="facts">
                <dt>{t("detail.license")}</dt>
                <dd>{entry.license}</dd>
                <dt>{t("detail.repo")}</dt>
                <dd>
                  <button type="button" className="link" onClick={() => openExternal(entry.homepage)}>
                    {entry.repo}
                  </button>
                </dd>
              </dl>
            </div>
            {(entry.preserve.length > 0 || entry.user_data_paths.length > 0) && (
              <div className="card">
                {entry.preserve.length > 0 && (
                  <>
                    <h2>{t("detail.preserve")}</h2>
                    <ul className="code-list">
                      {entry.preserve.map((p) => (
                        <li key={p}>
                          <code>{p}</code>
                        </li>
                      ))}
                    </ul>
                  </>
                )}
                {entry.user_data_paths.length > 0 && (
                  <>
                    <h2>{t("detail.userData")}</h2>
                    <ul className="code-list">
                      {entry.user_data_paths.map((p) => (
                        <li key={p}>
                          <code>{p}</code>
                        </li>
                      ))}
                    </ul>
                  </>
                )}
              </div>
            )}
          </div>
        </>
      )}
    </section>
  );
}
