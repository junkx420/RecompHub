import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import type { Category } from "../api/types";
import { CatalogStatus } from "../components/CatalogStatus";
import { EntryIcon } from "../components/EntryIcon";
import { ErrorBox } from "../components/ErrorBox";
import { SourceBadge } from "../components/SourceBadge";
import { localized } from "../i18n";
import { useApp } from "../state";

const FILTERS: (Category | "all")[] = ["all", "game", "emulator", "tool"];

export function Discover({ onOpen }: { onOpen: (id: string) => void }) {
  const { t, i18n } = useTranslation();
  const { catalog, catalogError, catalogLoading, reloadCatalog } = useApp();
  const [filter, setFilter] = useState<Category | "all">("all");
  const [query, setQuery] = useState("");

  const entries = useMemo(() => {
    const q = query.trim().toLowerCase();
    return (catalog?.entries ?? []).filter(
      (e) =>
        (filter === "all" || e.category === filter) &&
        (q === "" || e.name.toLowerCase().includes(q) || e.system.includes(q) || e.repo.toLowerCase().includes(q)),
    );
  }, [catalog, filter, query]);

  if (catalogError) return <ErrorBox error={catalogError} onRetry={() => void reloadCatalog()} />;
  if (!catalog) return <p className="muted">{catalogLoading ? t("common.loading") : null}</p>;

  return (
    <section className="page">
      <div className="toolbar">
        <div className="chips" role="tablist" aria-label={t("tabs.discover")}>
          {FILTERS.map((f) => (
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
        <input
          type="search"
          className="search"
          placeholder={t("discover.search")}
          aria-label={t("discover.search")}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
      </div>
      <CatalogStatus catalog={catalog} />
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
