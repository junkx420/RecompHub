import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Detail } from "./pages/Detail";
import { Discover } from "./pages/Discover";
import { Library } from "./pages/Library";
import { Settings } from "./pages/Settings";

type Tab = "discover" | "library" | "settings";
const TABS: Tab[] = ["discover", "library", "settings"];

export function App() {
  const { t } = useTranslation();
  const [tab, setTab] = useState<Tab>("discover");
  const [detailId, setDetailId] = useState<string | null>(null);

  function selectTab(next: Tab) {
    setTab(next);
    setDetailId(null);
  }

  return (
    <div className="app">
      <header className="topbar">
        <div className="brand">
          <span className="logo" aria-hidden="true">
            ◆
          </span>
          {t("app.name")}
        </div>
        <nav className="tabs" role="tablist" aria-label={t("app.name")}>
          {TABS.map((id) => (
            <button
              key={id}
              type="button"
              role="tab"
              aria-selected={tab === id}
              className={`tab ${tab === id ? "active" : ""}`}
              onClick={() => selectTab(id)}
            >
              {t(`tabs.${id}`)}
            </button>
          ))}
        </nav>
      </header>
      <main className="content">
        {detailId !== null ? (
          <Detail id={detailId} onBack={() => setDetailId(null)} />
        ) : tab === "discover" ? (
          <Discover onOpen={setDetailId} />
        ) : tab === "library" ? (
          <Library />
        ) : (
          <Settings />
        )}
      </main>
    </div>
  );
}
