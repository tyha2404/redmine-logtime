import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  api,
  errorText,
  issueBrief,
  newId,
  type Batch,
  type DayPlan,
  type IssueBrief,
  type PlanItem,
  type Profile,
  type ReportView,
  type Settings,
  type SubtaskDraft,
  type TaskView,
  type WritePlan,
} from "./api";
import { fmtDayLong, fmtDayShort, fmtHours } from "./fmt";
import IssuePicker from "./IssuePicker";
import { buildLeave, type ParentCache } from "./leave";
import QuotaRail from "./QuotaRail";
import { useToast } from "./toast";
import { useAiRun } from "./useAiRun";
import WriteDialog from "./WriteDialog";

const EPS = 1e-6;

/** What the user decided for one task of the report. */
export interface Choice {
  parent: IssueBrief | null;
  /** "new" creates a sub-task named `subject`; a number logs on that existing sub-task. */
  subtask: "new" | number;
  subject: string;
  /** Idempotency key of the new sub-task, renewed when its parent or name changes. */
  subKey: string;
}

export interface DayHours {
  id: string;
  date: string;
  hours: number;
  /** Ticked to be written. */
  on: boolean;
  /** Already on Redmine from an earlier write of this report. */
  written: boolean;
}

const itemId = (task: string, date: string) => `${task}|${date}`;
const leaveId = (date: string) => `leave|${date}`;

/**
 * Report text → Claude analysis → per-task choices (parent, sub-task, hours)
 * → one confirmed write to Redmine. Shared by the "Ghi logtime" page (one or
 * many days, `day` = null) and the calendar's day panel (`day` = that date).
 */
