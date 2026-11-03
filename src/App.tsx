import { useCallback, useEffect, useRef, useState } from "react";
import { api, type Settings, type TrayAction } from "./api";
import CalendarPage, { type DayAsk } from "./CalendarPage";
import type { FillAsk } from "./ReportComposer";
import HistoryPage from "./HistoryPage";
import LeavePage from "./LeavePage";
import { Icon, type IconName } from "./icons";
import { inTauri, isMac } from "./platform";
import ReportPage from "./ReportPage";
import SettingsPage, { type SettingsHandle } from "./SettingsPage";
import SetupPage from "./SetupPage";
import { PageSkeleton } from "./Skeleton";
import { ToastProvider, useToast } from "./toast";
import UpdateDialog from "./UpdateDialog";
import { useModal } from "./useModal";
import { useUpdate } from "./useUpdate";

type Page = "setup" | "calendar" | "review" | "leave" | "history" | "settings";

type NavItem = { page: Exclude<Page, "setup">; label: string; icon: IconName };

/** The activity bar, like an editor's: daily work at the top. Settings sit at the foot. */
const NAV: NavItem[] = [
  { page: "calendar", label: "Lịch tháng", icon: "calendar" },
  { page: "review", label: "Ghi logtime", icon: "review" },
  { page: "leave", label: "Ngày nghỉ", icon: "leave" },
  { page: "history", label: "Lịch sử ghi", icon: "history" },
];

// On macOS the window has no title bar; the traffic lights sit over the top of the activity bar.
const overlayTitleBar = inTauri && isMac;

function initials(name: string): string {
  const words = name.trim().split(/\s+/).filter(Boolean);
  if (words.length === 0) return "?";
  // Vietnamese names end with the given name: "Họ Đệm Tên" → "T".
  const last = words[words.length - 1];
  return (words.length > 1 ? words[0][0] + last[0] : last.slice(0, 2)).toUpperCase();
}

export default function App() {
  return (
    <ToastProvider>
      <Shell />
    </ToastProvider>
  );
}

