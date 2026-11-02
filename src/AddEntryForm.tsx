import { useEffect, useState } from "react";
import {
  api,
  errorText,
  newId,
  type Activity,
  type Issue,
  type PlanItem,
  type Settings,
  type SubtaskDraft,
  type Tracker,
} from "./api";
import { fmtHours } from "./fmt";

type Mode = "existing" | "subtask";

interface Props {
  date: string;
  settings: Settings;
  remainingHours: number;
  onAdd: (items: PlanItem[], subtasks: SubtaskDraft[]) => void;
  onCancel: () => void;
}

/** Looks up an issue id as the user types, debounced. */
function useIssueLookup(raw: string) {
  const [issue, setIssue] = useState<Issue | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    setIssue(null);
    setError(null);
    const id = Number(raw.replace(/^#/, ""));
    if (!raw.trim() || !Number.isInteger(id) || id <= 0) return;
    setLoading(true);
    let live = true;
    const t = setTimeout(() => {
      api
        .lookupIssue(id)
        .then((i) => live && setIssue(i))
        .catch((e) => live && setError(errorText(e)))
        .finally(() => live && setLoading(false));
    }, 350);
    return () => {
      live = false;
      clearTimeout(t);
      setLoading(false);
    };
  }, [raw]);

  return { issue, error, loading };
}

