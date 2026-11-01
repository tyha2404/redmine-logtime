import { useEffect, useMemo, useRef, useState } from "react";
import { api, newId, type DaySummary, type PlanItem, type ReviewView, type Settings, type SubtaskDraft, type TimeEntry } from "./api";
import { fmtDayShort, fmtHours, isoOf, parseIso, todayIso, weekdayShort } from "./fmt";
import { Icon } from "./icons";
import { useToast } from "./toast";
import type { DraftApi } from "./useDraft";

const EPS = 1e-6;
/** Covered by the totals row, so not repeated under the table. */
const HOUR_WARNINGS = /^(Vượt|Còn thiếu) /;

interface Props {
  view: ReviewView;
  draft: DraftApi;
  settings: Settings;
  selected: Set<string>;
  onToggle: (ids: string[], on: boolean) => void;
  disabled: boolean;
}

/** One queued target (issue or new sub-task) across a week. */
interface QueueRow {
  key: string;
  label: string;
  kind: PlanItem["kind"];
  subtask?: SubtaskDraft;
  items: PlanItem[];
}

/** What is already on Redmine for one issue across a week. */
interface ExistingRow {
  key: string;
  label: string;
  hint: string;
  byDate: Map<string, TimeEntry[]>;
}

type DayState = "ok" | "short" | "over" | "off" | "future";

/**
 * The queue as a timesheet, one table per week: a row per task, a column per
 * day, the hours in the cells. What is already on Redmine sits underneath in
 * grey, and the bottom row adds both up against the daily quota, so a short
 * or overfull day stands out at a glance, including days with nothing queued.
 */
