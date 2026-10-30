import type { InstalledSummary } from "./api/types";

/** Installed apps whose latest known release differs from what is installed. */
export function outdated(installed: InstalledSummary[], latest: Record<string, string>): InstalledSummary[] {
  return installed.filter((a) => latest[a.id] !== undefined && latest[a.id] !== a.version);
}