function Shell() {
  const [page, setPage] = useState<Page | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [settingsFocus, setSettingsFocus] = useState<"about" | null>(null);
  const [reportFocus, setReportFocus] = useState(false);
  const [dayAsk, setDayAsk] = useState<DayAsk | null>(null);
  const [fillAsk, setFillAsk] = useState<FillAsk | null>(null);
  const [me, setMe] = useState<string | null>(null);
  const update = useUpdate();
  const [showUpdate, setShowUpdate] = useState(false);
  const toast = useToast();
  const settingsPage = useRef<SettingsHandle>(null);
  const [settingsDirty, setSettingsDirty] = useState(false);
  /** Where the user asked to go while Settings had unsaved changes. */
  const [leaveTo, setLeaveTo] = useState<Page | null>(null);
  /** Pages slide in when the user switches, not when the first one replaces the loading skeleton. */
  const [pageAnim, setPageAnim] = useState(false);

  // The avatar shows who Redmine thinks you are.
  useEffect(() => {
    if (!settings?.redmine_url || page === "setup" || page === null) return;
    api
      .testConnection(settings.redmine_url, null, settings.accept_invalid_certs)
      .then((u) => setMe(`${u.lastname} ${u.firstname}`.trim() || u.login))
      .catch(() => setMe(null));
  }, [settings?.redmine_url, settings?.accept_invalid_certs, page === "setup"]);

  const loadSettings = useCallback(() => api.getSettings().then((v) => setSettings(v.settings)), []);

  // First run: a short setup until a URL and key are configured.
  useEffect(() => {
    api
      .getSettings()
      .then((v) => {
        setSettings(v.settings);
        const configured = !!v.settings.redmine_url && v.has_api_key;
        // Browser mock mode can open a page directly: ?page=review
        let forced = import.meta.env.VITE_MOCK === "1" ? new URLSearchParams(location.search).get("page") : null;
        if (forced === "report") forced = "review";
        setPage(configured ? ((forced as Page | null) ?? "calendar") : "setup");
      })
      .catch(() => setPage("settings"));
  }, []);

  const navigate = useCallback(
    (target: Page) => {
      if (target === "calendar" || target === "review" || target === "leave") loadSettings();
      setSettingsFocus(null);
      setReportFocus(false);
      setPageAnim(true);
      setPage(target);
    },
    [loadSettings],
  );

  // Leaving Settings with unsaved changes asks first instead of dropping them.
  const go = useCallback(
    (target: Page) => {
      if (page === "settings" && target !== "settings" && settingsDirty) return setLeaveTo(target);
      navigate(target);
    },
    [page, settingsDirty, navigate],
  );

  const openAbout = () => {
    go("settings");
    setSettingsFocus("about");
  };
  /** The update dialog when this build can install the new version; the install command otherwise. */
  const offerUpdate = () => (update.canInstall ? setShowUpdate(true) : openAbout());

  // Like an editor: a found update is announced once, then stays in the activity bar and the status bar.
  const announced = useRef<string | null>(null);
  useEffect(() => {
    if (!update.available || !update.canInstall || !update.latest || announced.current === update.latest) return;
    announced.current = update.latest;
    toast(`Có bản mới ${update.latest}.`, { action: { label: "Cập nhật", run: () => setShowUpdate(true) } });
  }, [update.available, update.canInstall, update.latest, toast]);

  // Menu bar / tray shortcuts. Before setup is done they only show the window.
  const onTray = useRef<(a: TrayAction) => void>(() => {});
  const askCount = useRef(0);
  onTray.current = (action) => {
    if (page === "setup" || page === null) return;
    go("calendar");
    setDayAsk({ panel: action === "quick" ? "quick" : "report", n: ++askCount.current });
  };
  useEffect(() => {
    const off = api.onTrayAction((a) => onTray.current(a));
    return () => {
      off.then((f) => f());
    };
  }, []);

  const leave = (target: Page) => {
    setLeaveTo(null);
    setSettingsDirty(false);
    navigate(target);
  };

  useEffect(() => {
    document.documentElement.dataset.titlebar = overlayTitleBar ? "overlay" : "native";
  }, []);

  if (page === "setup") {
    return (
      <div className="shell is-bare">
        {overlayTitleBar && <div className="titlebar" data-tauri-drag-region />}
        <SetupPage
          onDone={() => {
            loadSettings();
            go("calendar");
          }}
          onOpenSettings={() => setPage("settings")}
        />
      </div>
    );
  }

  const host = settings?.redmine_url ? hostOf(settings.redmine_url) : "";

  return (
    <div className="shell">
      {overlayTitleBar && <div className="titlebar" data-tauri-drag-region />}

      <nav className="activity" aria-label="Điều hướng">
        {NAV.map((item) => (
          <ActivityItem key={item.page} item={item} current={page === item.page} onClick={() => go(item.page)} />
        ))}
        <div className="activity-foot">
          {update.available && (
            <button
              className="activity-item"
              title={`Có bản ${update.latest}`}
              aria-label={`Có bản ${update.latest}, ${update.canInstall ? "bấm để cập nhật" : "xem cách cập nhật"}`}
              onClick={offerUpdate}
            >
              <span className="activity-icon has-dot">
                <Icon name="download" size={24} />
              </span>
            </button>
          )}
          {me && (
            <span className="activity-item is-static" title={`${me} trên ${host}`} aria-label={me}>
              <span className="avatar">{initials(me)}</span>
            </span>
          )}
          <ActivityItem
            item={{ page: "settings", label: "Cài đặt", icon: "settings" }}
            current={page === "settings"}
            onClick={() => go("settings")}
          />
        </div>
      </nav>

      <main className="main" data-page-anim={pageAnim ? "" : undefined}>
        {(page === null || !settings) && page !== "history" && page !== "settings" && <PageSkeleton />}
        {page === "calendar" && settings && (
          <CalendarPage
            settings={settings}
            onOpenSettings={() => go("settings")}
            onOpenHistory={() => go("history")}
            onOpenReport={() => {
              go("review");
              setReportFocus(true);
            }}
            onOpenLeave={() => go("leave")}
            onFillMissing={(dates) => {
              go("review");
              setFillAsk({ dates, n: ++askCount.current });
            }}
            ask={dayAsk}
            onAskTaken={() => setDayAsk(null)}
          />
        )}
        {page === "review" && settings && (
          <ReportPage
            settings={settings}
            focusReport={reportFocus}
            onOpenHistory={() => go("history")}
            onOpenSettings={() => go("settings")}
            fill={fillAsk}
            onFillTaken={() => setFillAsk(null)}
          />
        )}
        {page === "leave" && settings && <LeavePage settings={settings} onOpenSettings={() => go("settings")} />}
        {page === "history" && <HistoryPage />}
        {page === "settings" && (
          <SettingsPage
            ref={settingsPage}
            onDirtyChange={setSettingsDirty}
            update={update}
            focus={settingsFocus}
            onSaved={loadSettings}
          />
        )}
      </main>

      <footer className="statusbar">
        {host && (
          <span className="status-item is-remote" title={me ? `${me} trên ${host}` : host}>
            <Icon name="server" size={14} />
            {host}
          </span>
        )}
        <span className="status-fill" />
        {update.available && (
          <button className="status-item is-action" onClick={offerUpdate}>
            <Icon name="download" size={14} />
            Có bản {update.latest}
          </button>
        )}
        <span className="status-item">Logtime {__APP_VERSION__}</span>
      </footer>

      {showUpdate && <UpdateDialog update={update} onClose={() => setShowUpdate(false)} />}

      {leaveTo && (
        <UnsavedDialog
          onSave={async () => {
            if (await settingsPage.current?.save()) leave(leaveTo);
            else setLeaveTo(null);
          }}
          onDiscard={() => leave(leaveTo)}
          onStay={() => setLeaveTo(null)}
        />
      )}
    </div>
  );
}

