import { useTranslation } from "react-i18next";

export function Library() {
  const { t } = useTranslation();
  return (
    <section className="page">
      <div className="empty-state">
        <p>{t("library.empty")}</p>
        <p className="muted">{t("library.emptyHint")}</p>
      </div>
    </section>
  );
}