export function useReportFlow(day: string | null, settings: Settings) {
  const ai = useAiRun();
  const [text, setText] = useState("");
  const [view, setView] = useState<ReportView | null>(null);
  const [tasks, setTasks] = useState<TaskView[]>([]);
  const [choices, setChoices] = useState<Record<string, Choice>>({});
  const [plans, setPlans] = useState<DayPlan[]>([]);
  const [profile, setProfile] = useState<Profile | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  /** Hours the user typed, by item id; the planner's split otherwise. */
  const [edits, setEdits] = useState<Record<string, number>>({});
  /** Hours written to Redmine from this analysis, by item id. */
  const [written, setWritten] = useState<Record<string, number>>({});
  /** The user's sub-tasks under each parent, to log on one that exists. */
  const [children, setChildren] = useState<Record<number, IssueBrief[]>>({});
  const asked = useRef(new Set<number>());
  /** Unticked entries (task and date, or a day off), by item id. */
  const [off, setOff] = useState<Set<string>>(new Set());

  const adopt = useCallback((v: ReportView | null) => {
    setView(v);
    setTasks(v?.tasks ?? []);
    setPlans([]);
    setEdits({});
    setWritten({});
    setOff(new Set());
    setChoices(
      Object.fromEntries(
        (v?.tasks ?? []).map((t) => [
          t.task.key,
          {
            parent: t.parent,
            subtask: t.existing_subtask?.id ?? "new",
            subject: t.task.subtask_subject || t.task.text,
            subKey: newId(),
          },
        ]),
      ),
    );
  }, []);

  // Restore what was last typed and analysed for this slot.
  useEffect(() => {
    let live = true;
    api.getLastReport(day).then((r) => {
      if (!live) return;
      setText(r.text);
      adopt(r.view);
    });
    api.getProfile().then((p) => live && setProfile(p));
    return () => {
      live = false;
    };
  }, [day, adopt]);

  // The planner splits each day among the ticked entries only.
  const planView = useMemo(
    () =>
      view && {
        ...view,
        analysis: {
          ...view.analysis,
          tasks: view.analysis.tasks.map((t) => ({ ...t, days: t.days.filter((d) => !off.has(itemId(t.key, d.date))) })),
        },
      },
    [view, off],
  );

  // Re-split the hours whenever the selection changes. Until the new split
  // arrives the old one is stale, so writing waits for it.
  const [planning, setPlanning] = useState(false);
  useEffect(() => {
    if (!planView) return;
    let live = true;
    setPlanning(true);
    api
      .planReport(planView, [])
      .then((p) => live && setPlans(p))
      .catch((e) => live && setNotice(errorText(e)))
      .finally(() => live && setPlanning(false));
    return () => {
      live = false;
    };
  }, [planView]);

  // Sub-tasks of every chosen parent, fetched once each.
  useEffect(() => {
    for (const c of Object.values(choices)) {
      const id = c.parent?.id;
      if (!id || asked.current.has(id)) continue;
      asked.current.add(id);
      api
        .listChildren(id)
        .then((list) => setChildren((m) => ({ ...m, [id]: list.map(issueBrief) })))
        .catch(() => asked.current.delete(id));
    }
  }, [choices]);

  const analyze = async () => {
    setNotice(null);
    const v = await ai.run(() => api.analyzeReport(text, day));
    if (v) adopt(v);
  };

  const setChoice = (key: string, patch: Partial<Choice>) =>
    setChoices((c) => ({ ...c, [key]: { ...c[key], ...patch } }));

  const setOn = (ids: string[], on: boolean) =>
    setOff((prev) => {
      const next = new Set(prev);
      ids.forEach((id) => (on ? next.delete(id) : next.add(id)));
      return next;
    });

  const changeParent = (key: string, parent: IssueBrief) => {
    setChoice(key, { parent, subtask: "new", subKey: newId() });
    setTasks((ts) =>
      ts.map((t) => (t.task.key === key ? { ...t, problems: t.problems.filter((p) => !p.includes("task cha")) } : t)),
    );
  };

  const changeSubject = (key: string, subject: string) => setChoice(key, { subject, subKey: newId() });

  const setHours = (id: string, hours: number) => setEdits((e) => ({ ...e, [id]: hours }));

  /** Options for "Task con": the user's sub-tasks of the parent, plus the one Claude found. */
  const subtasksOf = (t: TaskView): IssueBrief[] => {
    const c = choices[t.task.key];
    const list = (c?.parent && children[c.parent.id]) || [];
    const found = t.existing_subtask;
    return found && found.parent_id === c?.parent?.id && !list.some((x) => x.id === found.id) ? [found, ...list] : list;
  };

  const planned = useMemo(() => new Set(view?.days.map((d) => d.date)), [view]);
  const allocations = plans.flatMap((p) => p.allocations);

  /** Hours of a task per reported day: what the user typed, else the planner's split. */
  const daysOf = (t: TaskView): DayHours[] =>
    t.task.days
      .filter((d) => planned.has(d.date))
      .map((d) => {
        const id = itemId(t.task.key, d.date);
        const split = allocations.find((a) => a.task === t.task.key && a.date === d.date)?.hours ?? 0;
        const on = !off.has(id);
        return { id, date: d.date, hours: written[id] ?? (on ? (edits[id] ?? split) : 0), on, written: id in written };
      });

  /** Work still to write: ticked entries with hours, minus what is on Redmine already. */
  const pending = tasks.flatMap((t) => daysOf(t).filter((d) => d.on && !d.written && d.hours > EPS).map((d) => ({ t, d })));
  const leave = (view?.analysis.leave ?? []).filter((l) => planned.has(l.date));
  const leaveOn = (date: string) => !off.has(leaveId(date));
  const pendingLeave = leave.filter((l) => leaveOn(l.date) && !(leaveId(l.date) in written));
  /** Every entry that can be ticked, for "Chọn tất cả". */
  const allIds = [
    ...tasks.flatMap((t) => daysOf(t).filter((d) => !d.written).map((d) => d.id)),
    ...leave.filter((l) => !(leaveId(l.date) in written)).map((l) => leaveId(l.date)),
  ];
  const totalHours = pending.reduce((s, x) => s + x.d.hours, 0) + pendingLeave.reduce((s, l) => s + l.hours, 0);
  const count = pending.length + pendingLeave.length;

  const trackerId = settings.subtask_tracker_id ?? profile?.subtask_tracker_id ?? null;
  const tasksToWrite = [...new Set(pending.map((x) => x.t))];
  const missingParent = tasksToWrite.filter((t) => !choices[t.task.key].parent).length;
  const needsTracker = tasksToWrite.some((t) => choices[t.task.key].subtask === "new");
  const badHours = pending.some((x) => !(x.d.hours <= 24));
  const blankSubject = tasksToWrite.some((t) => choices[t.task.key].subtask === "new" && !choices[t.task.key].subject.trim());
  const duplicateCount = tasks.filter((t) => t.task.days.length === 0 && t.duplicates.length > 0).length;
  const blocker =
    missingParent > 0
      ? `${missingParent} việc chưa có task cha.`
      : blankSubject
        ? "Nhập tên cho task con mới."
        : needsTracker && !trackerId
          ? "Chưa chọn tracker cho task con trong Cài đặt."
          : badHours
            ? "Số giờ mỗi ngày phải từ 0 đến 24."
            : pendingLeave.length > 0 && settings.leave_types.length === 0
              ? "Báo cáo có ngày nghỉ nhưng chưa cài nghỉ phép trong Cài đặt (mục Nghỉ phép, “Tìm trên Redmine”)."
              : count === 0
                ? "Không có giờ nào để ghi."
                : null;

  /** The entries and new sub-tasks to send, for the confirmation dialog. */
  const buildPlan = async (): Promise<WritePlan | null> => {
    if (!view || blocker || planning) return null;
    setNotice(null);
    const items: PlanItem[] = [];
    const subtasks: SubtaskDraft[] = [];
    for (const t of tasksToWrite) {
      const c = choices[t.task.key];
      const parent = c.parent!;
      let target: PlanItem["target"];
      if (c.subtask === "new") {
        subtasks.push({
          key: c.subKey,
          parent_id: parent.id,
          parent_subject: parent.subject,
          project_id: parent.project_id,
          project_name: parent.project_name,
          tracker_id: trackerId!,
          subject: c.subject.trim(),
        });
        target = { type: "new_subtask", key: c.subKey };
      } else {
        const sub = subtasksOf(t).find((x) => x.id === c.subtask);
        target = { type: "issue", issue_id: c.subtask, label: `#${c.subtask}${sub ? ` ${sub.subject}` : ""}` };
      }
      for (const { d } of pending.filter((x) => x.t === t)) {
        items.push({
          id: d.id,
          date: d.date,
          target,
          hours: d.hours,
          activity_id: settings.default_activity_id,
          comments: t.task.comment || t.task.text,
          kind: "work",
        });
      }
    }
    // Each day off becomes its own issue under HR's parent for that month.
    try {
      const cache: ParentCache = new Map();
      for (const l of pendingLeave) {
        const type = settings.leave_types.find((x) => x.key === l.kind) ?? settings.leave_types[0];
        const span = l.hours + EPS >= settings.hours_per_day ? "full" : "half";
        const built = await buildLeave({ settings, type, date: l.date, span, cache });
        items.push({ ...built.item, id: leaveId(l.date), hours: l.hours });
        subtasks.push(built.subtask);
      }
    } catch (e) {
      setNotice(errorText(e));
      return null;
    }
    return { items, subtasks };
  };

  /**
   * Records what a write put on Redmine. The saved analysis is dropped so a
   * restart cannot write it twice; once nothing is left, the result closes.
   * Returns how many entries were written.
   */
  const afterWrite = async (batch: Batch): Promise<number> => {
    const done = batch.ops.filter((o) => o.payload.op === "create_time_entry" && o.status === "done");
    if (done.length === 0) return 0;
    const now = { ...written };
    for (const o of done) if (o.payload.op === "create_time_entry") now[o.payload.id] = o.payload.hours;
    setWritten(now);
    await api.clearLastReport(day).catch(() => {});
    const left = pending.some((x) => !(x.d.id in now)) || pendingLeave.some((l) => !(leaveId(l.date) in now));
    if (!left) adopt(null);
    return done.length;
  };

  /** Drops the analysis; the text stays for reference. */
  const finish = async () => {
    adopt(null);
    await api.clearLastReport(day).catch(() => {});
  };

  return {
    ai,
    text,
    setText,
    view,
    tasks,
    choices,
    setChoice,
    setOn,
    allIds,
    off,
    leaveOn,
    leaveId,
    changeParent,
    changeSubject,
    setHours,
    subtasksOf,
    daysOf,
    plans,
    planning,
    written,
    profile,
    notice,
    analyze,
    duplicateCount,
    leave,
    totalHours,
    count,
    blocker,
    buildPlan,
    afterWrite,
    finish,
  };
}