/** An icon-only page button; the name shows on hover. */
function ActivityItem({ item, current, onClick }: { item: NavItem; current: boolean; onClick: () => void }) {
  return (
    <button
      className="activity-item"
      aria-current={current ? "page" : undefined}
      aria-label={item.label}
      title={item.label}
      onClick={onClick}
    >
      <span className="activity-icon">
        <Icon name={item.icon} size={24} />
      </span>
    </button>
  );
}

/** Asks what to do with unsaved settings before leaving the page. */
function UnsavedDialog({
  onSave,
  onDiscard,
  onStay,
}: {
  onSave: () => Promise<void>;
  onDiscard: () => void;
  onStay: () => void;
}) {
  const [busy, setBusy] = useState(false);
  const { close, ...modal } = useModal(onStay, busy);
  return (
    <dialog {...modal} className="confirm-dialog" aria-labelledby="unsaved-title">
      <h2 id="unsaved-title">Cài đặt có thay đổi chưa lưu</h2>
      <p>Lưu trước khi chuyển trang, hay bỏ những thay đổi này?</p>
      <div className="confirm-dialog-actions">
        <button className="btn ghost" onClick={() => close()} disabled={busy}>
          Ở lại
        </button>
        <button className="btn" onClick={() => close(onDiscard)} disabled={busy}>
          Bỏ thay đổi
        </button>
        <button
          className="btn primary"
          disabled={busy}
          onClick={async () => {
            setBusy(true);
            await onSave();
          }}
        >
          {busy ? "Đang lưu…" : "Lưu rồi chuyển"}
        </button>
      </div>
    </dialog>
  );
}

function hostOf(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return url;
  }
}
