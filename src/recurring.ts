import type { DaySummary, FixedEntry, PlanItem, RecurringTask, Settings } from "./api";
import { parseIso } from "./fmt";

const EPS = 1e-6;

/** ISO weekday (Monday = 1) of a YYYY-MM-DD date. */
export function isoWeekday(date: string): number {
  return ((parseIso(date).getDay() + 6) % 7) + 1;
}

/**
 * The recurring work still due on a calendar day, like logtime-core's
 * `recurring::due`: on its weekdays, on workdays only, not when the day
 * already has time on its issue, in list order while it fits the free hours.
 */
export function recurringDue(settings: Settings, day: DaySummary): FixedEntry[] {
  if (day.status === "weekend" || day.status === "holiday") return [];
  const weekday = isoWeekday(day.date);
  let left = settings.hours_per_day - day.hours;
  const out: FixedEntry[] = [];
  for (const r of settings.recurring ?? []) {
    if (r.paused || !r.issue_id || r.hours <= EPS) continue;
    if (r.weekdays.length > 0 && !r.weekdays.includes(weekday)) continue;
    if (day.entries.some((e) => e.issue?.id === r.issue_id)) continue;
    if (r.hours > left + EPS) continue;
    left -= r.hours;
    out.push(fixedOf(r, day.date));
  }
  return out;
}

export function fixedOf(r: RecurringTask, date: string): FixedEntry {
  const { id, issue_id, label, hours, activity_id, comments } = r;
  return { id, date, issue_id, label, hours, activity_id, comments };
}

/** Item id of a recurring entry in a write plan (and in the report's ticks). */
export const recurringId = (f: Pick<FixedEntry, "id" | "date">) => `rec|${f.id}|${f.date}`;

/** "Họp daily" from "#123 Họp daily". */
export const subjectOf = (label: string) => label.replace(/^#\d+\s*/, "");

/** The time entry to write for a recurring task on its day. */
export function recurringItem(f: FixedEntry, settings: Settings, hours = f.hours): PlanItem {
  return {
    id: recurringId(f),
    date: f.date,
    target: { type: "issue", issue_id: f.issue_id, label: f.label },
    hours,
    activity_id: f.activity_id ?? settings.default_activity_id,
    comments: f.comments.trim() || subjectOf(f.label),
    kind: "work",
  };
}
