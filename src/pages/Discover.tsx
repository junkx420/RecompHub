import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import type { Category, Maturity } from "../api/types";
import { CatalogStatus } from "../components/CatalogStatus";
import { EntryIcon } from "../components/EntryIcon";
import { ErrorBox } from "../components/ErrorBox";
import { SourceBadge } from "../components/SourceBadge";
import { localized } from "../i18n";
import { useApp } from "../state";

const FILTERS: (Category | "all")[] = ["all", "game", "emulator", "tool"];

/** The catalog, either the playable ports or the experimental ones. */
export function Discover({ maturity, onOpen }: { maturity: Maturity; onOpen: (id: string) => void }) {
  const { t, i18n } = useTranslation();
  const { catalog, catalogError, catalogLoading, reloadCatalog } = useApp();
  const [filter, setFilter] = useState<Category | "all">("all");
  const [system, setSystem] = useState("all");
  const [query, setQuery] = useState("");

  const inTab = useMemo(
    () => (catalog?.entries ?? []).filter((e) => (e.maturity ?? "playable") === maturity),
    [catalog, maturity],
  );
  const systems = useMemo(() => [...new Set(inTab.map((e) => e.system))].sort(), [inTab]);
  const categories = FILTERS.filter((f) => f === "all" || inTab.some((e) => e.category === f));

  const entries = useMemo(() => {
    const q = query.trim().toLowerCase();
    return inTab.filter(
      (e) =>
        (filter === "all" || e.category === filter) &&
        (system === "all" || e.system === system) &&
        (q === "" || e.name.toLowerCase().includes(q) || e.system.includes(q) || e.repo.toLowerCase().includes(q)),
    );
  }, [inTab, filter, system, query]);

  if (catalogError) return <ErrorBox error={catalogError} onRetry={() => void reloadCatalog()} />;
  if (!catalog) return <p className="muted">{catalogLoading ? t("common.loading") : null}</p>;

  return (
    <section className="page">
      {maturity === "experimental" && <p className="hint">{t("discover.experimentalHint")}</p>}
      <div className="toolbar">
        {categories.length > 2 && (
          <div className="chips" role="tablist" aria-label={t("tabs.discover")}>
            {categories.map((f) => (
              <button
                key={f}
                type="button"
                role="tab"
                aria-selected={filter === f}
                className={`chip ${filter === f ? "active" : ""}`}
                onClick={() => setFilter(f)}
              >
                {t(`category.${f}`)}
              </button>
            ))}
          </div>
        )}
        <div className="row">
          <select
            className="system-filter"
            value={system}
            onChange={(e) => setSystem(e.target.value)}
            aria-label={t("discover.system")}
          >
            <option value="all">{t("discover.allSystems")}</option>
            {systems.map((s) => (
              <option key={s} value={s}>
                {t(`systems.${s}`, { defaultValue: s.toUpperCase() })}
              </option>
            ))}
          </select>
          <input
            type="search"
            className="search"
            placeholder={t("discover.search")}
            aria-label={t("discover.search")}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
      </div>
      <CatalogStatus catalog={catalog} />
      <p className="muted small">{t("discover.count", { count: entries.length })}</p>
      {entries.length === 0 ? (
        <p className="muted empty">{t("discover.empty")}</p>
      ) : (
        <ul className="tile-grid">
          {entries.map((e) => (
            <li key={e.id}>
              <button type="button" className="tile" onClick={() => onOpen(e.id)}>
                <EntryIcon entry={e} />
                <span className="tile-name">{e.name}</span>
                <span className="tile-meta">
                  <span className="badge">{e.system.toUpperCase()}</span>
                  <span className="muted">{t(`category.${e.category}`)}</span>
                  <SourceBadge source={e.source} />
                </span>
                <span className="tile-desc">{localized(e.description, i18n.language)}</span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