export type ReportFlow = ReturnType<typeof useReportFlow>;

/** One-line summary of an analysed report. */
export function summaryLine(f: ReportFlow): string {
  const v = f.view!;
  const scope = v.day
    ? ""
    : v.analysis.period_from === v.analysis.period_to
      ? `Báo cáo ngày ${fmtDayShort(v.analysis.period_from)}: `
      : `Báo cáo từ ${fmtDayShort(v.analysis.period_from)} đến ${fmtDayShort(v.analysis.period_to)}: `;
  const parts = [
    f.count > 0 ? `${scope}${f.count} logtime, ${fmtHours(f.totalHours)} sẽ được ghi` : `${scope}chưa chọn gì để ghi`,
  ];
  if (f.leave.length) parts.push(`${f.leave.length} ngày nghỉ phép`);
  if (f.duplicateCount) parts.push(`${f.duplicateCount} việc trùng đã bỏ qua`);
  let s = parts.join(", ") + ".";
  if (v.skipped_future.length) s += ` Bỏ qua ${v.skipped_future.map(fmtDayShort).join(", ")} vì chưa tới.`;
  return s.charAt(0).toUpperCase() + s.slice(1);
}

/**
 * The analysed report grouped by day: tick entries, fix hours, and change a
 * task's parent and sub-task (for all its days); then write to Redmine after
 * a confirmation dialog.
 */
