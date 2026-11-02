import { useState } from "react";
import { api, errorText } from "./api";
import { pad } from "./fmt";
import { APP_VERSION, INSTALL_COMMAND, type Update } from "./useUpdate";

function clock(ms: number): string {
  const d = new Date(ms);
  return `${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/** This build's version, whether a newer one is out, and how to get it. */
export default function AboutCard({ update }: { update: Update }) {
  const [copied, setCopied] = useState(false);
  const [logError, setLogError] = useState<string | null>(null);
  const { installing } = update;

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
      <h3>Phiên bản</h3>
      <p className="about-version">
        Bản đang dùng <strong>{APP_VERSION}</strong>
      </p>
      <div className="about-status">
        {update.checking ? (
          <span className="muted">Đang kiểm tra bản mới…</span>
        ) : update.available ? (
          <span className="about-new">Có bản {update.latest}.</span>
        ) : update.latest ? (
          <span className="text-ok">Đã là bản mới nhất.</span>
        ) : null}
        {update.checkedAt && !update.checking && <span className="muted">Kiểm tra lúc {clock(update.checkedAt)}</span>}
        <button className="btn small" onClick={() => update.check()} disabled={update.checking || installing !== null}>
          Kiểm tra bản cập nhật
        </button>
      </div>
      {update.available && update.canInstall && (
        <div className="about-install">
          <button className="btn primary" onClick={update.install} disabled={installing !== null}>
            {installing === null ? `Cập nhật lên ${update.latest}` : `Đang tải ${Math.round(installing * 100)}%…`}
          </button>
          <p className="hint">App tự khởi động lại; cài đặt và đăng nhập được giữ nguyên.</p>
        </div>
      )}
      {update.installError && <p className="notice error">{update.installError}</p>}
      {logError && <p className="notice error">{logError}</p>}

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
        <button className="link" onClick={() => api.openLogDir().catch((e) => setLogError(errorText(e)))}>
          Mở thư mục log
        </button>{" "}
        và gửi kèm file log khi báo lỗi. Log không chứa API key hay mật khẩu.
      </p>
    </section>
  );
}
