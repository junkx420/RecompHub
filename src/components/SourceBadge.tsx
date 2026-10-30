import { useTranslation } from "react-i18next";
import type { EntrySource } from "../api/types";

export function SourceBadge({ source }: { source: EntrySource }) {
  const { t } = useTranslation();
  if (source.kind === "official") return null;
  return (
    <span className="badge badge-warning" title={source.url}>
      {t("badge.unverified")}
    </span>
  );
}
