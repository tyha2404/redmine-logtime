import { useState } from "react";
import { api, errorText, type Activity, type Settings } from "./api";
import { allowanceOf, leaveSubject } from "./leave";
import { todayIso } from "./fmt";

/**
 * Leave the way HR runs it: a project per kind of leave, a parent issue
 * there every month, a sub-task per day off. "Tìm trên Redmine" fills it in.
 */
export default function LeaveSettings(props: {
  settings: Settings;
  activities: Activity[];
  onChange: (patch: Partial<Settings>) => void;
}) {
  const { settings, activities, onChange } = props;
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<{ kind: "ok" | "error"; text: string } | null>(null);
  const types = settings.leave_types;

  const discover = async () => {
    setBusy(true);
    setNotice(null);
    try {
      const found = await api.discoverLeave();
      // Old subjects mix decomposed accents and "BE -Tỷ"-style spacing.
      const name = found.name?.normalize("NFC").replace(/\s*-\s*/, " - ") ?? null;
      onChange({
        leave_types: found.types,
        ...(name && !settings.leave_name.trim() ? { leave_name: name } : {}),
      });
      setNotice({
        kind: "ok",
        text: `Tìm thấy ${found.types.length} loại nghỉ${name ? `, tên của bạn “${name}”` : ""}. Bấm “Lưu cài đặt” để giữ.`,
      });
    } catch (e) {
      setNotice({ kind: "error", text: errorText(e) });
    } finally {
      setBusy(false);
    }
  };

  const patchType = (projectId: number, patch: Partial<(typeof types)[number]>) =>
    onChange({ leave_types: types.map((t) => (t.project_id === projectId ? { ...t, ...patch } : t)) });

  return (
    <section className="card">
      <h2>Nghỉ phép</h2>
      <p className="hint">
        Mỗi tháng HR mở một task cha cho từng loại nghỉ. Khi xin nghỉ, app tạo một issue cho ngày đó dưới task cha của
        tháng rồi log giờ vào. Giờ log trong các project này được tính là ngày nghỉ trên lịch.
      </p>

      {types.length === 0 ? (
        <p className="muted leave-empty">Chưa có loại nghỉ nào. Bấm “Tìm trên Redmine” để app tự tìm.</p>
      ) : (
        <ul className="leave-types">
          {types.map((t) => (
            <li key={t.project_id}>
              <div>
                <strong>{t.label}</strong>
                <span className="muted small">{t.project_name}</span>
              </div>
              <label className="leave-allowance" title="Số ngày được nghỉ mỗi năm; để trống nếu không giới hạn">
                <input
                  type="number"
                  min={0}
                  step={0.5}
                  inputMode="decimal"
                  placeholder="—"
                  value={allowanceOf(t) ?? ""}
                  onChange={(e) =>
                    patchType(t.project_id, { days_per_year: e.target.value === "" ? null : Math.max(0, Number(e.target.value)) })
                  }
                  aria-label={`Số ngày ${t.label.toLowerCase()} mỗi năm`}
                />
                <span>ngày/năm</span>
              </label>
              <select
                aria-label={`Hoạt động khi log ${t.label.toLowerCase()}`}
                value={t.activity_id ?? ""}
                onChange={(e) => patchType(t.project_id, { activity_id: e.target.value ? Number(e.target.value) : null })}
                disabled={activities.length === 0}
              >
                <option value="">Hoạt động mặc định</option>
                {activities.map((a) => (
                  <option key={a.id} value={a.id}>
                    {a.name}
                  </option>
                ))}
              </select>
            </li>
          ))}
        </ul>
      )}

      <label className="field">
        <span>Tên của bạn trong tiêu đề issue nghỉ</span>
        <input
          placeholder="BE - Tỷ"
          value={settings.leave_name}
          onChange={(e) => onChange({ leave_name: e.target.value })}
        />
        {types[0] && (
          <small className="muted">Ví dụ: {leaveSubject(settings, types[0], todayIso(), "full")}</small>
        )}
      </label>

      <div className="card-foot">
        {notice && <p className={`notice ${notice.kind}`}>{notice.text}</p>}
        <div className="row">
          <button className="btn" onClick={discover} disabled={busy}>
            {busy ? "Đang tìm…" : types.length ? "Tìm lại trên Redmine" : "Tìm trên Redmine"}
          </button>
        </div>
      </div>
    </section>
  );
}
