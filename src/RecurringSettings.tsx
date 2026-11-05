import { newId, type Activity, type IssueBrief, type RecurringTask } from "./api";
import { fmtHours } from "./fmt";
import { Icon } from "./icons";
import IssuePicker from "./IssuePicker";
import { subjectOf } from "./recurring";

const WEEKDAYS: [number, string][] = [
  [1, "T2"],
  [2, "T3"],
  [3, "T4"],
  [4, "T5"],
  [5, "T6"],
  [6, "T7"],
  [7, "CN"],
];

interface Props {
  tasks: RecurringTask[];
  workdays: number[];
  hoursPerDay: number;
  /** Work activities (no leave ones). */
  activities: Activity[];
  onChange: (tasks: RecurringTask[]) => void;
}

/** What `save` checks before writing the settings; null when fine. */
export function recurringProblem(tasks: RecurringTask[], hoursPerDay: number): string | null {
  for (const [i, t] of tasks.entries()) {
    const name = t.issue_id ? `“${subjectOf(t.label)}”` : `thứ ${i + 1}`;
    if (!t.issue_id) return `Chọn issue cho việc lặp lại ${name}.`;
    if (!(t.hours > 0 && t.hours <= hoursPerDay))
      return `Số giờ của việc lặp lại ${name} phải lớn hơn 0 và không quá ${fmtHours(hoursPerDay)}.`;
  }
  return null;
}

/**
 * Work done the same way on fixed weekdays (stand-up, weekly meeting): the
 * report flow adds it to each reported day before splitting the hours, and
 * the calendar's day panel can log it in one click.
 */
export default function RecurringSettings({ tasks, workdays, hoursPerDay, activities, onChange }: Props) {
  const patch = (id: string, p: Partial<RecurringTask>) =>
    onChange(tasks.map((t) => (t.id === id ? { ...t, ...p } : t)));
  const add = () =>
    onChange([
      ...tasks,
      {
        id: newId(),
        issue_id: 0,
        label: "",
        hours: 0.5,
        weekdays: [],
        activity_id: null,
        comments: "",
        paused: false,
      },
    ]);
  const perDay = tasks.filter((t) => !t.paused && t.weekdays.length === 0).reduce((s, t) => s + t.hours, 0);

  return (
    <section className="card recurring-card">
      <h3>Việc lặp lại</h3>
      <p className="hint">
        Việc cố định theo ngày trong tuần như daily hay họp sprint. Khi phân tích báo cáo, app tự thêm vào mỗi ngày và
        trừ giờ trước khi chia cho các việc khác; trên Lịch tháng có nút ghi nhanh cho ngày còn thiếu.
      </p>
      {tasks.length > 0 && (
        <ul className="recurring-list">
          {tasks.map((t) => {
            const value: IssueBrief | null = t.issue_id
              ? {
                  id: t.issue_id,
                  subject: subjectOf(t.label),
                  project_id: 0,
                  project_name: "",
                  tracker: "",
                  status: "",
                  parent_id: null,
                }
              : null;
            const name = t.issue_id ? subjectOf(t.label) : "việc lặp lại mới";
            return (
              <li key={t.id} className={`recurring-item${t.paused ? " is-paused" : ""}`}>
                <div className="field">
                  <span>Issue</span>
                  <IssuePicker
                    value={value}
                    onPick={(i) => patch(t.id, { issue_id: i.id, label: `#${i.id} ${i.subject}` })}
                    label={`Issue của ${name}`}
                    remote
                    empty="Gõ #id hoặc tên issue để tìm"
                  />
                </div>
                <div className="recurring-grid">
                  <label className="field">
                    <span>Số giờ</span>
                    <input
                      type="number"
                      inputMode="decimal"
                      min={0.25}
                      max={hoursPerDay}
                      step={0.25}
                      value={t.hours}
                      onChange={(e) => patch(t.id, { hours: Number(e.target.value) })}
                    />
                  </label>
                  <div className="field" role="group" aria-label={`Ngày của ${name}`}>
                    <span>{t.weekdays.length ? "Vào các ngày" : "Mọi ngày làm việc, hoặc chọn ngày"}</span>
                    <div className="toggles">
                      {WEEKDAYS.filter(([d]) => workdays.includes(d)).map(([d, label]) => (
                        <label key={d} className="toggle">
                          <input
                            type="checkbox"
                            checked={t.weekdays.includes(d)}
                            onChange={() =>
                              patch(t.id, {
                                weekdays: t.weekdays.includes(d)
                                  ? t.weekdays.filter((x) => x !== d)
                                  : [...t.weekdays, d].sort(),
                              })
                            }
                          />
                          <span>{label}</span>
                        </label>
                      ))}
                    </div>
                  </div>
                </div>
                <div className="recurring-grid">
                  <label className="field">
                    <span>Ghi chú</span>
                    <input
                      value={t.comments}
                      placeholder={t.issue_id ? subjectOf(t.label) : "Daily stand-up"}
                      onChange={(e) => patch(t.id, { comments: e.target.value })}
                    />
                  </label>
                  <label className="field">
                    <span>Hoạt động</span>
                    <select
                      value={t.activity_id ?? ""}
                      onChange={(e) => patch(t.id, { activity_id: e.target.value ? Number(e.target.value) : null })}
                    >
                      <option value="">Mặc định</option>
                      {activities.map((a) => (
                        <option key={a.id} value={a.id}>
                          {a.name}
                        </option>
                      ))}
                    </select>
                  </label>
                </div>
                <div className="recurring-foot">
                  <label className="check">
                    <input
                      type="checkbox"
                      checked={t.paused}
                      onChange={(e) => patch(t.id, { paused: e.target.checked })}
                    />
                    Tạm dừng
                  </label>
                  <button className="link danger" onClick={() => onChange(tasks.filter((x) => x.id !== t.id))}>
                    Xoá
                  </button>
                </div>
              </li>
            );
          })}
        </ul>
      )}
      {perDay > hoursPerDay / 2 && (
        <p className="notice text-partial">
          Việc lặp lại mỗi ngày đã chiếm {fmtHours(perDay)} trên {fmtHours(hoursPerDay)}; báo cáo sẽ còn ít giờ để chia.
        </p>
      )}
      <div className="card-foot">
        <div className="row">
          <button className="btn" onClick={add}>
            <Icon name="plus" size={16} />
            Thêm việc lặp lại
          </button>
        </div>
      </div>
    </section>
  );
}
