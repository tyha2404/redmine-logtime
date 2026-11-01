import { useState } from "react";
import { api, errorText } from "./api";
import { inTauri } from "./platform";
import { APP_VERSION, INSTALL_COMMAND, type UpdateState } from "./useUpdate";

const MOCK = import.meta.env.VITE_MOCK === "1";

/** This build's version, whether a newer one is out, and how to get it. */
export default function AboutCard({ update }: { update: UpdateState }) {
  const [copied, setCopied] = useState(false);
  /** Download progress 0–1 while installing, or null. */
  const [installing, setInstalling] = useState<number | null>(null);
  const [installError, setInstallError] = useState<string | null>(null);

  const install = async () => {
    setInstallError(null);
    setInstalling(0);
    const unlisten = await api.onUpdateProgress((p) => setInstalling(p.total ? p.downloaded / p.total : 0));
    try {
      await api.installUpdate(); // restarts the app when it succeeds
    } catch (e) {
      setInstallError(errorText(e));
      setInstalling(null);
    } finally {
      unlisten();
    }
  };

  const copy = async () => {
    if (!INSTALL_COMMAND) return;
    try {
      await navigator.clipboard.writeText(INSTALL_COMMAND);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // Clipboard can be blocked; the command stays selectable below.
    }
  };

  return (
    <section className="card about" id="about">
      <h2>Phiên bản</h2>
      <p className="about-version">
        Bản đang dùng <strong>{APP_VERSION}</strong>
      </p>
      {update.checking && <p className="muted about-status">Đang kiểm tra bản mới…</p>}
      {!update.checking && update.latest && !update.available && <p className="text-ok about-status">Đã là bản mới nhất.</p>}
      {update.available && <p className="about-new about-status">Có bản {update.latest}.</p>}
      {update.available && (inTauri || MOCK) && (
        <div className="about-install">
          <button className="btn primary" onClick={install} disabled={installing !== null}>
            {installing === null ? `Cập nhật lên ${update.latest}` : `Đang tải ${Math.round(installing * 100)}%…`}
          </button>
          <p className="hint">App tự khởi động lại; cài đặt và đăng nhập được giữ nguyên.</p>
        </div>
      )}
      {installError && <p className="notice error">{installError}</p>}

      {INSTALL_COMMAND ? (
        <>
          <p className="hint">
            {update.available
              ? "Hoặc đóng app, chạy lệnh dưới trong Terminal rồi mở lại. Cài đặt và API key được giữ nguyên."
              : "Lệnh này cài app trên máy mới, và cập nhật lên bản mới nhất khi chạy lại."}
          </p>
          <div className="about-cmd">
            <code>{INSTALL_COMMAND}</code>
            <button className="btn small" onClick={copy}>
              {copied ? "Đã chép" : "Chép lệnh"}
            </button>
          </div>
          {update.error && <p className="hint">{update.error}</p>}
        </>
      ) : (
        <p className="hint">
          Bản build từ mã nguồn. Bản phát hành trên GitHub hiện lệnh cài và báo khi có bản mới tại đây.
        </p>
      )}
      <p className="hint about-logs">
        Gặp lỗi?{" "}
        <button className="link" onClick={() => api.openLogDir().catch((e) => setInstallError(errorText(e)))}>
          Mở thư mục log
        </button>{" "}
        và gửi kèm file log khi báo lỗi. Log không chứa API key hay mật khẩu.
      </p>
    </section>
  );
}
