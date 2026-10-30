import { useTranslation } from "react-i18next";
import { formatError } from "../errors";

export function ErrorBox({ error, onRetry }: { error: unknown; onRetry?: () => void }) {
  const { t } = useTranslation();
  return (
    <div className="error-box" role="alert">
      <strong>{t("errors.title")}</strong>
      <p>{formatError(error, t)}</p>
      {onRetry && (
        <button type="button" className="button" onClick={onRetry}>
          {t("common.retry")}
        </button>
      )}
    </div>
  );
}