export default function AddEntryForm({ date, settings, remainingHours, onAdd, onCancel }: Props) {
  const [mode, setMode] = useState<Mode>("existing");
  const [issueRaw, setIssueRaw] = useState("");
  const [subject, setSubject] = useState("");
  const [trackerId, setTrackerId] = useState<number | null>(settings.subtask_tracker_id);
  // Text, so an empty or half-typed value stays as typed until submit.
  const [hoursRaw, setHoursRaw] = useState(String(remainingHours > 0 ? remainingHours : 1));
  const [activityId, setActivityId] = useState<number | null>(settings.default_activity_id);
  const [comments, setComments] = useState("");
  const [activities, setActivities] = useState<Activity[]>([]);
  const [trackers, setTrackers] = useState<Tracker[]>([]);
  const [formError, setFormError] = useState<string | null>(null);
  /** Submitted while the issue was still being looked up: finish once it is. */
  const [pending, setPending] = useState(false);
  const lookup = useIssueLookup(issueRaw);

  // An old complaint goes as soon as the user changes anything.
  useEffect(() => setFormError(null), [mode, issueRaw, subject, trackerId, hoursRaw, activityId]);

  useEffect(() => {
    // Leave has its own form; its activities would log work as a day off.
    const leave = new Set(settings.leave_types.flatMap((t) => (t.activity_id ? [t.activity_id] : [])));
    api
      .listActivities()
      .then((a) => setActivities(a.filter((x) => x.active && !leave.has(x.id))))
      .catch(() => {});
    api
      .listTrackers()
      .then(setTrackers)
      .catch(() => {});
  }, []);

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    if (lookup.loading) return setPending(true);
    toWrite();
  };

  useEffect(() => {
    if (!pending || lookup.loading) return;
    setPending(false);
    toWrite();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pending, lookup.loading]);

  const toWrite = () => {
    const issue = lookup.issue;
    const what = mode === "existing" ? "issue" : "issue cha";
    if (!issue) {
      if (lookup.error) return setFormError(`Không tìm thấy ${what}: ${lookup.error}`);
      return setFormError(issueRaw.trim() ? `Số ${what} không hợp lệ, ví dụ #1234.` : `Nhập số ${what}, ví dụ #1234.`);
    }
    const hours = parseHours(hoursRaw);
    if (hours === null) return setFormError("Nhập số giờ, ví dụ 2 hoặc 1.5.");
    if (!(hours > 0 && hours <= 24)) return setFormError("Số giờ phải lớn hơn 0 và không quá 24.");
    if (!activityId) return setFormError("Chọn hoạt động.");

    const base = {
      id: newId(),
      date,
      hours,
      activity_id: activityId,
      comments: comments.trim(),
      kind: "work" as const,
    };
    if (mode === "existing") {
      onAdd([{ ...base, target: { type: "issue", issue_id: issue.id, label: `#${issue.id} ${issue.subject}` } }], []);
      return;
    }
    if (!subject.trim()) return setFormError("Nhập tên task con.");
    if (!trackerId) return setFormError("Chọn tracker cho task con.");
    const sub: SubtaskDraft = {
      key: newId(),
      parent_id: issue.id,
      parent_subject: issue.subject,
      project_id: issue.project.id,
      project_name: issue.project.name,
      tracker_id: trackerId,
      subject: subject.trim(),
    };
    onAdd([{ ...base, comments: base.comments || sub.subject, target: { type: "new_subtask", key: sub.key } }], [sub]);
  };

  return (
    <form className="add-form" onSubmit={submit} noValidate>
      <div className="segmented" role="radiogroup" aria-label="Ghi vào đâu">
        <button type="button" role="radio" aria-checked={mode === "existing"} onClick={() => setMode("existing")}>
          Issue có sẵn
        </button>
        <button type="button" role="radio" aria-checked={mode === "subtask"} onClick={() => setMode("subtask")}>
          Tạo task con
        </button>
      </div>

      <label className="field">
        <span>{mode === "existing" ? "Issue" : "Issue cha (do BA tạo)"}</span>
        <input
          inputMode="numeric"
          placeholder="#1234"
          value={issueRaw}
          onChange={(e) => setIssueRaw(e.target.value)}
          autoFocus
        />
        <IssueHint {...lookup} />
      </label>

      {mode === "subtask" && (
        <>
          <label className="field">
            <span>Tên task con</span>
            <input value={subject} onChange={(e) => setSubject(e.target.value)} placeholder="Sửa feedback công nợ" />
          </label>
          <label className="field">
            <span>Tracker</span>
            <select
              value={trackerId ?? ""}
              onChange={(e) => setTrackerId(e.target.value ? Number(e.target.value) : null)}
            >
              <option value="">Chọn tracker</option>
              {trackers.map((t) => (
                <option key={t.id} value={t.id}>
                  {t.name}
                </option>
              ))}
            </select>
          </label>
        </>
      )}

      <div className="field-row">
        <label className="field">
          <span>Số giờ</span>
          <input
            type="number"
            inputMode="decimal"
            min={0.25}
            max={24}
            step={0.25}
            value={hoursRaw}
            onChange={(e) => setHoursRaw(e.target.value)}
          />
          {remainingHours > 0 && <small className="muted">còn trống {fmtHours(remainingHours)}</small>}
        </label>
        <label className="field grow">
          <span>Hoạt động</span>
          <select
            value={activityId ?? ""}
            onChange={(e) => setActivityId(e.target.value ? Number(e.target.value) : null)}
          >
            <option value="">Chọn hoạt động</option>
            {activities.map((a) => (
              <option key={a.id} value={a.id}>
                {a.name}
              </option>
            ))}
          </select>
        </label>
      </div>

      <label className="field">
        <span>Ghi chú</span>
        <input value={comments} onChange={(e) => setComments(e.target.value)} placeholder="Việc đã làm" />
      </label>

      {formError && (
        <p className="notice error" role="alert">
          {formError}
        </p>
      )}
      <div className="form-actions">
        <button type="submit" className="btn primary" disabled={pending}>
          {pending ? "Đang tìm issue…" : "Ghi lên Redmine…"}
        </button>
        <button type="button" className="btn ghost" onClick={onCancel}>
          Huỷ
        </button>
      </div>
    </form>
  );
}

function IssueHint({ issue, error, loading }: ReturnType<typeof useIssueLookup>) {
  if (loading) return <small className="muted">Đang tìm…</small>;
  if (error) return <small className="text-missing">{error}</small>;
  if (!issue) return null;
  return (
    <small className="issue-hint">
      <strong>{issue.subject}</strong>
      <span className="muted">
        {" "}
        trong {issue.project.name}, {issue.tracker.name}, {issue.status.name}
      </span>
    </small>
  );
}

/** "1,5" or "1.5" → 1.5; null when empty or not a number. */
function parseHours(raw: string): number | null {
  const text = raw.trim().replace(",", ".");
  if (!text) return null;
  const n = Number(text);
  return Number.isFinite(n) ? n : null;
}
