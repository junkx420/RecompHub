import { useTranslation } from "react-i18next";
import type { LoadedCatalog } from "../api/types";

export function CatalogStatus({ catalog }: { catalog: LoadedCatalog }) {
  const { t, i18n } = useTranslation();
  const date = catalog.fetched_at
    ? new Date(catalog.fetched_at).toLocaleString(i18n.language, { dateStyle: "medium", timeStyle: "short" })
    : null;
  return (
    <div className={`catalog-status origin-${catalog.origin}`}>
      <span>{t(`catalog.origin.${catalog.origin}`)}</span>
      {date && <span className="muted"> · {t("catalog.fetchedAt", { date })}</span>}
      {catalog.warnings.length > 0 && (
        <span className="muted" title={catalog.warnings.join("\n")}>
          {" "}
          · {t("catalog.warnings", { count: catalog.warnings.length })}
        </span>
      )}
    </div>
  );
}
