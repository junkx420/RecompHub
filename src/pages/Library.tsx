import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { useTranslation } from "react-i18next";
import { inTauri } from "../api";
import { EntryIcon } from "../components/EntryIcon";
import { useApp } from "../state";

export function Library({ onOpen }: { onOpen: (id: string) => void }) {
  const { t, i18n } = useTranslation();
  const { installed, catalog, jobs } = useApp();

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

  return (
    <section className="page">
      <ul className="library-list">
        {installed.map((app) => {
          const entry = catalog?.entries.find((e) => e.id === app.id);
          const date = new Date(app.installed_at).toLocaleDateString(i18n.language, { dateStyle: "medium" });
          return (
            <li key={app.id} className="card library-item">
              {entry && <EntryIcon entry={entry} size={56} />}
              <div className="library-main">
                <button type="button" className="link library-name" onClick={() => onOpen(app.id)}>
                  {app.name}
                </button>
                <span className="muted small">
                  {t("library.version", { version: app.version })} · {t("library.installedOn", { date })}
                  {jobs[app.id] && ` · ${t("library.busy")}`}
                </span>
                <code className="small">{app.dir}</code>
              </div>
              {inTauri && (
                <button type="button" className="button" onClick={() => void revealItemInDir(app.dir)}>
                  {t("install.openFolder")}
                </button>
              )}
            </li>
          );
        })}
      </ul>
    </section>
  );
}