export function ReportResult({
  f,
  settings,
  compact = false,
  onWritten,
  onOpenHistory,
}: {
  f: ReportFlow;
  settings: Settings;
  compact?: boolean;
  onWritten?: () => void;
  onOpenHistory?: () => void;
}) {
  const toast = useToast();
  const [plan, setPlan] = useState<WritePlan | null>(null);
  const [preparing, setPreparing] = useState(false);
  /** The entry whose task is being changed, shown under that entry. */
  const [editing, setEditing] = useState<string | null>(null);
  const view = f.view!;
  const allOn = f.allIds.every((id) => !f.off.has(id));
  const skipped = f.tasks.filter((t) => t.task.days.length === 0);
  const days = view.days.filter(
    (d) => d.workday || d.existing_hours > 0 || f.leave.some((l) => l.date === d.date) || f.tasks.some((t) => t.task.days.some((x) => x.date === d.date)),
  );

  const open = async () => {
    setPreparing(true);
    try {
      setPlan(await f.buildPlan());
    } finally {
      setPreparing(false);
    }
  };

  const closed = async (batch: Batch | null) => {
    setPlan(null);
    if (!batch) return;
    const n = await f.afterWrite(batch);
    if (n > 0) {
      onWritten?.();
      toast(`Đã ghi ${n} logtime lên Redmine.`);
    }
  };

  return (
    <div className={`composer-result${compact ? " is-compact" : ""}`}>
      <p className="composer-sum">{summaryLine(f)}</p>

      {view.analysis.questions.length > 0 && (
        <div className="ai-questions">
          <p>Claude cần anh/chị kiểm tra (sửa báo cáo rồi phân tích lại nếu cần):</p>
          <ul>
            {view.analysis.questions.map((q) => (
              <li key={q}>{q}</li>
            ))}
          </ul>
        </div>
      )}

      <div className="rtasks-head">
        <h3 className="composer-sub">{days.length > 1 ? `Theo ngày (${days.length} ngày)` : "Việc trong báo cáo"}</h3>
        {f.allIds.length > 1 && (
          <button className="link" onClick={() => f.setOn(f.allIds, !allOn)} disabled={f.ai.running}>
            {allOn ? "Bỏ chọn tất cả" : "Chọn tất cả"}
          </button>
        )}
      </div>

      <div className="rdays">
        {days.map((day) => (
          <DayGroup key={day.date} f={f} day={day} settings={settings} editing={editing} onEdit={setEditing} />
        ))}
      </div>

      {skipped.length > 0 && (
        <section className="rskipped">
          <h3 className="composer-sub">Đã log trước đó, không ghi lại</h3>
          <ul>
            {skipped.map((t) => (
              <li key={t.task.key}>
                <strong>{t.task.text}</strong>
                {t.duplicates.map((d) => (
                  <span key={d.date} className="muted">
                    {" "}
                    · {fmtDayShort(d.date)}: {d.reason}
                  </span>
                ))}
              </li>
            ))}
          </ul>
        </section>
      )}

      <div className="composer-actions">
        {(f.blocker || f.notice) && <p className="notice error">{f.notice ?? f.blocker}</p>}
        <button className="btn primary" disabled={!!f.blocker || f.ai.running || f.planning || preparing} onClick={open}>
          {preparing
            ? "Đang chuẩn bị…"
            : f.planning
              ? "Đang chia giờ…"
              : `Ghi ${f.count} logtime (${fmtHours(f.totalHours)}) lên Redmine`}
        </button>
      </div>

      {plan && <WriteDialog plan={plan} redmineUrl={settings.redmine_url} onClose={closed} onOpenHistory={onOpenHistory} />}
    </div>
  );
}

