import { forwardRef, useEffect, useImperativeHandle, useRef, useState } from "react";
import AboutCard from "./AboutCard";
import GchatSettings from "./GchatSettings";
import { api, errorText, type Activity, type Settings, type ToolStatus, type Tracker } from "./api";
import LeaveSettings from "./LeaveSettings";
import ProfileSection from "./ProfileSection";
import type { UpdateState } from "./useUpdate";

const WEEKDAYS: [number, string][] = [
  [1, "T2"],
  [2, "T3"],
  [3, "T4"],
  [4, "T5"],
  [5, "T6"],
  [6, "T7"],
  [7, "CN"],
];

type Notice = { kind: "ok" | "error"; text: string } | null;

/** Accepts YYYY-MM-DD or DD/MM/YYYY per line; returns ISO dates or the first bad line. */
function parseHolidays(text: string): { dates: string[]; bad: string | null } {
  const dates: string[] = [];
  for (const raw of text.split(/\n|,/)) {
    const line = raw.trim();
    if (!line) continue;
    let m = line.match(/^(\d{4})-(\d{1,2})-(\d{1,2})$/);
    let iso: string | null = null;
    if (m) iso = `${m[1]}-${m[2].padStart(2, "0")}-${m[3].padStart(2, "0")}`;
    m = line.match(/^(\d{1,2})\/(\d{1,2})\/(\d{4})$/);
    if (m) iso = `${m[3]}-${m[2].padStart(2, "0")}-${m[1].padStart(2, "0")}`;
    if (!iso || !isRealDate(iso)) return { dates, bad: line };
    dates.push(iso);
  }
  return { dates: [...new Set(dates)].sort(), bad: null };
}

/** True for a day that exists: 2026-02-28 yes, 2026-02-31 no (Date.parse rolls it over). */
function isRealDate(iso: string): boolean {
  const [y, m, d] = iso.split("-").map(Number);
  const date = new Date(y, m - 1, d);
  return date.getFullYear() === y && date.getMonth() === m - 1 && date.getDate() === d;
}

/** Comparable form of everything the save button writes. */
function snapshot(s: Settings, holidayText: string, apiKey: string): string {
  return JSON.stringify({ ...s, holidays: holidayText.trim(), apiKey: apiKey.trim() });
}

/** Lets the app save before it leaves a page with unsaved changes. */
export interface SettingsHandle {
  /** Saves the form; false when it could not (the reason is shown on the page). */
  save: () => Promise<boolean>;
}

interface Props {
  onSaved: () => void;
  /** Called whenever the form starts or stops differing from what is saved. */
  onDirtyChange?: (dirty: boolean) => void;
  update: UpdateState;
  /** Scroll to a section once loaded, e.g. "about" from the rail's update notice. */
  focus?: "about" | null;
}

