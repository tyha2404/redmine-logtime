import { useEffect, useState } from "react";
import { api, errorText, type Settings } from "./api";

type State = { kind: "idle" } | { kind: "checking" } | { kind: "error"; text: string } | { kind: "done"; name: string };

/**
 * First run on a new machine: only what the calendar needs to load, a
 * Redmine address and the user's API key. Everything else has defaults and
 * lives in Settings.
 */
export default function SetupPage({ onDone, onOpenSettings }: { onDone: () => void; onOpenSettings: () => void }) {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [url, setUrl] = useState("");
  const [key, setKey] = useState("");
  const [hasKey, setHasKey] = useState(false);
  const [selfSigned, setSelfSigned] = useState(false);
  const [state, setState] = useState<State>({ kind: "idle" });

  useEffect(() => {
    api.getSettings().then((v) => {
      setSettings(v.settings);
      setUrl(v.settings.redmine_url);
      setSelfSigned(v.settings.accept_invalid_certs);
      setHasKey(v.has_api_key);
    });
  }, []);

  // The calendar opens a moment after the tank fills, so the success registers.
  useEffect(() => {
    if (state.kind !== "done") return;
    const t = setTimeout(onDone, 1400);
    return () => clearTimeout(t);
  }, [state, onDone]);

  const connect = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!settings) return;
    const redmineUrl = normalizeUrl(url);
    if (!redmineUrl) return setState({ kind: "error", text: "Nhập địa chỉ Redmine, ví dụ https://redmine.congty.vn" });
    if (!key.trim() && !hasKey) return setState({ kind: "error", text: "Nhập API key của bạn." });
    setState({ kind: "checking" });
    try {
      const user = await api.testConnection(redmineUrl, key.trim() || null, selfSigned);
      await api.saveSettings({ ...settings, redmine_url: redmineUrl, accept_invalid_certs: selfSigned });
      if (key.trim()) await api.saveApiKey(key.trim());
      setUrl(redmineUrl);
      setState({ kind: "done", name: `${user.firstname} ${user.lastname}`.trim() || user.login });
    } catch (err) {
      setState({ kind: "error", text: errorText(err) });
    }
  };

  const busy = state.kind === "checking" || state.kind === "done";

  return (
    <div className="setup">
      <div className={`setup-tank is-${state.kind}`} aria-hidden>
        <span className="setup-tank-fill" />
        <span className="setup-tank-line" />
      </div>

      <form className="setup-form" onSubmit={connect} noValidate>
        <h1 className="setup-title">Kết nối Redmine</h1>
        <p className="setup-lede">
          App đọc logtime của bạn để vẽ lịch tháng. Chưa có gì được ghi lên Redmine cho tới khi bạn bấm ghi ở bước duyệt.
        </p>

        <label className="field">
          <span>Địa chỉ Redmine</span>
          <input
            type="url"
            inputMode="url"
            placeholder="https://redmine.congty.vn"
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            disabled={busy}
            autoFocus
          />
        </label>

        <label className="field">
          <span>API key</span>
          <input
            type="password"
            autoComplete="off"
            placeholder={hasKey ? "Đã lưu trên máy. Nhập key mới để thay." : "40 ký tự chữ và số"}
            value={key}
            onChange={(e) => setKey(e.target.value)}
            disabled={busy}
          />
          <small className="muted">
            Trên Redmine: Tài khoản của tôi, cột bên phải, mục API access key, bấm Hiện. Key chỉ lưu trên máy này.
          </small>
        </label>

        <label className="check">
          <input type="checkbox" checked={selfSigned} onChange={(e) => setSelfSigned(e.target.checked)} disabled={busy} />
          Redmine nội bộ dùng chứng chỉ SSL tự ký
        </label>

        {state.kind === "error" && <p className="notice error">{state.text}</p>}
        {state.kind === "done" && (
          <p className="notice ok" role="status">
            Đã kết nối. Chào {state.name}, đang mở lịch tháng.
          </p>
        )}

        <div className="setup-actions">
          <button type="submit" className="btn primary" disabled={busy || !settings}>
            {state.kind === "checking" ? "Đang kết nối…" : "Kết nối và mở lịch"}
          </button>
          <button type="button" className="btn ghost" onClick={onOpenSettings} disabled={busy}>
            Mở cài đặt đầy đủ
          </button>
        </div>
      </form>
    </div>
  );
}

/** Adds https:// when missing and drops a trailing slash. */
function normalizeUrl(raw: string): string {
  const s = raw.trim().replace(/\/+$/, "");
  if (!s) return "";
  return /^https?:\/\//i.test(s) ? s : `https://${s}`;
}