/** One reported day: its total against the quota, then each entry to write. */
function DayGroup({
  f,
  day,
  settings,
  editing,
  onEdit,
}: {
  f: ReportFlow;
  day: ReportView["days"][number];
  settings: Settings;
  editing: string | null;
  onEdit: (id: string | null) => void;
}) {
  const hpd = settings.hours_per_day;
  const rows = f.tasks.flatMap((t) => f.daysOf(t).filter((d) => d.date === day.date).map((d) => ({ t, d })));
  const leave = f.leave.find((l) => l.date === day.date);
  const leaveWritten = !!leave && f.leaveId(day.date) in f.written;
  const leaveOn = !!leave && f.leaveOn(day.date);
  const writtenHours = rows.filter((r) => r.d.written).reduce((s, r) => s + r.d.hours, 0) + (leaveWritten ? leave!.hours : 0);
  const plannedHours =
    rows.filter((r) => r.d.on && !r.d.written).reduce((s, r) => s + r.d.hours, 0) + (leave && leaveOn && !leaveWritten ? leave.hours : 0);
  const existing = day.existing_hours + writtenHours;
  const total = existing + plannedHours;
  const state = total > hpd + EPS ? "over" : total + EPS >= hpd ? "full" : day.workday ? "short" : "off";
  const warnings: string[] = [];
  if (!day.workday && plannedHours > EPS) warnings.push("Ngày nghỉ hoặc ngày lễ.");
  if (total > hpd + EPS) warnings.push(`Vượt ${fmtHours(total - hpd)} so với ${fmtHours(hpd)}.`);
  else if (day.workday && total + EPS < hpd) warnings.push(`Còn thiếu ${fmtHours(hpd - total)} để đủ ${fmtHours(hpd)}.`);
  const dups = f.tasks.filter((t) => t.task.days.length > 0).flatMap((t) => t.duplicates.filter((d) => d.date === day.date).map((d) => ({ t, d })));
  const leaveType = leave && (settings.leave_types.find((x) => x.key === leave.kind) ?? settings.leave_types[0]);

  return (
    <section className="rday-group" aria-label={fmtDayLong(day.date)}>
      <header className="rday-head">
        <h4 className="rday-title">{fmtDayLong(day.date)}</h4>
        <span className={`rday-total is-${state}`}>
          {fmtHours(total)} / {fmtHours(hpd)}
        </span>
        <QuotaRail existing={existing} planned={plannedHours} quota={hpd} />
      </header>
      <ul className="rentries">
        {day.existing_hours > 0 && (
          <li className="rentry is-static">
            <span />
            <span className="rentry-text muted">Đã log trước đó trên Redmine</span>
            <span className="rentry-hours muted">{fmtHours(day.existing_hours)}</span>
          </li>
        )}
        {leave && (
          <li className={`rentry is-leave${leaveOn || leaveWritten ? "" : " is-off"}${leaveWritten ? " is-written" : ""}`}>
            <input
              type="checkbox"
              checked={leaveOn || leaveWritten}
              disabled={leaveWritten || f.ai.running}
              onChange={(e) => f.setOn([f.leaveId(day.date)], e.target.checked)}
              aria-label={`Ghi nghỉ phép ngày ${fmtDayShort(day.date)}`}
            />
            <span className="rentry-text">{leaveType?.label ?? "Nghỉ phép"}</span>
            <span className="rentry-hours">{leaveWritten ? `✓ ${fmtHours(leave.hours)}` : fmtHours(leave.hours)}</span>
          </li>
        )}
        {rows.map(({ t, d }) => (
          <EntryRow key={d.id} f={f} view={t} day={d} editing={editing === d.id} onEdit={(on) => onEdit(on ? d.id : null)} />
        ))}
        {rows.length === 0 && !leave && day.workday && <li className="rentry is-static muted">Báo cáo không có việc nào cho ngày này.</li>}
      </ul>
      {(warnings.length > 0 || dups.length > 0) && (
        <div className="rday-notes">
          {warnings.map((w) => (
            <p key={w} className="rwarn-line">
              {w}
            </p>
          ))}
          {dups.map(({ t, d }) => (
            <p key={t.task.key} className="muted small">
              Bỏ qua “{t.task.text}”: {d.reason}
            </p>
          ))}
        </div>
      )}
    </section>
  );
}

