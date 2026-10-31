import { useCallback, useEffect, useState } from "react";
import { api, type Settings } from "./api";
import CalendarPage from "./CalendarPage";
import HistoryPage from "./HistoryPage";
import ReportPage from "./ReportPage";
import ReviewPage from "./ReviewPage";
import SettingsPage from "./SettingsPage";
import { useDraft } from "./useDraft";

type Page = "calendar" | "report" | "review" | "history" | "settings";

export default function App() {
  const [page, setPage] = useState<Page | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  const draft = useDraft();

  const loadSettings = useCallback(() => api.getSettings().then((v) => setSettings(v.settings)), []);

  // First run: land on settings until a URL and key are configured.
  useEffect(() => {
    api
      .getSettings()
      .then((v) => {
        setSettings(v.settings);
        // Browser mock mode can open a page directly: ?page=review
        const forced = import.meta.env.VITE_MOCK === "1" ? new URLSearchParams(location.search).get("page") : null;
        setPage((forced as Page | null) ?? (v.settings.redmine_url && v.has_api_key ? "calendar" : "settings"));
      })
      .catch(() => setPage("settings"));
  }, []);

  const pending = draft.draft.items.length;
  const failed = draft.draft.items.filter((i) => i.last_error).length;

  const nav = (target: Page, label: string, badge?: React.ReactNode) => (
    <button
      className="rail-item"
      aria-current={page === target ? "page" : undefined}
      onClick={() => {
        if (target === "calendar" || target === "review") draft.reload();
        if (target === "calendar" || target === "report") loadSettings();
        setPage(target);
      }}
    >
      <span>{label}</span>
      {badge}
    </button>
  );

  return (
    <div className="shell">
      <nav className="rail" aria-label="Điều hướng">
        <div className="brand">
          <span className="brand-mark" aria-hidden />
          Logtime
        </div>
        {nav("calendar", "Lịch tháng")}
        {nav("report", "Nhập báo cáo")}
        {nav(
          "review",
          "Duyệt và ghi",
          pending > 0 && (
            <span className={`rail-badge${failed ? " has-failed" : ""}`} aria-label={`${pending} mục chờ ghi`}>
              {pending}
            </span>
          ),
        )}
        {nav("history", "Lịch sử ghi")}
        {nav("settings", "Cài đặt")}
        {draft.error && <p className="rail-error">{draft.error}</p>}
        {settings?.redmine_url && (
          <p className="rail-server" title={settings.redmine_url}>
            Đang dùng
            <strong>{hostOf(settings.redmine_url)}</strong>
          </p>
        )}
      </nav>
      <main className="main">
        {page === "calendar" && settings && (
          <CalendarPage
            settings={settings}
            draft={draft}
            onOpenSettings={() => setPage("settings")}
            onOpenReview={() => setPage("review")}
            onOpenReport={() => {
              loadSettings();
              setPage("report");
            }}
          />
        )}
        {page === "report" && settings && (
          <ReportPage
            settings={settings}
            draft={draft}
            onOpenReview={() => setPage("review")}
            onOpenSettings={() => setPage("settings")}
          />
        )}
        {page === "review" && <ReviewPage draft={draft} onOpenHistory={() => setPage("history")} />}
        {page === "history" && <HistoryPage onDraftChanged={draft.reload} />}
        {page === "settings" && (
          <SettingsPage
            onSaved={() => {
              loadSettings();
              setPage("calendar");
            }}
          />
        )}
      </main>
    </div>
  );
}

function hostOf(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return url;
  }
}
