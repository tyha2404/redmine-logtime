import { useModal } from "./useModal";
import { APP_VERSION, type Update } from "./useUpdate";

/** "A new version is out": what's new, then one click to download, install and restart. */
export default function UpdateDialog({ update, onClose }: { update: Update; onClose: () => void }) {
  const busy = update.installing !== null;
  const { close, ...modal } = useModal(onClose, busy);
  const pct = Math.round((update.installing ?? 0) * 100);

  return (
    <dialog {...modal} className="confirm-dialog update-dialog" aria-labelledby="update-title">
      <h2 id="update-title">Có bản mới {update.latest}</h2>
      <p>
        Bạn đang dùng bản {APP_VERSION}. App sẽ tải bản mới, cài rồi tự khởi động lại; cài đặt và đăng nhập được giữ
        nguyên.
      </p>

      {update.notes && (
        <section className="update-notes" aria-label="Có gì mới">
          <h3>Có gì mới</h3>
          <p>{update.notes}</p>
        </section>
      )}

      {busy && (
        <div className="update-progress">
          <span className="stat-bar" role="progressbar" aria-valuenow={pct} aria-valuemin={0} aria-valuemax={100}>
            <span style={{ width: `${pct}%` }} />
          </span>
          <p className="hint">{pct < 100 ? `Đang tải ${pct}%…` : "Đang cài, app sẽ khởi động lại…"}</p>
        </div>
      )}
      {update.installError && <p className="notice error">{update.installError}</p>}

      <div className="confirm-dialog-actions">
        <button className="btn ghost" onClick={() => close()} disabled={busy}>
          Để sau
        </button>
        <button className="btn primary" onClick={update.install} disabled={busy}>
          {busy ? "Đang cập nhật…" : update.installError ? "Thử lại" : "Cập nhật và khởi động lại"}
        </button>
      </div>
    </dialog>
  );
}