const SettingsPage = forwardRef<SettingsHandle, Props>(function SettingsPage(
  { onSaved, onDirtyChange, update: updateState, focus },
  ref,
) {
  const [settings, setSettings] = useState<Settings | null>(null);
  /** What is on disk, to tell the user when the form has unsaved changes. */
  const [savedSnapshot, setSavedSnapshot] = useState("");
  const [hasKey, setHasKey] = useState(false);
  const [apiKey, setApiKey] = useState("");
  const [holidayText, setHolidayText] = useState("");
  const [activities, setActivities] = useState<Activity[]>([]);
  const [trackers, setTrackers] = useState<Tracker[]>([]);
  const [tools, setTools] = useState<ToolStatus[] | null>(null);
  const [aiCheck, setAiCheck] = useState<Notice | "running">(null);
  const [connNotice, setConnNotice] = useState<Notice>(null);
  const [saveNotice, setSaveNotice] = useState<Notice>(null);
  const [busy, setBusy] = useState(false);

  const loadLists = () => {
    api.listActivities().then(setActivities).catch(() => {});
    api.listTrackers().then(setTrackers).catch(() => {});
  };

  useEffect(() => {
    api
      .getSettings()
      .then((v) => {
        setSettings(v.settings);
        setSavedSnapshot(snapshot(v.settings, v.settings.holidays.join("\n"), ""));
        setHasKey(v.has_api_key);
        setHolidayText(v.settings.holidays.join("\n"));
        if (v.settings.redmine_url && v.has_api_key) loadLists();
      })
      .catch((e) => setSaveNotice({ kind: "error", text: errorText(e) }));
    api.checkTools().then(setTools);
  }, []);

  const loaded = settings !== null;
  const current = settings === null ? "" : snapshot(settings, holidayText, apiKey);
  const dirty = settings !== null && current !== savedSnapshot;
  // A save error is about the values it tried; editing them retires it.
  useEffect(() => setSaveNotice((n) => (n?.kind === "error" ? null : n)), [current]);
  useEffect(() => onDirtyChange?.(dirty), [dirty, onDirtyChange]);
  useEffect(() => () => onDirtyChange?.(false), [onDirtyChange]);

  const saveRef = useRef<() => Promise<boolean>>(async () => true);
  useImperativeHandle(ref, () => ({ save: () => saveRef.current() }), []);

  useEffect(() => {
    if (loaded && focus) document.getElementById(focus)?.scrollIntoView({ block: "start", behavior: "smooth" });
  }, [loaded, focus]);

  if (!settings) return <div className="page-pad muted">Đang tải cài đặt…</div>;

  const update = <K extends keyof Settings>(key: K, value: Settings[K]) =>
    setSettings({ ...settings, [key]: value });

  const toggleWorkday = (d: number) =>
    update(
      "workdays",
      settings.workdays.includes(d)
        ? settings.workdays.filter((x) => x !== d)
        : [...settings.workdays, d].sort(),
    );

  const testConnection = async () => {
    setBusy(true);
    setConnNotice(null);
    try {
      const user = await api.testConnection(
        settings.redmine_url,
        apiKey.trim() || null,
        settings.accept_invalid_certs,
      );
      const name = `${user.firstname} ${user.lastname}`.trim();
      setConnNotice({ kind: "ok", text: `Kết nối được. Bạn đang đăng nhập là ${name} (${user.login}).` });
    } catch (e) {
      setConnNotice({ kind: "error", text: errorText(e) });
    } finally {
      setBusy(false);
    }
  };

  const save = async (): Promise<boolean> => {
    const { dates, bad } = parseHolidays(holidayText);
    if (bad) {
      setSaveNotice({ kind: "error", text: `Ngày lễ "${bad}" không đúng hoặc không có thật. Dùng 2026-09-02 hoặc 02/09/2026.` });
      return false;
    }
    const hpd = settings.hours_per_day;
    if (!(Number.isFinite(hpd) && hpd > 0 && hpd <= 24)) {
      setSaveNotice({ kind: "error", text: "Số giờ mỗi ngày phải lớn hơn 0 và không quá 24." });
      return false;
    }
    if (settings.workdays.length === 0) {
      setSaveNotice({ kind: "error", text: "Chọn ít nhất một ngày làm việc trong tuần." });
      return false;
    }
    setBusy(true);
    setSaveNotice(null);
    try {
      const next = { ...settings, holidays: dates, redmine_url: settings.redmine_url.trim() };
      await api.saveSettings(next);
      setSettings(next);
      setSavedSnapshot(snapshot(next, dates.join("\n"), ""));
      if (apiKey.trim()) {
        await api.saveApiKey(apiKey);
        setApiKey("");
        setHasKey(true);
      }
      setHolidayText(dates.join("\n"));
      setSaveNotice({ kind: "ok", text: "Đã lưu cài đặt." });
      loadLists();
      return true;
    } catch (e) {
      setSaveNotice({ kind: "error", text: errorText(e) });
      return false;
    } finally {
      setBusy(false);
    }
  };
  saveRef.current = save;

  const removeKey = async () => {
    await api.saveApiKey("");
    setHasKey(false);
    setConnNotice({ kind: "ok", text: "Đã xoá API key khỏi máy." });
  };

  const activeActivities = activities.filter((a) => a.active);
  const leaveActivityIds = new Set(settings.leave_types.flatMap((t) => (t.activity_id ? [t.activity_id] : [])));
  /** Work never defaults to a leave activity. */
  const workActivities = activeActivities.filter((a) => !leaveActivityIds.has(a.id) || a.id === settings.default_activity_id);

  return (
    <div className="settings page-pad">
      <h1 className="page-title">Cài đặt</h1>

      <div className="settings-grid">

      <section className="card">
        <h2>Kết nối Redmine</h2>
        <label className="field">
          <span>Địa chỉ Redmine</span>
          <input
            type="url"
            placeholder="https://redmine.congty.vn"
            value={settings.redmine_url}
            onChange={(e) => update("redmine_url", e.target.value)}
          />
        </label>
        <label className="field">
          <span>API key</span>
          <input
            type="password"
            autoComplete="off"
            placeholder={hasKey ? "Đã lưu trên máy. Nhập key mới để thay." : "Trong Redmine: Tài khoản của tôi → API access key"}
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
          />
        </label>
        <label className="check">
          <input
            type="checkbox"
            checked={settings.accept_invalid_certs}
            onChange={(e) => update("accept_invalid_certs", e.target.checked)}
          />
          Chấp nhận chứng chỉ SSL tự ký (chỉ dùng cho Redmine nội bộ)
        </label>
        <div className="card-foot">
          {connNotice && <p className={`notice ${connNotice.kind}`}>{connNotice.text}</p>}
          <div className="row">
            <button className="btn" onClick={testConnection} disabled={busy || !settings.redmine_url}>
              Kiểm tra kết nối
            </button>
            {hasKey && (
              <button className="btn ghost danger" onClick={removeKey} disabled={busy}>
                Xoá API key
              </button>
            )}
          </div>
        </div>
      </section>

      <section className="card">
        <h2>Claude</h2>
        <p className="hint">Phân tích báo cáo cần Claude CLI (đã đăng nhập) và uv để chạy MCP Redmine.</p>
        {!tools ? (
          <p className="muted">Đang kiểm tra…</p>
        ) : (
          <ul className="tools">
            {tools.map((t) => (
              <li key={t.name}>
                <code>{t.name}</code>
                {t.version ? (
                  <span className="text-ok">{t.version}</span>
                ) : (
                  <span>
                    <span className="text-missing">Chưa cài.</span> Cài bằng <code>{t.install_hint}</code>
                  </span>
                )}
              </li>
            ))}
          </ul>
        )}
        <label className="check">
          <input
            type="checkbox"
            checked={settings.claude_ignore_user_settings}
            onChange={(e) => update("claude_ignore_user_settings", e.target.checked)}
          />
          <span>
            Không dùng <code>~/.claude/settings.json</code> khi gọi Claude (dùng tài khoản claude.ai đã đăng nhập). App tự
            bật khi cấu hình Claude của bạn bị từ chối API key.
          </span>
        </label>
        <div className="card-foot">
          {aiCheck && aiCheck !== "running" && <p className={`notice ${aiCheck.kind}`}>{aiCheck.text}</p>}
          <div className="row">
          <button
            className="btn"
            disabled={aiCheck === "running"}
            onClick={async () => {
              setAiCheck("running");
              try {
                const reply = await api.aiTest();
                const saved = await api.getSettings();
                setSettings((cur) =>
                  cur && { ...cur, claude_ignore_user_settings: saved.settings.claude_ignore_user_settings },
                );
                // The backend may have flipped this flag on disk; that is not an unsaved edit.
                setSavedSnapshot((prev) =>
                  JSON.stringify({ ...JSON.parse(prev), claude_ignore_user_settings: saved.settings.claude_ignore_user_settings }),
                );
                setAiCheck({ kind: "ok", text: `Claude trả lời: “${reply}”` });
              } catch (e) {
                setAiCheck({ kind: "error", text: errorText(e) });
              }
            }}
          >
            {aiCheck === "running" ? "Đang gọi Claude…" : "Chạy thử Claude"}
          </button>
          </div>
        </div>
      </section>

      <section className="card">
        <h2>Ngày làm việc</h2>
        <div className="field-pair">
        <label className="field hours-field">
          <span>Số giờ mỗi ngày</span>
          <input
            type="number"
            min={1}
            max={24}
            step={0.5}
            value={settings.hours_per_day}
            onChange={(e) => update("hours_per_day", Number(e.target.value))}
          />
        </label>
        <fieldset className="field">
          <legend>Các ngày làm việc trong tuần</legend>
          <div className="toggles">
            {WEEKDAYS.map(([d, label]) => (
              <label key={d} className="toggle">
                <input
                  type="checkbox"
                  checked={settings.workdays.includes(d)}
                  onChange={() => toggleWorkday(d)}
                />
                <span>{label}</span>
              </label>
            ))}
          </div>
        </fieldset>
        </div>
        <label className="field grow-field">
          <span>Ngày lễ, mỗi dòng một ngày (2026-09-02 hoặc 02/09/2026)</span>
          <textarea
            rows={4}
            placeholder={"2026-09-02\n01/01/2027"}
            value={holidayText}
            onChange={(e) => setHolidayText(e.target.value)}
          />
        </label>
      </section>

      <section className="card">
        <h2>Ghi logtime</h2>
        <p className="hint">Dùng khi app tạo task con cho công việc và ghi logtime lên Redmine.</p>
        <div className="field-grid">
        <ActivitySelect
          label="Hoạt động mặc định khi log công việc"
          value={settings.default_activity_id}
          options={workActivities}
          onChange={(v) => update("default_activity_id", v)}
        />
        <label className="field">
          <span>Tracker khi tạo task con</span>
          <select
            value={settings.subtask_tracker_id ?? ""}
            onChange={(e) => update("subtask_tracker_id", e.target.value ? Number(e.target.value) : null)}
            disabled={trackers.length === 0}
          >
            <option value="">{trackers.length ? "Chọn tracker" : "Kết nối Redmine để tải danh sách"}</option>
            {trackers.map((t) => (
              <option key={t.id} value={t.id}>
                {t.name}
              </option>
            ))}
          </select>
        </label>
        </div>
      </section>


      <GchatSettings />

      <LeaveSettings
        settings={settings}
        activities={activeActivities}
        onChange={(patch) => setSettings({ ...settings, ...patch })}
      />

      <ProfileSection
        settings={settings}
        trackers={trackers}
        onApply={(patch) => {
          setSettings({ ...settings, ...patch });
          setSaveNotice({ kind: "ok", text: "Đã điền gợi ý. Bấm “Lưu cài đặt” để giữ." });
        }}
      />

      <AboutCard update={updateState} />

      </div>

      <div className={`save-bar${dirty ? " is-dirty" : ""}`}>
        {/* A failed save keeps the changes unsaved, so its reason must show anyway. */}
        {saveNotice && (saveNotice.kind === "error" || !dirty) ? (
          <p className={`notice ${saveNotice.kind}`} role={saveNotice.kind === "error" ? "alert" : "status"}>
            {saveNotice.text}
          </p>
        ) : (
          dirty && <p className="save-dirty">Có thay đổi chưa lưu</p>
        )}
        <button className="btn" onClick={onSaved} disabled={busy}>
          Về lịch
        </button>
        <button className="btn primary" onClick={save} disabled={busy}>
          Lưu cài đặt
        </button>
      </div>
    </div>
  );
});

export default SettingsPage;

function ActivitySelect(props: {
  label: string;
  value: number | null;
  options: Activity[];
  onChange: (v: number | null) => void;
}) {
  const { label, value, options, onChange } = props;
  return (
    <label className="field">
      <span>{label}</span>
      <select
        value={value ?? ""}
        onChange={(e) => onChange(e.target.value ? Number(e.target.value) : null)}
        disabled={options.length === 0}
      >
        <option value="">{options.length ? "Chọn hoạt động" : "Kết nối Redmine để tải danh sách"}</option>
        {options.map((a) => (
          <option key={a.id} value={a.id}>
            {a.name}
          </option>
        ))}
      </select>
    </label>
  );
}
