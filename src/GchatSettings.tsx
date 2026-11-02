import { useEffect, useState } from "react";
import { api, errorText, type GchatStatus } from "./api";

type Notice = { kind: "ok" | "error"; text: string } | null;

/** Google sign-in for importing reports. Saved on its own, not with "Lưu cài đặt". */
export default function GchatSettings() {
  const [status, setStatus] = useState<GchatStatus | null>(null);
  const [clientId, setClientId] = useState("");
  const [secret, setSecret] = useState("");
  const [connecting, setConnecting] = useState(false);
  const [notice, setNotice] = useState<Notice>(null);

  useEffect(() => {
    api.gchatStatus().then((s) => {
      setStatus(s);
      setClientId(s.client_id ?? "");
    });
  }, []);

  const connect = async () => {
    setConnecting(true);
    setNotice(null);
    try {
      const s = await api.gchatConnect(clientId, secret);
      setStatus(s);
      setSecret("");
      setNotice({ kind: "ok", text: "Đã kết nối. Vào Ghi logtime → Lấy từ Google Chat." });
    } catch (e) {
      setNotice({ kind: "error", text: errorText(e) });
    } finally {
      setConnecting(false);
    }
  };

  const disconnect = async () => {
    setNotice(null);
    try {
      setStatus(await api.gchatDisconnect());
    } catch (e) {
      setNotice({ kind: "error", text: errorText(e) });
    }
  };

  return (
    <section className="card">
      <h3>Google Chat</h3>
      <p className="hint">
        Lấy báo cáo bạn đã gửi trong group Google Chat để AI phân tích. App chỉ đọc tin nhắn của bạn, không gửi gì. Cần
        OAuth client loại Desktop app do admin Google Workspace của công ty tạo.
      </p>
      {!status ? (
        <p className="muted">Đang kiểm tra…</p>
      ) : status.connected ? (
        <p>
          Đã kết nối bằng <strong>{status.email || "tài khoản Google"}</strong>.
        </p>
      ) : (
        <>
          <label className="field">
            <span>Client ID</span>
            <input
              value={clientId}
              onChange={(e) => setClientId(e.target.value)}
              placeholder="…apps.googleusercontent.com"
              autoComplete="off"
              spellCheck={false}
              disabled={connecting}
            />
          </label>
          <label className="field">
            <span>Client secret</span>
            <input
              type="password"
              value={secret}
              onChange={(e) => setSecret(e.target.value)}
              autoComplete="off"
              disabled={connecting}
            />
          </label>
        </>
      )}
      <div className="card-foot">
        {connecting && <p className="notice">Đang chờ bạn đăng nhập và cho phép trong trình duyệt…</p>}
        {notice && <p className={`notice ${notice.kind}`}>{notice.text}</p>}
        {status && (
          <div className="row">
            {status.connected ? (
              <button className="btn ghost danger" onClick={disconnect}>
                Ngắt kết nối
              </button>
            ) : connecting ? (
              <button className="btn" onClick={() => api.gchatCancelConnect()}>
                Huỷ
              </button>
            ) : (
              <button className="btn" onClick={connect} disabled={!clientId.trim() || !secret.trim()}>
                Kết nối Google
              </button>
            )}
          </div>
        )}
      </div>
    </section>
  );
}