/** One task on one day: tick, hours, and where it is logged (shared by all its days). */
function EntryRow({
  f,
  view,
  day,
  editing,
  onEdit,
}: {
  f: ReportFlow;
  view: TaskView;
  day: DayHours;
  editing: boolean;
  onEdit: (on: boolean) => void;
}) {
  const c = f.choices[view.task.key];
  if (!c) return null;
  const t = view.task;
  const span = f.daysOf(view).length;
  const sub = c.subtask === "new" ? null : f.subtasksOf(view).find((x) => x.id === c.subtask);
  const conf = Math.round(t.parent_confidence * 100);
  const fromAi = !!c.parent && t.parent_issue_id === c.parent.id;
  const confClass = conf >= 80 ? "hi" : conf >= 60 ? "mid" : "lo";
  const label = fmtDayShort(day.date);

  return (
    <li className={`rentry${day.on || day.written ? "" : " is-off"}${day.written ? " is-written" : ""}${view.problems.length ? " is-unsure" : ""}`}>
      <input
        type="checkbox"
        checked={day.on || day.written}
        disabled={day.written || f.ai.running}
        onChange={(e) => f.setOn([day.id], e.target.checked)}
        aria-label={`Ghi “${t.text}” ngày ${label}`}
      />
      <div className="rentry-main">
        <div className="rentry-text">
          {t.text}
          {span > 1 && <span className="rentry-span">{span} ngày</span>}
        </div>
        <button
          type="button"
          className={`rentry-target${c.parent ? "" : " is-missing"}`}
          onClick={() => onEdit(!editing)}
          disabled={day.written || f.ai.running}
          aria-expanded={editing}
          aria-label={`Đổi task cho “${t.text}”`}
        >
          {c.parent ? (
            <>
              <span className="rentry-id">#{c.parent.id}</span>
              <span className="rentry-name">{c.parent.subject}</span>
            </>
          ) : (
            <span>Chưa có task cha, bấm để chọn</span>
          )}
          <span className="rentry-arrow" aria-hidden>
            ›
          </span>
          {c.subtask === "new" ? (
            <span className="rentry-name">
              <span className="rentry-new">Mới</span> {c.subject || "chưa đặt tên"}
            </span>
          ) : (
            <span className="rentry-name">
              <span className="rentry-id">#{c.subtask}</span> {sub?.subject ?? ""}
            </span>
          )}
          {fromAi && <span className={`conf conf-${confClass}`}>{conf}%</span>}
        </button>
        {view.problems.length > 0 && <p className="rentry-problem">{view.problems.join(" ")}</p>}
      </div>
      {day.written ? (
        <span className="rentry-hours is-done">✓ {fmtHours(day.hours)}</span>
      ) : (
        <HoursInput hours={day.hours} disabled={!day.on || f.ai.running} label={`Số giờ “${t.text}” ngày ${label}`} onChange={(h) => f.setHours(day.id, h)} />
      )}
      {editing && (
        <div className="rentry-editor">
          <TaskEditor f={f} view={view} span={span} />
          <button className="btn small" onClick={() => onEdit(false)}>
            Xong
          </button>
        </div>
      )}
    </li>
  );
}

