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
  const [hours, setHours] = useState(remainingHours > 0 ? remainingHours : 1);
  const [activityId, setActivityId] = useState<number | null>(settings.default_activity_id);
  const [comments, setComments] = useState("");
  const [activities, setActivities] = useState<Activity[]>([]);
  const [trackers, setTrackers] = useState<Tracker[]>([]);
  const [formError, setFormError] = useState<string | null>(null);
  const lookup = useIssueLookup(issueRaw);

  useEffect(() => {
    api.listActivities().then((a) => setActivities(a.filter((x) => x.active))).catch(() => {});
    api.listTrackers().then(setTrackers).catch(() => {});
  }, []);

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    const issue = lookup.issue;
    if (!issue) return setFormError(mode === "existing" ? "Nhập một issue hợp lệ." : "Nhập issue cha hợp lệ.");
    if (!(hours > 0 && hours <= 24)) return setFormError("Số giờ phải từ 0 đến 24.");
    if (!activityId) return setFormError("Chọn hoạt động.");

    const base = { id: newId(), date, hours, activity_id: activityId, comments: comments.trim(), kind: "work" as const, last_error: null };
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
    <form className="add-form" onSubmit={submit}>
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
            <select value={trackerId ?? ""} onChange={(e) => setTrackerId(e.target.value ? Number(e.target.value) : null)}>
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
          <input type="number" min={0.25} max={24} step={0.25} value={hours} onChange={(e) => setHours(Number(e.target.value))} />
          {remainingHours > 0 && <small className="muted">còn trống {fmtHours(remainingHours)}</small>}
        </label>
        <label className="field grow">
          <span>Hoạt động</span>
          <select value={activityId ?? ""} onChange={(e) => setActivityId(e.target.value ? Number(e.target.value) : null)}>
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

      {formError && <p className="notice error">{formError}</p>}
      <div className="form-actions">
        <button type="submit" className="btn primary">
          Thêm vào hàng chờ
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
