import type { Op, PlanItem, WritePlan } from "./api";

const WEEKDAY_NAMES = ["Chủ nhật", "Thứ hai", "Thứ ba", "Thứ tư", "Thứ năm", "Thứ sáu", "Thứ bảy"];
const WEEKDAY_SHORT = ["CN", "T2", "T3", "T4", "T5", "T6", "T7"];

export function pad(n: number): string {
  return String(n).padStart(2, "0");
}

export function isoOf(d: Date): string {
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

export function todayIso(): string {
  return isoOf(new Date());
}

export function parseIso(iso: string): Date {
  const [y, m, d] = iso.split("-").map(Number);
  return new Date(y, m - 1, d);
}

export function fmtHours(h: number): string {
  return `${Number.isInteger(h) ? h : h.toFixed(2).replace(/\.?0+$/, "")}h`;
}

/** "Thứ tư, 28/10" */
export function fmtDayLong(iso: string): string {
  const d = parseIso(iso);
  return `${WEEKDAY_NAMES[d.getDay()]}, ${pad(d.getDate())}/${pad(d.getMonth() + 1)}`;
}

export function fmtDayShort(iso: string): string {
  const d = parseIso(iso);
  return `${pad(d.getDate())}/${pad(d.getMonth() + 1)}`;
}

export function weekdayShort(iso: string): string {
  return WEEKDAY_SHORT[parseIso(iso).getDay()];
}

/** "28/10/2026 23:10" from an RFC 3339 UTC timestamp, in local time. */
export function fmtTimestamp(ts: string): string {
  const d = new Date(ts);
  return `${pad(d.getDate())}/${pad(d.getMonth() + 1)}/${d.getFullYear()} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/** What an item will be logged against, in words. */
export function targetLabel(item: PlanItem, plan: Pick<WritePlan, "subtasks">): string {
  if (item.target.type === "issue") return item.target.label;
  const key = item.target.key;
  const sub = plan.subtasks.find((s) => s.key === key);
  if (!sub) return "Task con mới";
  return item.kind === "leave" ? sub.subject : `Task con mới: ${sub.subject}`;
}

/** One line describing a batch operation; `ops` resolves new sub-task names. */
export function describeOp(op: Op, ops: Op[]): string {
  const p = op.payload;
  if (p.op === "create_subtask") {
    return `Tạo task con "${p.subject}" dưới #${p.parent_id}${op.result_id ? `, thành #${op.result_id}` : ""}`;
  }
  if (p.op === "rename_issue") {
    return `Đổi tên #${p.issue_id}: “${p.before}” → “${p.after.trim()}”`;
  }
  let target: string;
  if (p.target.type === "issue") {
    target = p.target.label;
  } else {
    const key = p.target.key;
    const sub = ops.find((o) => o.payload.op === "create_subtask" && o.payload.key === key);
    const subject = sub?.payload.op === "create_subtask" ? sub.payload.subject : "task con mới";
    target = sub?.result_id ? `#${sub.result_id} ${subject}` : subject;
  }
  return `${fmtDayShort(p.date)}: ${fmtHours(p.hours)} vào ${target}`;
}

/** Ops a batch is about: new entries and renames (not the sub-tasks created for them). */
export const isMainOp = (o: Op): boolean => o.payload.op !== "create_subtask";

/** What a batch did, in words: "ghi … logtime", "đổi tên … task", or "cập nhật … mục" when mixed. */
export function batchWhat(ops: Op[]): { verb: string; noun: string } {
  const kinds = new Set(ops.filter(isMainOp).map((o) => o.payload.op));
  if (kinds.size === 1 && kinds.has("rename_issue")) return { verb: "đổi tên", noun: "task" };
  if (kinds.size === 1) return { verb: "ghi", noun: "logtime" };
  return { verb: "cập nhật", noun: "mục" };
}
