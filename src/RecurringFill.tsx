import { useState } from "react";
import type { FixedEntry, MonthSummary, Settings, WritePlan } from "./api";
import { fmtHours, fmtDayShort, weekdayShort } from "./fmt";
import { recurringDue, recurringId, recurringItem, subjectOf } from "./recurring";
import { useModal } from "./useModal";

/** The recurring work still due in a month: each workday up to today, by the same rule as a day. */
export function monthRecurring(settings: Settings, summary: MonthSummary | null, today: string): FixedEntry[] {
  return (summary?.days ?? []).filter((d) => d.date <= today).flatMap((d) => recurringDue(settings, d));
}

interface Props {
  entries: FixedEntry[];
  settings: Settings;
  month: number;
  /** The ticked entries as one write; the write dialog confirms it. */
  onConfirm: (plan: WritePlan) => void;
  onClose: () => void;
}

/** "Ghi việc lặp lại": every due entry of the month ticked, so the user unticks the days they skipped. */
export default function RecurringFill({ entries, settings, month, onConfirm, onClose }: Props) {
  const { close, ...modal } = useModal(onClose);
  /** Unticked entries, by `recurringId`. */
  const [off, setOff] = useState<Set<string>>(() => new Set());
  const picked = entries.filter((e) => !off.has(recurringId(e)));
  const hours = picked.reduce((s, e) => s + e.hours, 0);

  const toggle = (ids: string[], on: boolean) =>
    setOff((prev) => {
      const next = new Set(prev);
      for (const id of ids) {
        if (on) next.delete(id);
        else next.add(id);
      }
      return next;
    });

  /** One switch per task, for "I skipped every daily this month". */
  const tasks = [...new Set(entries.map((e) => e.id))].map((id) => {
    const mine = entries.filter((e) => e.id === id);
    const ids = mine.map(recurringId);
    return { id, label: mine[0].label, hours: mine[0].hours, ids, on: ids.filter((x) => !off.has(x)).length };
  });

  return (
    <dialog {...modal} className="confirm-dialog recurring-fill" aria-labelledby="rfill-title">
      <h2 id="rfill-title">Ghi việc lặp lại tháng {month}</h2>
      <p>Các ngày làm việc tới hôm nay chưa có giờ trên issue của việc lặp lại. Bỏ chọn những ngày bạn không làm.</p>

      <div className="rfill-tasks">
        {tasks.map((t) => (
          <label key={t.id} className="check">
            <input
              type="checkbox"
              checked={t.on === t.ids.length}
              ref={(el) => {
                if (el) el.indeterminate = t.on > 0 && t.on < t.ids.length;
              }}
              onChange={(e) => toggle(t.ids, e.target.checked)}
            />
            <span>
              {subjectOf(t.label)}{" "}
              <span className="muted">
                {fmtHours(t.hours)} · {t.on}/{t.ids.length} ngày
              </span>
            </span>
          </label>
        ))}
      </div>

      <div className="list-card rfill-list">
        <p className="list-card-head">
          Đã chọn {picked.length}/{entries.length} mục · {fmtHours(hours)}
        </p>
        <ul>
          {entries.map((e) => {
            const id = recurringId(e);
            return (
              <li key={id}>
                <label className="rfill-row">
                  <input type="checkbox" checked={!off.has(id)} onChange={(ev) => toggle([id], ev.target.checked)} />
                  <span className="rfill-date">
                    {weekdayShort(e.date)} {fmtDayShort(e.date)}
                  </span>
                  <span className="rfill-task">{subjectOf(e.label)}</span>
                  <span className="rfill-hours">{fmtHours(e.hours)}</span>
                </label>
              </li>
            );
          })}
        </ul>
      </div>

      <div className="confirm-dialog-actions">
        <button className="btn ghost" onClick={() => close()}>
          Huỷ
        </button>
        <button
          className="btn primary"
          disabled={picked.length === 0}
          onClick={() => close(() => onConfirm({ subtasks: [], items: picked.map((e) => recurringItem(e, settings)) }))}
        >
          Ghi {picked.length} mục ({fmtHours(hours)})
        </button>
      </div>
    </dialog>
  );
}
