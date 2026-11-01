import { useEffect, useState } from "react";
import {
  api,
  errorText,
  newId,
  type Draft,
  type LeaveParent,
  type LeaveType,
  type PlanItem,
  type Settings,
  type SubtaskDraft,
  type TimeEntry,
} from "./api";

/** Which part of the day is off. "half" is a half day without saying which. */
export type LeaveSpan = "full" | "morning" | "afternoon" | "half";

export function leaveHours(settings: Settings, span: LeaveSpan): number {
  return span === "full" ? settings.hours_per_day : settings.hours_per_day / 2;
}

/** "BE - Tỷ xin nghỉ phép ngày 29/10/2026", the way colleagues name theirs. */
export function leaveSubject(settings: Settings, type: LeaveType, date: string, span: LeaveSpan): string {
  const [y, m, d] = date.split("-");
  const when = { full: "ngày", morning: "sáng ngày", afternoon: "chiều ngày", half: "1/2 ngày" }[span];
  return [settings.leave_name.trim(), type.verb, when, `${d}/${m}/${y}`].filter(Boolean).join(" ");
}

/** Month parents are looked up once per kind and month while queueing. */
export type ParentCache = Map<string, Promise<LeaveParent>>;

export function findParent(type: LeaveType, date: string, cache?: ParentCache): Promise<LeaveParent> {
  const k = `${type.project_id}:${date.slice(0, 7)}`;
  const hit = cache?.get(k);
  if (hit) return hit;
  const p = api.leaveParent(type.project_id, date);
  cache?.set(k, p);
  return p;
}

/**
 * One day off: a new issue under HR's parent for that month, and the
 * logtime on it. `key` keeps an existing draft sub-task when changing kind.
 */
export async function buildLeave(opts: {
  settings: Settings;
  type: LeaveType;
  date: string;
  span: LeaveSpan;
  subject?: string;
  cache?: ParentCache;
  key?: string;
  itemId?: string;
}): Promise<{ item: PlanItem; subtask: SubtaskDraft }> {
  const { settings, type, date, span } = opts;
  const parent = await findParent(type, date, opts.cache);
  const key = opts.key ?? newId();
  const subtask: SubtaskDraft = {
    key,
    parent_id: parent.id,
    parent_subject: parent.subject,
    project_id: parent.project_id,
    project_name: parent.project_name,
    tracker_id: parent.tracker_id,
    subject: opts.subject?.trim() || leaveSubject(settings, type, date, span),
    start_date: date,
  };
  const item: PlanItem = {
    id: opts.itemId ?? newId(),
    date,
    target: { type: "new_subtask", key },
    hours: leaveHours(settings, span),
    activity_id: type.activity_id ?? settings.default_activity_id,
    comments: span === "full" ? type.label : `${type.label} nửa ngày`,
    kind: "leave",
    last_error: null,
  };
  return { item, subtask };
}

/** The kind of a queued leave, from the project its issue goes into. */
export function leaveTypeOf(settings: Settings, projectId: number | undefined): LeaveType | undefined {
  return settings.leave_types.find((t) => t.project_id === projectId);
}

const DEFAULT_ALLOWANCE: Record<string, number> = { annual: 12, sick: 4 };

/** Days per year for a kind of leave, or null when it has no limit. */
export function allowanceOf(type: LeaveType): number | null {
  return type.days_per_year ?? DEFAULT_ALLOWANCE[type.key] ?? null;
}

/**
 * Leave hours on one date as days, to the nearest half day: HR counts whole
 * and half days, and a 7h entry on an 8h day is still a day off.
 */
export function daysOf(hours: number, hoursPerDay: number): number {
  return Math.max(0.5, Math.round((hours / hoursPerDay) * 2) / 2);
}

export interface LeaveDay {
  date: string;
  type: LeaveType;
  hours: number;
  days: number;
  issueId: number | null;
  /** Still in the review queue, not on Redmine yet. */
  queued: boolean;
}

export interface LeaveBalance {
  type: LeaveType;
  allowance: number | null;
  used: number;
  queued: number;
  /** Allowance minus used and queued; null without a limit. */
  left: number | null;
}

/** Logged and queued leave of one year, per date and per kind. */
export function leaveYear(
  settings: Settings,
  year: number,
  entries: TimeEntry[],
  draft: Pick<Draft, "items" | "subtasks">,
): { days: LeaveDay[]; balances: LeaveBalance[] } {
  const hpd = settings.hours_per_day;
  const byKey = new Map<string, Omit<LeaveDay, "days">>();
  const put = (date: string, type: LeaveType, hours: number, issueId: number | null, queued: boolean) => {
    const k = `${date}|${type.project_id}|${queued}`;
    const prev = byKey.get(k);
    byKey.set(k, { date, type, hours: (prev?.hours ?? 0) + hours, issueId: prev?.issueId ?? issueId, queued });
  };
  for (const e of entries) {
    const type = leaveTypeOf(settings, e.project.id);
    if (type && e.spent_on.startsWith(`${year}-`)) put(e.spent_on, type, e.hours, e.issue?.id ?? null, false);
  }
  for (const i of draft.items) {
    if (i.kind !== "leave" || !i.date.startsWith(`${year}-`)) continue;
    const projectId =
      i.target.type === "new_subtask" ? draft.subtasks.find((s) => s.key === (i.target as { key: string }).key)?.project_id : undefined;
    const type = leaveTypeOf(settings, projectId);
    if (type) put(i.date, type, i.hours, i.target.type === "issue" ? i.target.issue_id : null, true);
  }
  const days = [...byKey.values()]
    .map((d) => ({ ...d, days: daysOf(d.hours, hpd) }))
    .sort((a, b) => a.date.localeCompare(b.date));
  const balances = settings.leave_types.map((type) => {
    const mine = days.filter((d) => d.type.project_id === type.project_id);
    const used = mine.filter((d) => !d.queued).reduce((s, d) => s + d.days, 0);
    const queued = mine.filter((d) => d.queued).reduce((s, d) => s + d.days, 0);
    const allowance = allowanceOf(type);
    return { type, allowance, used, queued, left: allowance === null ? null : allowance - used - queued };
  });
  return { days, balances };
}

/** "12", "11.5", written like hours elsewhere in the app. */
export function fmtDays(n: number): string {
  return Number.isInteger(n) ? String(n) : n.toFixed(1);
}

const entryCache = new Map<number, { at: number; entries: Promise<TimeEntry[]> }>();

/** A year's leave entries, shared for a minute between pages and forms. */
export function useLeaveEntries(year: number, reloadKey = 0) {
  const [state, setState] = useState<{ entries: TimeEntry[] | null; error: string | null }>({ entries: null, error: null });
  useEffect(() => {
    let live = true;
    const hit = entryCache.get(year);
    const fresh = hit && Date.now() - hit.at < 60_000 && reloadKey === 0;
    const entries = fresh ? hit.entries : api.leaveEntries(year);
    if (!fresh) entryCache.set(year, { at: Date.now(), entries });
    setState((s) => ({ entries: s.entries, error: null }));
    entries
      .then((e) => live && setState({ entries: e, error: null }))
      .catch((e) => {
        entryCache.delete(year);
        if (live) setState({ entries: null, error: errorText(e) });
      });
    return () => {
      live = false;
    };
  }, [year, reloadKey]);
  return state;
}

/** "Nghỉ phép năm" → "Phép năm", where the context already says it is leave. */
export function shortLabel(label: string): string {
  const s = label.replace(/^nghỉ\s+/i, "");
  return s.charAt(0).toUpperCase() + s.slice(1);
}