/** Parent and sub-task of a task; the change applies to every day of it. */
function TaskEditor({ f, view, span }: { f: ReportFlow; view: TaskView; span: number }) {
  const c = f.choices[view.task.key];
  const t = view.task;
  const key = t.key;
  const fromAi = !!c.parent && t.parent_issue_id === c.parent.id;
  return (
    <>
      {span > 1 && <p className="rentry-note">Áp dụng cho “{t.text}” ở cả {span} ngày.</p>}
      <dl className="rtask-grid">
        <dt>Task cha</dt>
        <dd>
          <IssuePicker value={c.parent} onPick={(p) => f.changeParent(key, p)} label={`Task cha của “${t.text}”`} />
          {t.parent_reason && fromAi && <p className="rtask-reason">{t.parent_reason}</p>}
        </dd>
        <dt>Task con</dt>
        <dd>
          <div className="rtask-sub">
            <select
              className="rtask-select"
              value={String(c.subtask)}
              onChange={(e) => f.setChoice(key, { subtask: e.target.value === "new" ? "new" : Number(e.target.value) })}
              disabled={!c.parent}
              aria-label={`Task con của “${t.text}”`}
            >
              <option value="new">Tạo task con mới</option>
              {f.subtasksOf(view).map((o) => (
                <option key={o.id} value={o.id}>
                  #{o.id} {o.subject}
                  {o.status ? ` (${o.status})` : ""}
                </option>
              ))}
            </select>
            {c.subtask === "new" && (
              <input
                className="subject-input"
                value={c.subject}
                onChange={(e) => f.changeSubject(key, e.target.value)}
                placeholder="Tên task con mới"
                aria-label={`Tên task con mới cho “${t.text}”`}
              />
            )}
          </div>
        </dd>
      </dl>
    </>
  );
}

/** Hours of one entry; 0 leaves it out. */
function HoursInput({ hours, disabled, label, onChange }: { hours: number; disabled: boolean; label: string; onChange: (h: number) => void }) {
  const [raw, setRaw] = useState(String(hours));
  useEffect(() => setRaw(String(hours)), [hours]);
  const commit = () => {
    const h = Number(raw.replace(",", "."));
    if (Number.isFinite(h) && h >= 0) onChange(Math.round(h * 4) / 4);
    else setRaw(String(hours));
  };
  return (
    <label className={`rentry-hours hinput${hours <= EPS ? " is-zero" : ""}`}>
      <input
        type="text"
        inputMode="decimal"
        value={disabled && hours <= EPS ? "" : raw}
        disabled={disabled}
        onChange={(e) => setRaw(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
        aria-label={label}
      />
      <span>h</span>
    </label>
  );
}
