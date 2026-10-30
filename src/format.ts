const UNITS = ["B", "KB", "MB", "GB", "TB"];

/** Human-readable size with binary steps, e.g. `48.0 MB`. */
export function formatBytes(bytes: number, locale?: string): string {
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit++;
  }
  const digits = unit === 0 ? 0 : 1;
  const number = value.toLocaleString(locale, { minimumFractionDigits: digits, maximumFractionDigits: digits });
  return `${number} ${UNITS[unit]}`;
}
