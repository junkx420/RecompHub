import type { TFunction } from "i18next";
import { isAppError } from "./api/types";

/** Turn a backend error (`{ code, params }`) or any thrown value into text. */
export function formatError(err: unknown, t: TFunction): string {
  if (isAppError(err)) {
    const params: Record<string, unknown> = { ...(err.params ?? {}) };
    if (err.code === "rate_limited" && typeof params.reset_at === "number") {
      const seconds = params.reset_at - Date.now() / 1000;
      params.minutes = Math.max(1, Math.ceil(seconds / 60));
    }
    const key = `errors.${err.code}`;
    return t(key, { ...params, defaultValue: t("errors.unknown", { message: err.code }) });
  }
  const message = err instanceof Error ? err.message : String(err);
  return t("errors.unknown", { message });
}
