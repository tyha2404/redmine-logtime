import { useState } from "react";
import { APP_VERSION, INSTALL_COMMAND, type UpdateState } from "./useUpdate";

/** This build's version, whether a newer one is out, and how to get it. */
export default function AboutCard({ update }: { update: UpdateState }) {
  const [copied, setCopied] = useState(false);

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

      {INSTALL_COMMAND ? (
        <>
          <p className="hint">
            {update.available
              ? "Đóng app, chạy lệnh dưới trong Terminal rồi mở lại. Cài đặt và API key được giữ nguyên."
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
    </section>
  );
}
