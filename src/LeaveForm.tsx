import { useEffect, useMemo, useState } from "react";
import { errorText, type LeaveParent, type PlanItem, type Settings, type SubtaskDraft, type TimeEntry } from "./api";
import { fmtDayShort, fmtHours, parseIso } from "./fmt";
import {
  buildLeave,
  daysOf,
  findParent,
  fmtDays,
  leaveHours,
  leaveSubject,
  leaveYear,
  shortLabel,
  useLeaveEntries,
  type LeaveSpan,
} from "./leave";

const SPANS: { span: LeaveSpan; label: string }[] = [
  { span: "full", label: "Cả ngày" },
  { span: "morning", label: "Buổi sáng" },
  { span: "afternoon", label: "Buổi chiều" },
];

/**
 * A day off, the way HR wants it: a new issue under this month's parent of
 * the chosen kind, named like everyone else's, with the hours logged on it.
 */
export default function LeaveForm(props: {
  date: string;
  settings: Settings;
  /** Hands the leave to the confirmation dialog. */
  onAdd: (item: PlanItem, subtask: SubtaskDraft) => void;
  onCancel: () => void;
  onOpenSettings: () => void;
}) {
  const { date, settings, onAdd, onCancel, onOpenSettings } = props;
  const types = settings.leave_types;
  const [typeKey, setTypeKey] = useState(types[0]?.key ?? "");
  const [span, setSpan] = useState<LeaveSpan>("full");
  const [subject, setSubject] = useState("");
  const [edited, setEdited] = useState(false);
  const [parent, setParent] = useState<LeaveParent | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const type = types.find((t) => t.key === typeKey) ?? types[0];
  const year = Number(date.slice(0, 4));
  const { entries } = useLeaveEntries(year);
  const blocked = useMemo(() => whyNoLeave(settings, date, entries, span), [settings, date, entries, span]);
  const balance = useMemo(
    () => (entries && type ? leaveYear(settings, year, entries).balances.find((b) => b.type.key === type.key) : undefined),
    [entries, settings, year, type],
  );

  // The subject follows the choices until the user types their own.
  useEffect(() => {
    if (type && !edited) setSubject(leaveSubject(settings, type, date, span));
  }, [type, span, date, settings, edited]);

  useEffect(() => {
    if (!type) return;
    let live = true;
    setParent(null);
    setProblem(null);
    findParent(type, date)
      .then((p) => live && setParent(p))
      .catch((e) => live && setProblem(errorText(e)));
    return () => {
      live = false;
    };
  }, [type, date]);

  if (!type) {
    return (
      <p className="hint">
        Chưa có loại nghỉ nào. Vào{" "}
        <button className="link" onClick={onOpenSettings}>
          Cài đặt
        </button>
        , mục Nghỉ phép, bấm “Tìm trên Redmine”.
      </p>
    );
  }

  const add = async () => {
    setBusy(true);
    try {
      const { item, subtask } = await buildLeave({ settings, type, date, span, subject });
      onAdd(item, subtask);
    } catch (e) {
      setProblem(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="leave-form">
      <div className="segmented" role="radiogroup" aria-label="Loại nghỉ">
        {types.map((t) => (
          <button
            key={t.key}
            role="radio"
            aria-checked={t.key === type.key}
            aria-label={t.label}
            title={`${t.label} (${t.project_name})`}
            onClick={() => setTypeKey(t.key)}
          >
            {shortLabel(t.label)}
          </button>
        ))}
      </div>
      <div className="segmented" role="radiogroup" aria-label="Thời gian nghỉ">
        {SPANS.map((s) => (
          <button key={s.span} role="radio" aria-checked={s.span === span} onClick={() => setSpan(s.span)}>
            {s.label}
          </button>
        ))}
      </div>

      {balance && balance.left !== null && (
        <p className={`leave-balance-line${balance.left - daysOf(leaveHours(settings, span), settings.hours_per_day) < 0 ? " is-over" : ""}`}>
          {balance.left - daysOf(leaveHours(settings, span), settings.hours_per_day) < 0
            ? `${type.label} ${year} còn ${fmtDays(Math.max(balance.left, 0))} ngày; thêm ngày này sẽ vượt ${fmtDays(
                -(balance.left - daysOf(leaveHours(settings, span), settings.hours_per_day)),
              )} ngày.`
            : `${type.label} ${year} còn ${fmtDays(balance.left)} trên ${fmtDays(balance.allowance!)} ngày.`}
        </p>
      )}

      <label className="field">
        <span>Tên issue nghỉ</span>
        <input
          value={subject}
          onChange={(e) => {
            setSubject(e.target.value);
            setEdited(true);
          }}
        />
      </label>
      {!settings.leave_name.trim() && (
        <p className="hint">
          Đặt tên của bạn (ví dụ “BE - Tên”) trong{" "}
          <button className="link" onClick={onOpenSettings}>
            Cài đặt
          </button>{" "}
          để tên issue nghỉ giống của mọi người.
        </p>
      )}

      <p className="leave-parent">
        {problem ? (
          <span className="text-missing">{problem}</span>
        ) : parent ? (
          <>
            Dưới <strong>#{parent.id}</strong> {parent.subject}
            <span className="muted"> trong {parent.project_name}</span>
          </>
        ) : (
          <span className="muted">Đang tìm task cha của tháng…</span>
        )}
      </p>

      {blocked && (
        <p className="notice error" role="alert">
          {blocked}
        </p>
      )}

      <div className="form-actions">
        <button className="btn primary" onClick={add} disabled={busy || !parent || !subject.trim() || blocked !== null}>
          {busy ? "Đang chuẩn bị…" : `Ghi ${fmtHours(leaveHours(settings, span))} nghỉ lên Redmine…`}
        </button>
        <button className="btn ghost" onClick={onCancel}>
          Huỷ
        </button>
      </div>
    </div>
  );
}

const EPS = 1e-6;
const DAY_NAMES = ["Chủ nhật", "Thứ bảy"];

/**
 * Why this day cannot take (more) leave, or null when it can: days that are
 * not workdays need none, and a day holds at most one workday of leave,
 * counting what is already on Redmine.
 */
function whyNoLeave(
  settings: Settings,
  date: string,
  entries: TimeEntry[] | null,
  span: LeaveSpan,
): string | null {
  const wd = parseIso(date).getDay();
  if (settings.holidays.includes(date)) return `${fmtDayShort(date)} là ngày lễ, không cần xin nghỉ.`;
  if (!settings.workdays.includes(((wd + 6) % 7) + 1)) {
    const name = wd === 0 ? DAY_NAMES[0] : wd === 6 ? DAY_NAMES[1] : `Ngày ${fmtDayShort(date)}`;
    return `${name} không phải ngày làm việc, không cần xin nghỉ.`;
  }
  const leaveProjects = new Set(settings.leave_types.map((t) => t.project_id));
  const logged = (entries ?? []).filter((e) => e.spent_on === date && leaveProjects.has(e.project.id));
  const taken = logged.reduce((s, e) => s + e.hours, 0);
  if (taken + leaveHours(settings, span) <= settings.hours_per_day + EPS) return null;
  const where = logged.map((e) => (e.issue ? `#${e.issue.id}` : e.project.name)).join(", ");
  if (taken + EPS >= settings.hours_per_day) return `Ngày này đã nghỉ cả ngày (${where}).`;
  return `Ngày này đã nghỉ ${fmtHours(taken)} (${where}); chỉ xin thêm được ${fmtHours(settings.hours_per_day - taken)}. Chọn buổi sáng hoặc buổi chiều.`;
}