export default function WeekGrid({ view, draft, settings, selected, onToggle, disabled }: Props) {
  const items = draft.draft.items;
  const weeks = useMemo(() => weeksOf(items.map((i) => i.date)), [items]);
  const months = useMemo(
    () => [...new Set(weeks.flat().map((d) => d.slice(0, 7)))].sort(),
    [weeks],
  );

  // Redmine's logtimes for days that have nothing queued (the review only
  // covers queued days). Refetched when items come or go, e.g. after a write.
  const [summary, setSummary] = useState<Map<string, DaySummary>>(new Map());
  const monthsKey = months.join(",");
  useEffect(() => {
    let live = true;
    Promise.all(months.map((m) => api.monthSummary(Number(m.slice(0, 4)), Number(m.slice(5, 7)))))
      .then((all) => live && setSummary(new Map(all.flatMap((s) => s.days.map((d) => [d.date, d] as const)))))
      .catch(() => {});
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [monthsKey, items.length]);

  const reviewByDate = useMemo(() => new Map(view.days.map((d) => [d.date, d])), [view]);
  const existingOf = (date: string): TimeEntry[] | null =>
    reviewByDate.get(date)?.existing ?? summary.get(date)?.entries ?? null;

  const [editing, setEditing] = useState<string | null>(null);

  return (
    <div className="wgrid-weeks">
      {weeks.map((week) => (
        <Week
          key={week[0]}
          week={week}
          draft={draft}
          settings={settings}
          quota={view.hours_per_day}
          selected={selected}
          onToggle={onToggle}
          disabled={disabled}
          existingOf={existingOf}
          warningsOf={(date) => reviewByDate.get(date)?.warnings ?? []}
          editing={editing}
          setEditing={setEditing}
        />
      ))}
    </div>
  );
}

function Week(props: {
  week: string[];
  draft: DraftApi;
  settings: Settings;
  quota: number;
  selected: Set<string>;
  onToggle: (ids: string[], on: boolean) => void;
  disabled: boolean;
  existingOf: (date: string) => TimeEntry[] | null;
  warningsOf: (date: string) => string[];
  editing: string | null;
  setEditing: (key: string | null) => void;
}) {
  const { week, draft, settings, quota, selected, onToggle, disabled, existingOf, warningsOf, editing, setEditing } = props;
  const toast = useToast();
  const today = todayIso();
  const inWeek = new Set(week);
  const weekItems = draft.draft.items.filter((i) => inWeek.has(i.date));

  const rows = queueRows(weekItems, draft.draft.subtasks);
  const existingRows = existingRowsOf(week, existingOf, settings);

  // Workdays always; weekends and holidays only when something is on them.
  const columns = week.filter(
    (d) => isWorkday(d, settings) || weekItems.some((i) => i.date === d) || (existingOf(d) ?? []).length > 0,
  );

  const dayInfo = (date: string) => {
    const existing = existingOf(date);
    const existingHours = (existing ?? []).reduce((s, e) => s + e.hours, 0);
    const planned = weekItems.filter((i) => i.date === date).reduce((s, i) => s + i.hours, 0);
    const total = existingHours + planned;
    const work = isWorkday(date, settings);
    const known = existing !== null || planned > 0;
    let state: DayState;
    // Not loaded from Redmine yet: no verdict rather than a false "short".
    if (!known) state = "future";
    else if (total > quota + EPS) state = "over";
    else if (!work) state = "off";
    else if (total + EPS >= quota) state = "ok";
    else state = date > today && planned === 0 ? "future" : "short";
    return { total, existingHours, planned, state, known };
  };

  const days = columns.map((d) => ({ date: d, ...dayInfo(d) }));
  const short = days.filter((d) => d.state === "short");
  const over = days.filter((d) => d.state === "over");
  const plannedTotal = weekItems.reduce((s, i) => s + i.hours, 0);

  const errors = weekItems.filter((i) => i.last_error);
  const warnings = columns.flatMap((d) => warningsOf(d).filter((w) => !HOUR_WARNINGS.test(w)).map((w) => ({ date: d, text: w })));

  const removeItems = async (ids: string[], what: string) => {
    const removed = await draft.remove(ids);
    toast(`Đã bỏ ${what} khỏi hàng chờ.`, { action: { label: "Hoàn tác", run: () => draft.restore(removed) } });
  };

  const commitCell = async (row: QueueRow, date: string, cellItems: PlanItem[], raw: string) => {
    setEditing(null);
    const value = raw.trim().replace(",", ".");
    const h = value === "" ? 0 : Number(value);
    if (!Number.isFinite(h) || h < 0 || h > 24) {
      toast("Số giờ phải từ 0 đến 24.", { tone: "error" });
      return;
    }
    const [item] = cellItems;
    if (!item) {
      if (h === 0) return;
      const tpl = row.items[0];
      await draft.add([{ ...tpl, id: newId(), date, hours: h, last_error: null }]);
      return;
    }
    if (Math.abs(h - item.hours) < EPS) return;
    if (h === 0) return removeItems([item.id], `${fmtHours(item.hours)} ngày ${fmtDayShort(date)}`);
    await draft.update(item.id, { hours: h });
  };

  return (
    <section className="wgrid">
      <header className="wgrid-head">
        <h3>
          Tuần {fmtDayShort(week[0])} – {fmtDayShort(week[6])}
        </h3>
        <p className="wgrid-sum">
          <strong>{fmtHours(plannedTotal)}</strong> chờ ghi
          {short.length > 0 && <span className="wgrid-flag is-short">{short.length} ngày thiếu giờ</span>}
          {over.length > 0 && <span className="wgrid-flag is-over">{over.length} ngày vượt giờ</span>}
          {short.length === 0 && over.length === 0 && <span className="wgrid-flag is-ok">Đủ giờ cả tuần</span>}
        </p>
      </header>

      <div className="wgrid-scroll">
        <table className="wgrid-table" style={{ ["--cols" as string]: columns.length }}>
          <thead>
            <tr>
              <th className="wgrid-task" scope="col">
                Task
              </th>
              {days.map((d) => (
                <th key={d.date} scope="col" className={`wgrid-day is-${d.state}${d.date === today ? " is-today" : ""}`}>
                  <span className="wgrid-wd">{weekdayShort(d.date)}</span>
                  <span className="wgrid-dn">{parseIso(d.date).getDate()}</span>
                </th>
              ))}
              <th scope="col" className="wgrid-total">
                Tổng
              </th>
            </tr>
          </thead>

          <tbody>
            {rows.map((row) => {
              const ids = row.items.map((i) => i.id);
              const on = ids.filter((id) => selected.has(id)).length;
              const rowHours = row.items.reduce((s, i) => s + i.hours, 0);
              const notes = [...new Set(row.items.map((i) => i.comments.trim()).filter(Boolean))];
              return (
                <tr key={row.key} className={`wgrid-row${row.kind === "leave" ? " is-leave" : ""}${on === 0 ? " is-off" : ""}`}>
                  <th scope="row" className="wgrid-task">
                    <div className="wgrid-task-in">
                      <input
                        type="checkbox"
                        checked={on === ids.length}
                        ref={(el) => {
                          if (el) el.indeterminate = on > 0 && on < ids.length;
                        }}
                        disabled={disabled}
                        onChange={(e) => onToggle(ids, e.target.checked)}
                        aria-label={`Chọn ${row.label}`}
                      />
                      <div className="wgrid-label" title={notes.join("\n")}>
                        <span className="wgrid-name">
                          {row.subtask && <span className="ritem-new">{row.kind === "leave" ? "Nghỉ" : "Mới"}</span>}
                          {row.label}
                        </span>
                        {row.subtask && row.kind === "work" && (
                          <span className="wgrid-sub">
                            dưới #{row.subtask.parent_id} {row.subtask.parent_subject}
                          </span>
                        )}
                        {notes.length > 0 && <span className="wgrid-notes">{notes.join(" · ")}</span>}
                      </div>
                      <button
                        className="icon-btn wgrid-x"
                        disabled={disabled}
                        onClick={() => removeItems(ids, `${row.label} (${fmtHours(rowHours)})`)}
                        aria-label={`Bỏ ${row.label} của tuần này khỏi hàng chờ`}
                        title="Bỏ cả dòng khỏi hàng chờ"
                      >
                        <Icon name="close" size={14} />
                      </button>
                    </div>
                  </th>
                  {columns.map((date) => {
                    const cellItems = row.items.filter((i) => i.date === date);
                    const key = `${row.key}|${date}`;
                    return (
                      <Cell
                        key={date}
                        items={cellItems}
                        selected={selected}
                        disabled={disabled}
                        canAdd={row.kind === "work"}
                        editing={editing === key}
                        onEdit={() => setEditing(key)}
                        onCancel={() => setEditing(null)}
                        onCommit={(raw) => commitCell(row, date, cellItems, raw)}
                        label={`${row.label}, ${weekdayShort(date)} ${fmtDayShort(date)}`}
                      />
                    );
                  })}
                  <td className="wgrid-total">{fmtHours(rowHours)}</td>
                </tr>
              );
            })}
          </tbody>

          {existingRows.length > 0 && (
            <tbody className="wgrid-existing">
              <tr>
                <th scope="rowgroup" colSpan={columns.length + 2} className="wgrid-group">
                  Đã có trên Redmine
                </th>
              </tr>
              {existingRows.map((row) => {
                const total = [...row.byDate.values()].flat().reduce((s, e) => s + e.hours, 0);
                return (
                  <tr key={row.key} className="wgrid-row">
                    <th scope="row" className="wgrid-task">
                      <div className="wgrid-task-in">
                        <span className="wgrid-dot" aria-hidden />
                        <div className="wgrid-label" title={row.hint}>
                          <span className="wgrid-name">{row.label}</span>
                          {row.hint && <span className="wgrid-notes">{row.hint}</span>}
                        </div>
                      </div>
                    </th>
                    {columns.map((date) => {
                      const es = row.byDate.get(date) ?? [];
                      const h = es.reduce((s, e) => s + e.hours, 0);
                      return (
                        <td key={date} className="wgrid-cell is-existing" title={es.map((e) => e.comments).filter(Boolean).join("\n")}>
                          {h > 0 ? fmtHours(h).slice(0, -1) : ""}
                        </td>
                      );
                    })}
                    <td className="wgrid-total">{fmtHours(total)}</td>
                  </tr>
                );
              })}
            </tbody>
          )}

          <tfoot>
            <tr>
              <th scope="row" className="wgrid-task">
                Tổng ngày <span className="muted">/ {fmtHours(quota)}</span>
              </th>
              {days.map((d) => (
                <td
                  key={d.date}
                  className={`wgrid-daytotal is-${d.state}`}
                  title={dayTitle(d, quota)}
                >
                  {d.state === "future" && d.total === 0 ? null : d.known ? (
                    <>
                      <span className="wgrid-dt">{fmtHours(d.total).slice(0, -1)}</span>
                      <span className="wgrid-dmark">{dayMark(d, quota)}</span>
                    </>
                  ) : (
                    <span className="muted">…</span>
                  )}
                </td>
              ))}
              <td className="wgrid-total">{fmtHours(days.reduce((s, d) => s + d.total, 0))}</td>
            </tr>
          </tfoot>
        </table>
      </div>

      {(errors.length > 0 || warnings.length > 0) && (
        <ul className="wgrid-notes-list">
          {errors.map((i) => (
            <li key={i.id} className="is-error">
              <strong>{fmtDayShort(i.date)}</strong> {fmtHours(i.hours)}: lần ghi trước lỗi: {i.last_error}
            </li>
          ))}
          {warnings.map((w) => (
            <li key={`${w.date}-${w.text}`}>
              <strong>{fmtDayShort(w.date)}</strong> {w.text}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

/**
 * Hours of one task on one day. Click to type a new value; 0 or empty takes
 * it off the queue, and an empty cell of a work row adds a logtime there.
 */
function Cell(props: {
  items: PlanItem[];
  selected: Set<string>;
  disabled: boolean;
  canAdd: boolean;
  editing: boolean;
  onEdit: () => void;
  onCancel: () => void;
  onCommit: (raw: string) => void;
  label: string;
}) {
  const { items, selected, disabled, canAdd, editing, onEdit, onCancel, onCommit, label } = props;
  const hours = items.reduce((s, i) => s + i.hours, 0);

  const off = items.length > 0 && items.every((i) => !selected.has(i.id));
  const error = items.find((i) => i.last_error)?.last_error;
  const cls = `wgrid-cell${items.length ? " has-items" : ""}${off ? " is-off" : ""}${error ? " has-error" : ""}`;

  if (editing) {
    return (
      <td className={`${cls} is-editing`}>
        <HoursInput initial={items.length ? String(hours) : ""} label={label} onCommit={onCommit} onCancel={onCancel} />
      </td>
    );
  }

  // Several logtimes on one task and day are edited one by one in the list.
  const editable = !disabled && (items.length === 1 || (items.length === 0 && canAdd));
  const title = [
    ...items.map((i) => `${fmtHours(i.hours)}${i.comments ? ` · ${i.comments}` : ""}`),
    error ? `Lần ghi trước lỗi: ${error}` : "",
    items.length > 1 ? "Có nhiều logtime, sửa trong chế độ Danh sách." : "",
  ]
    .filter(Boolean)
    .join("\n");

  return (
    <td className={cls}>
      <button
        className="wgrid-btn"
        disabled={!editable}
        onClick={onEdit}
        title={title || (editable ? "Bấm để thêm giờ" : undefined)}
        aria-label={items.length ? `${label}: ${fmtHours(hours)}` : `Thêm giờ: ${label}`}
      >
        {items.length ? fmtHours(hours).slice(0, -1) : ""}
        {items.length > 1 && <sup>×{items.length}</sup>}
      </button>
    </td>
  );
}

/** Mounted per edit, so it starts from the cell's hours, all selected. */
function HoursInput({ initial, label, onCommit, onCancel }: { initial: string; label: string; onCommit: (raw: string) => void; onCancel: () => void }) {
  const [value, setValue] = useState(initial);
  const cancelled = useRef(false);
  return (
    <input
      autoFocus
      inputMode="decimal"
      value={value}
      onChange={(e) => setValue(e.target.value)}
      onFocus={(e) => e.target.select()}
      onBlur={() => (cancelled.current ? onCancel() : onCommit(value))}
      onKeyDown={(e) => {
        if (e.key === "Enter") e.currentTarget.blur();
        else if (e.key === "Escape") {
          cancelled.current = true;
          e.currentTarget.blur();
        }
      }}
      aria-label={`Số giờ: ${label}`}
    />
  );
}

function queueRows(items: PlanItem[], subtasks: SubtaskDraft[]): QueueRow[] {
  const rows = new Map<string, QueueRow>();
  for (const i of [...items].sort((a, b) => a.date.localeCompare(b.date))) {
    const key = i.target.type === "issue" ? `issue:${i.target.issue_id}:${i.kind}` : `sub:${i.target.key}`;
    let row = rows.get(key);
    if (!row) {
      const target = i.target;
      const subtask = target.type === "new_subtask" ? subtasks.find((s) => s.key === target.key) : undefined;
      const label = target.type === "issue" ? target.label : (subtask?.subject ?? "Task con mới");
      row = { key, label, kind: i.kind, subtask, items: [] };
      rows.set(key, row);
    }
    row.items.push(i);
  }
  // Work first, leave last; each in order of first day.
  return [...rows.values()].sort((a, b) => Number(a.kind === "leave") - Number(b.kind === "leave"));
}

function existingRowsOf(week: string[], existingOf: (d: string) => TimeEntry[] | null, settings: Settings): ExistingRow[] {
  const rows = new Map<string, ExistingRow & { notes: Map<string, number> }>();
  for (const date of week) {
    for (const e of existingOf(date) ?? []) {
      const key = e.issue ? `i${e.issue.id}` : `p${e.project.id}`;
      let row = rows.get(key);
      if (!row) {
        const leave = settings.leave_types.find((t) => t.project_id === e.project.id);
        const label = e.issue ? `#${e.issue.id}` : e.project.name;
        row = { key, label: leave ? `${label} · ${leave.label}` : label, hint: "", byDate: new Map(), notes: new Map() };
        rows.set(key, row);
      }
      row.byDate.set(date, [...(row.byDate.get(date) ?? []), e]);
      const note = (e.comments ?? "").trim();
      if (note) row.notes.set(note, (row.notes.get(note) ?? 0) + e.hours);
    }
  }
  return [...rows.values()].map(({ notes, ...row }) => {
    // The comment with the most hours says best what the issue is.
    const [top] = [...notes.entries()].sort((a, b) => b[1] - a[1]);
    const project = [...row.byDate.values()][0]?.[0]?.project.name ?? "";
    return { ...row, hint: [top?.[0], project].filter(Boolean).join(" · ") };
  });
}

/** Monday-first weeks (7 ISO dates each) that contain any of `dates`. */
function weeksOf(dates: string[]): string[][] {
  const mondays = new Set<string>();
  for (const iso of dates) {
    const d = parseIso(iso);
    d.setDate(d.getDate() - ((d.getDay() + 6) % 7));
    mondays.add(isoOf(d));
  }
  return [...mondays].sort().map((m) => {
    const start = parseIso(m);
    return Array.from({ length: 7 }, (_, k) => {
      const d = new Date(start);
      d.setDate(start.getDate() + k);
      return isoOf(d);
    });
  });
}

function isWorkday(iso: string, settings: Settings): boolean {
  const wd = ((parseIso(iso).getDay() + 6) % 7) + 1;
  return settings.workdays.includes(wd) && !settings.holidays.includes(iso);
}

function dayMark(d: { state: DayState; total: number }, quota: number): string {
  if (d.state === "ok") return "✓";
  if (d.state === "short") return `−${fmtHours(quota - d.total).slice(0, -1)}`;
  if (d.state === "over") return `+${fmtHours(d.total - quota).slice(0, -1)}`;
  return "";
}

function dayTitle(d: { state: DayState; total: number; existingHours: number; planned: number }, quota: number): string {
  const parts = [`Đã có ${fmtHours(d.existingHours)}, chờ ghi ${fmtHours(d.planned)}`];
  if (d.state === "short") parts.push(`thiếu ${fmtHours(quota - d.total)}`);
  if (d.state === "over") parts.push(`vượt ${fmtHours(d.total - quota)}`);
  if (d.state === "future") parts.push("ngày chưa tới");
  if (d.state === "off") parts.push("ngày nghỉ");
  return parts.join(", ");
}
