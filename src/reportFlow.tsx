import { useCallback, useEffect, useMemo, useState } from "react";
import {
  api,
  errorText,
  issueBrief,
  newId,
  type DayPlan,
  type PlanItem,
  type Profile,
  type ReportView,
  type Settings,
  type SubtaskDraft,
  type TaskView,
} from "./api";
import { fmtDayShort, fmtHours } from "./fmt";
import { buildLeave, type ParentCache } from "./leave";
import { useAiRun } from "./useAiRun";
import type { DraftApi } from "./useDraft";

/** Per-task choices the user can change before queueing. */
export interface Choice {
  include: boolean;
  /** Log on the existing sub-task instead of creating a new one. */
  useExisting: boolean;
  subject: string;
}

/**
 * Report text → Claude analysis → per-task choices → hour plan → queue.
 * Shared by the report page (one or many days, `day` = null) and the
 * calendar's day panel (`day` = that date). Each keeps its own saved report.
 */
export function useReportFlow(day: string | null, settings: Settings, draft: DraftApi) {
  const ai = useAiRun();
  const [text, setText] = useState("");
  const [view, setView] = useState<ReportView | null>(null);
  const [tasks, setTasks] = useState<TaskView[]>([]);
  const [choices, setChoices] = useState<Record<string, Choice>>({});
  const [plans, setPlans] = useState<DayPlan[]>([]);
  const [profile, setProfile] = useState<Profile | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const adopt = useCallback((v: ReportView | null) => {
    setView(v);
    setTasks(v?.tasks ?? []);
    setPlans([]);
    setChoices(
      Object.fromEntries(
        (v?.tasks ?? []).map((t) => [
          t.task.key,
          {
            // Work already logged or queued on every reported day is skipped.
            include: t.task.days.length > 0,
            useExisting: !!t.existing_subtask,
            subject: t.task.subtask_subject || t.task.text,
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

  const excluded = useMemo(
    () => Object.entries(choices).filter(([, c]) => !c.include).map(([k]) => k),
    [choices],
  );

  useEffect(() => {
    if (!view) return;
    api.planReport(view, excluded).then(setPlans).catch((e) => setNotice(errorText(e)));
  }, [view, excluded]);

  const analyze = async () => {
    setNotice(null);
    const v = await ai.run(() => api.analyzeReport(text, day));
    if (v) adopt(v);
  };

  const setChoice = (key: string, patch: Partial<Choice>) =>
    setChoices((c) => ({ ...c, [key]: { ...c[key], ...patch } }));

  const changeParent = async (key: string, raw: string) => {
    const id = Number(raw.replace(/^#/, ""));
    if (!Number.isInteger(id) || id <= 0) return;
    try {
      const issue = await api.lookupIssue(id);
      setTasks((ts) =>
        ts.map((t) =>
          t.task.key === key
            ? { ...t, parent: issueBrief(issue), existing_subtask: null, problems: t.problems.filter((p) => !p.includes("task cha")) }
            : t,
        ),
      );
      setChoice(key, { useExisting: false });
    } catch (e) {
      setNotice(errorText(e));
    }
  };

  const hoursOf = (key: string) =>
    plans.flatMap((p) => p.allocations).filter((a) => a.task === key).reduce((s, a) => s + a.hours, 0);

  const included = tasks.filter((t) => choices[t.task.key]?.include);
  const duplicateCount = tasks.filter((t) => t.task.days.length === 0 && t.duplicates.length > 0).length;
  const trackerId = settings.subtask_tracker_id ?? profile?.subtask_tracker_id ?? null;
  const needsTracker = included.some((t) => !(choices[t.task.key]?.useExisting && t.existing_subtask));
  const plannedDays = new Set(view?.days.map((d) => d.date));
  const leave = (view?.analysis.leave ?? []).filter((l) => plannedDays.has(l.date));
  const totalHours = plans.reduce((s, p) => s + p.allocations.reduce((x, a) => x + a.hours, 0), 0);
  const missingParent = included.filter((t) => !t.parent).length;
  const blocker =
    missingParent > 0
      ? `${missingParent} việc chưa có task cha.`
      : needsTracker && !trackerId
        ? "Chưa chọn tracker cho task con trong Cài đặt."
        : leave.length > 0 && settings.leave_types.length === 0
          ? "Báo cáo có ngày nghỉ nhưng chưa cài nghỉ phép trong Cài đặt (mục Nghỉ phép, “Tìm trên Redmine”)."
          : totalHours === 0 && leave.length === 0
            ? "Không có giờ nào để log."
            : null;

  /** Adds the plan to the review queue; returns how many entries were added. */
  const queue = async (): Promise<number> => {
    if (!view || blocker) return 0;
    const items: PlanItem[] = [];
    const subtasks: SubtaskDraft[] = [];
    for (const t of included) {
      const c = choices[t.task.key];
      const parent = t.parent!;
      let target: PlanItem["target"];
      if (c.useExisting && t.existing_subtask) {
        target = { type: "issue", issue_id: t.existing_subtask.id, label: `#${t.existing_subtask.id} ${t.existing_subtask.subject}` };
      } else {
        const key = newId();
        subtasks.push({
          key,
          parent_id: parent.id,
          parent_subject: parent.subject,
          project_id: parent.project_id,
          project_name: parent.project_name,
          tracker_id: trackerId!,
          subject: c.subject.trim() || t.task.text,
        });
        target = { type: "new_subtask", key };
      }
      for (const a of plans.flatMap((p) => p.allocations).filter((x) => x.task === t.task.key)) {
        items.push({
          id: newId(),
          date: a.date,
          target,
          hours: a.hours,
          activity_id: settings.default_activity_id,
          comments: t.task.comment || t.task.text,
          kind: "work",
          last_error: null,
        });
      }
    }
    // Each day off becomes its own issue under HR's parent for that month.
    try {
      const cache: ParentCache = new Map();
      for (const l of leave) {
        const type = settings.leave_types.find((t) => t.key === l.kind) ?? settings.leave_types[0];
        const span = l.hours + 1e-6 >= settings.hours_per_day ? "full" : "half";
        const built = await buildLeave({ settings, type, date: l.date, span, cache });
        items.push({ ...built.item, hours: l.hours });
        subtasks.push(built.subtask);
      }
    } catch (e) {
      setNotice(errorText(e));
      return 0;
    }
    await draft.add(items, subtasks);
    return items.length;
  };

  /** Drops the analysis after its plan is queued; the text stays for reference. */
  const finish = async () => {
    adopt(null);
    await api.clearLastReport(day).catch(() => {});
  };

  return {
    ai,
    finish,
    text,
    setText,
    view,
    tasks,
    choices,
    setChoice,
    plans,
    profile,
    notice,
    analyze,
    changeParent,
    hoursOf,
    duplicateCount,
    leave,
    totalHours,
    blocker,
    queue,
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
  const parts = [`${scope}${f.tasks.length} việc, ${fmtHours(f.totalHours)} sẽ được log`];
  if (f.leave.length) parts.push(`${f.leave.length} ngày nghỉ phép`);
  if (f.duplicateCount) parts.push(`${f.duplicateCount} việc trùng đã bỏ qua`);
  let s = parts.join(", ") + ".";
  if (v.skipped_future.length) s += ` Bỏ qua ${v.skipped_future.map(fmtDayShort).join(", ")} vì chưa tới.`;
  return s;
}

export function TaskRow(props: {
  view: TaskView;
  choice: Choice | undefined;
  hours: number;
  onChoice: (patch: Partial<Choice>) => void;
  onParent: (raw: string) => void;
}) {
  const { view, choice, hours, onChoice, onParent } = props;
  const [parentRaw, setParentRaw] = useState(view.parent ? `#${view.parent.id}` : "");
  useEffect(() => setParentRaw(view.parent ? `#${view.parent.id}` : ""), [view.parent]);
  if (!choice) return null;
  const t = view.task;
  const conf = Math.round(t.parent_confidence * 100);
  const confClass = conf >= 80 ? "hi" : conf >= 60 ? "mid" : "lo";

  return (
    <li className={`rtask${choice.include ? "" : " is-off"}${view.problems.length ? " is-unsure" : ""}`}>
      <input
        type="checkbox"
        checked={choice.include}
        onChange={(e) => onChoice({ include: e.target.checked })}
        aria-label={`Log việc ${t.text}`}
      />
      <div className="rtask-body">
        <div className="rtask-title">
          <strong>{t.text}</strong>
          <span className="rtask-hours">{fmtHours(hours)}</span>
        </div>

        {/* Work that is entirely a duplicate shows just why it is skipped. */}
        {!(t.days.length === 0 && !choice.include) && (
        <dl className="rtask-grid">
          <dt>Task cha</dt>
          <dd>
            <div className="rtask-line">
              <input
                className="issue-input"
                value={parentRaw}
                placeholder="#1234"
                aria-label="Số issue task cha"
                onChange={(e) => setParentRaw(e.target.value)}
                onBlur={() => parentRaw !== (view.parent ? `#${view.parent.id}` : "") && onParent(parentRaw)}
                onKeyDown={(e) => e.key === "Enter" && onParent(parentRaw)}
              />
              {t.parent_issue_id === view.parent?.id && view.parent && (
                <span className={`conf conf-${confClass}`} title="Độ tin cậy của AI">
                  {conf}%
                </span>
              )}
              {view.parent ? (
                <span className="rtask-subject">
                  {view.parent.subject}
                  <span className="muted">
                    {" "}
                    {view.parent.tracker}, {view.parent.project_name}
                  </span>
                </span>
              ) : (
                <span className="text-missing">Chưa có, nhập số issue</span>
              )}
            </div>
            {t.parent_reason && t.parent_issue_id === view.parent?.id && <p className="rtask-reason">{t.parent_reason}</p>}
          </dd>

          <dt>Task con</dt>
          <dd>
            {view.existing_subtask && (
              <label className="check compact">
                <input
                  type="checkbox"
                  checked={choice.useExisting}
                  onChange={(e) => onChoice({ useExisting: e.target.checked })}
                />
                <span>
                  Dùng task con có sẵn{" "}
                  <strong>
                    #{view.existing_subtask.id} {view.existing_subtask.subject}
                  </strong>
                </span>
              </label>
            )}
            {!(choice.useExisting && view.existing_subtask) && (
              <div className="rtask-line">
                <input
                  className="subject-input"
                  value={choice.subject}
                  aria-label="Tên task con mới"
                  onChange={(e) => onChoice({ subject: e.target.value })}
                />
                <span className="muted small">tạo mới</span>
              </div>
            )}
          </dd>
        </dl>
        )}

        {view.duplicates.length > 0 && (
          <ul className="rdup">
            {view.duplicates.map((d) => (
              <li key={d.date}>
                {t.days.length === 0 ? "Trùng, không log lại" : `Bỏ qua ${fmtDayShort(d.date)}`}: {d.reason}
              </li>
            ))}
          </ul>
        )}
        {view.problems.length > 0 && (
          <ul className="rwarn">
            {view.problems.map((p) => (
              <li key={p}>{p}</li>
            ))}
          </ul>
        )}
      </div>
    </li>
  );
}
