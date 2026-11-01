import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  api,
  errorText,
  issueBrief,
  newId,
  type ApiCall,
  type Batch,
  type DayReview,
  type IssueBrief,
  type Op,
  type PlanItem,
  type ReviewView,
  type Settings,
  type SubtaskDraft,
  type Target,
} from "./api";
import { describeOp, fmtDayShort, fmtHours, parseIso, weekdayShort } from "./fmt";
import { Icon } from "./icons";
import IssueLink, { IssueLabel } from "./IssueLink";
import { buildLeave, leaveTypeOf } from "./leave";
import QuotaRail from "./QuotaRail";
import ReportComposer, { type ComposerHandle } from "./ReportComposer";
import { useToast } from "./toast";
import type { DraftApi } from "./useDraft";
import WeekGrid from "./WeekGrid";

type Layout = "grid" | "list";
const LAYOUT_KEY = "review-layout";

function savedLayout(): Layout {
  try {
    return localStorage.getItem(LAYOUT_KEY) === "list" ? "list" : "grid";
  } catch {
    return "grid";
  }
}

type Phase = { kind: "idle" } | { kind: "confirm" } | { kind: "running"; ops: Op[] } | { kind: "done"; batch: Batch };

interface Props {
  settings: Settings;
  draft: DraftApi;
  /** Opened from the calendar's "fill with a report": focus the report box. */
  focusReport: boolean;
  onOpenHistory: () => void;
  onOpenSettings: () => void;
}

/**
 * "Ghi logtime": a report becomes queued logtimes, the queue is checked and
 * corrected (hours, notes, parent and sub-task), then written to Redmine.
 */
export default function ReviewPage({ settings, draft, focusReport, onOpenHistory, onOpenSettings }: Props) {
  const [view, setView] = useState<ReviewView | null>(null);
  const [error, setError] = useState<string | null>(null);
  /** Why writing (or previewing) failed, shown beside the write button. */
  const [writeError, setWriteError] = useState<string | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [ack, setAck] = useState(false);
  const [preview, setPreview] = useState<ApiCall[] | null>(null);
  const [phase, setPhase] = useState<Phase>({ kind: "idle" });
  const [profileTracker, setProfileTracker] = useState<number | null>(null);
  const composer = useRef<ComposerHandle>(null);
  const [layout, setLayoutState] = useState<Layout>(savedLayout);
  const setLayout = (l: Layout) => {
    setLayoutState(l);
    try {
      localStorage.setItem(LAYOUT_KEY, l);
    } catch {
      // Only a convenience; the default layout is fine.
    }
  };

  const load = useCallback(async () => {
    try {
      setView(await api.reviewDraft());
      setError(null);
    } catch (e) {
      setError(errorText(e));
    }
  }, []);

  // Re-review whenever the draft changes (edits, removals, executions).
  useEffect(() => {
    load();
  }, [load, draft.draft]);

  useEffect(() => {
    api.getProfile().then((p) => setProfileTracker(p?.subtask_tracker_id ?? null));
  }, []);

  useEffect(() => {
    if (focusReport) composer.current?.focus();
  }, [focusReport]);

  const trackerId = settings.subtask_tracker_id ?? profileTracker;
  const redmineUrl = settings.redmine_url;

  // Keep the selection in sync: new items start selected, gone items drop out.
  const allIds = useMemo(() => draft.draft.items.map((i) => i.id), [draft.draft.items]);
  const seen = useRef(new Set<string>());
  useEffect(() => {
    setSelected((prev) => new Set(allIds.filter((id) => prev.has(id) || !seen.current.has(id))));
    allIds.forEach((id) => seen.current.add(id));
  }, [allIds]);

  const items = draft.draft.items.filter((i) => selected.has(i.id));
  const selectedHours = items.reduce((s, i) => s + i.hours, 0);
  const selectedDays = new Set(items.map((i) => i.date)).size;
  const newSubtasks = draft.draft.subtasks.filter((s) =>
    items.some((i) => i.target.type === "new_subtask" && i.target.key === s.key),
  );
  const running = phase.kind === "running";
  const canWrite = items.length > 0 && (newSubtasks.length === 0 || ack) && !running;

  const toggle = (ids: string[], on: boolean) =>
    setSelected((prev) => {
      const next = new Set(prev);
      ids.forEach((id) => (on ? next.add(id) : next.delete(id)));
      return next;
    });

  const showPreview = async () => {
    setWriteError(null);
    try {
      setPreview(await api.previewItems(items.map((i) => i.id)));
    } catch (e) {
      setWriteError(`Không xem trước được: ${errorText(e)}`);
    }
  };

  const execute = async () => {
    const ids = items.map((i) => i.id);
    const dates = items.map((i) => i.date).sort();
    const label = `${ids.length} logtime, ${fmtDayShort(dates[0])}${dates.length > 1 && dates[0] !== dates.at(-1) ? `–${fmtDayShort(dates.at(-1)!)}` : ""}`;
    setPhase({ kind: "running", ops: [] });
    setPreview(null);
    setWriteError(null);
    const unlisten = await api.onProgress((e) =>
      setPhase((p) => (p.kind === "running" ? { kind: "running", ops: [...p.ops, e.op] } : p)),
    );
    try {
      const batch = await api.executeItems(ids, label);
      setPhase({ kind: "done", batch });
      setAck(false);
    } catch (e) {
      setWriteError(`Chưa ghi được: ${errorText(e)}`);
      setPhase({ kind: "idle" });
    } finally {
      unlisten();
      draft.reload();
    }
  };

  return (
    <div className="review-page">
      <section className="review-main">
        <h1 className="page-title">Ghi logtime</h1>
        <ReportComposer ref={composer} settings={settings} draft={draft} onOpenSettings={onOpenSettings} />

        <header className="review-head">
          <h2 className="section-title">
            Hàng chờ
            {draft.draft.items.length > 0 && <span className="section-count">{draft.draft.items.length}</span>}
          </h2>
          {view && view.days.length > 0 && (
            <div className="row">
              <div className="segmented layout-switch" role="radiogroup" aria-label="Cách xem hàng chờ">
                <button role="radio" aria-checked={layout === "grid"} onClick={() => setLayout("grid")}>
                  Bảng tuần
                </button>
                <button role="radio" aria-checked={layout === "list"} onClick={() => setLayout("list")}>
                  Danh sách
                </button>
              </div>
              <button className="btn ghost small" onClick={() => toggle(allIds, true)}>
                Chọn tất cả
              </button>
              <button className="btn ghost small" onClick={() => toggle(allIds, false)}>
                Bỏ chọn
              </button>
            </div>
          )}
        </header>
        {error && <p className="notice error">{error}</p>}

        {!view && !error ? (
          <p className="muted">Đang đối chiếu với Redmine…</p>
        ) : view && view.days.length === 0 ? (
          <div className="empty">
            <p>
              Chưa có gì chờ ghi. Dán báo cáo ở trên, hoặc vào Lịch tháng, chọn một ngày rồi thêm logtime hay nghỉ phép.
            </p>
          </div>
        ) : view && layout === "grid" ? (
          <WeekGrid
            view={view}
            draft={draft}
            settings={settings}
            selected={selected}
            onToggle={toggle}
            disabled={running}
          />
        ) : (
          view?.days.map((day) => (
            <DayBlock
              key={day.date}
              day={day}
              quota={view.hours_per_day}
              draft={draft}
              selected={selected}
              onToggle={toggle}
              disabled={running}
              trackerId={trackerId}
              settings={settings}
            />
          ))
        )}
      </section>

      <aside className="review-side">
        <h2 className="side-title">Lần ghi này</h2>
        <p className="side-sum">
          <strong>{items.length}</strong> logtime, tổng <strong>{fmtHours(selectedHours)}</strong> trên {selectedDays} ngày.
        </p>

        {newSubtasks.length > 0 && (
          <div className="subtask-warn">
            <p>
              Sẽ tạo <strong>{newSubtasks.length} task con mới</strong> trước khi ghi logtime:
            </p>
            <ul>
              {newSubtasks.map((s) => (
                <li key={s.key}>
                  {s.subject}
                  <span className="muted">
                    {" "}
                    dưới <IssueLink id={s.parent_id} /> {s.parent_subject}
                  </span>
                </li>
              ))}
            </ul>
            <label className="check">
              <input type="checkbox" checked={ack} onChange={(e) => setAck(e.target.checked)} />
              Tôi hiểu task con đã tạo không xoá được bằng app này.
            </label>
          </div>
        )}

        {phase.kind === "confirm" ? (
          <div className="confirm">
            <p>
              Ghi {items.length} logtime lên <strong>{redmineUrl}</strong>?
            </p>
            <div className="row">
              <button className="btn primary" onClick={execute}>
                Ghi ngay
              </button>
              <button className="btn ghost" onClick={() => setPhase({ kind: "idle" })}>
                Thôi
              </button>
            </div>
          </div>
        ) : (
          <div className="side-actions">
            <button className="btn primary block" disabled={!canWrite} onClick={() => setPhase({ kind: "confirm" })}>
              {running ? "Đang ghi…" : `Ghi ${items.length} logtime lên Redmine`}
            </button>
            <button className="btn block" disabled={items.length === 0 || running} onClick={showPreview}>
              Xem request sẽ gửi
            </button>
            {!canWrite && !running && (
              <p className="hint side-why">
                {items.length === 0 ? "Chọn ít nhất một logtime để ghi." : "Tick xác nhận về task con ở trên để ghi."}
              </p>
            )}
          </div>
        )}

        {writeError && (
          <p className="notice error side-error" role="alert">
            {writeError}
          </p>
        )}

        {(phase.kind === "running" || phase.kind === "done") && (
          <Progress ops={phase.kind === "running" ? phase.ops : phase.batch.ops} />
        )}
        {phase.kind === "done" && <DoneNote batch={phase.batch} onOpenHistory={onOpenHistory} />}

        {preview && (
          <div className="preview">
            <div className="preview-head">
              <span>{preview.length} request, chưa gửi gì</span>
              <button className="btn ghost small" onClick={() => setPreview(null)}>
                Đóng
              </button>
            </div>
            <pre>
              {preview.map((c) => `${c.method} ${c.path}\n${JSON.stringify(c.body, null, 2)}`).join("\n\n")}
            </pre>
          </div>
        )}
      </aside>
    </div>
  );
}

function DayBlock(props: {
  day: DayReview;
  quota: number;
  draft: DraftApi;
  selected: Set<string>;
  onToggle: (ids: string[], on: boolean) => void;
  disabled: boolean;
  trackerId: number | null;
  settings: Settings;
}) {
  const { day, quota, draft, selected, onToggle, disabled, trackerId, settings } = props;
  const toast = useToast();
  const ids = day.items.map((i) => i.id);
  const allOn = ids.every((id) => selected.has(id));
  const d = parseIso(day.date);
  const state = day.total_hours > quota + 1e-6 ? "over" : day.total_hours + 1e-6 < quota && day.is_workday ? "short" : "ok";

  return (
    <article className={`rday is-${state}`}>
      <div className="rday-date">
        <span className="rday-num">{d.getDate()}</span>
        <span className="rday-wd">
          {weekdayShort(day.date)}, th{d.getMonth() + 1}
        </span>
      </div>
      <div className="rday-body">
        <div className="rday-top">
          <label className="check compact">
            <input type="checkbox" checked={allOn} disabled={disabled} onChange={(e) => onToggle(ids, e.target.checked)} />
            <span className="sr-only">Chọn cả ngày</span>
          </label>
          <QuotaRail existing={day.existing_hours} planned={day.planned_hours} quota={quota} />
          <span className="rday-total">
            {fmtHours(day.total_hours)}
            <span className="muted"> / {fmtHours(quota)}</span>
          </span>
        </div>

        {day.existing.length > 0 && (
          <p className="rday-existing">
            Đã có trên Redmine:{" "}
            {day.existing.map((e, k) => (
              <span key={e.id}>
                {k > 0 && ", "}
                {fmtHours(e.hours)} {e.issue ? <IssueLink id={e.issue.id} /> : e.project.name}
              </span>
            ))}
          </p>
        )}

        <ul className="ritems">
          {day.items.map((i) => (
            <ItemRow
              key={i.id}
              item={i}
              draft={draft}
              trackerId={trackerId}
              settings={settings}
              checked={selected.has(i.id)}
              disabled={disabled}
              onToggle={(on) => onToggle([i.id], on)}
              onChange={(patch) => draft.update(i.id, patch)}
              onRemove={async () => {
                const removed = await draft.remove([i.id]);
                toast(`Đã bỏ ${fmtHours(i.hours)} ngày ${fmtDayShort(i.date)} khỏi hàng chờ.`, {
                  action: { label: "Hoàn tác", run: () => draft.restore(removed) },
                });
              }}
            />
          ))}
        </ul>

        {day.warnings.length > 0 && (
          <ul className="rwarn">
            {day.warnings.map((w) => (
              <li key={w}>{w}</li>
            ))}
          </ul>
        )}
      </div>
    </article>
  );
}

function ItemRow(props: {
  item: PlanItem;
  draft: DraftApi;
  trackerId: number | null;
  settings: Settings;
  checked: boolean;
  disabled: boolean;
  onToggle: (on: boolean) => void;
  onChange: (patch: Partial<PlanItem>) => void;
  onRemove: () => void;
}) {
  const { item, draft, trackerId, settings, checked, disabled, onToggle, onChange, onRemove } = props;
  const toast = useToast();
  const [hours, setHours] = useState(String(item.hours));
  const [comments, setComments] = useState(item.comments);
  const [editing, setEditing] = useState(false);
  const subtask =
    item.target.type === "new_subtask" ? draft.draft.subtasks.find((s) => s.key === (item.target as { key: string }).key) : undefined;
  // A new sub-task is one task: changing it changes every logtime on it.
  const scope =
    item.target.type === "new_subtask"
      ? draft.draft.items.filter((i) => i.target.type === "new_subtask" && i.target.key === subtask?.key).map((i) => i.id)
      : [item.id];
  const label = subtask ? subtask.subject : item.target.type === "issue" ? item.target.label : "Task con mới";
  const leaveType = item.kind === "leave" ? leaveTypeOf(settings, subtask?.project_id) : undefined;

  /** Moves a queued day off to another kind: its project, month parent and activity. */
  const changeLeaveKind = async (key: string) => {
    const next = settings.leave_types.find((t) => t.key === key);
    if (!next || !subtask || next.key === leaveType?.key) return;
    try {
      const subject = leaveType ? subtask.subject.replace(leaveType.verb, next.verb) : undefined;
      const span = item.hours + 1e-6 >= settings.hours_per_day ? "full" : "half";
      const before = draft.snapshot();
      const built = await buildLeave({ settings, type: next, date: item.date, span, subject, key: subtask.key, itemId: item.id });
      await draft.retarget([item.id], built.item.target, built.subtask, {
        activity_id: built.item.activity_id,
        comments: built.item.comments,
      });
      toast(`Đã đổi sang ${next.label.toLowerCase()}.`, { action: { label: "Hoàn tác", run: () => draft.replace(before) } });
    } catch (e) {
      toast(errorText(e), { tone: "error" });
    }
  };

  useEffect(() => setHours(String(item.hours)), [item.hours]);
  useEffect(() => setComments(item.comments), [item.comments]);

  const commitHours = () => {
    const h = Number(hours);
    if (h > 0 && h <= 24 && h !== item.hours) onChange({ hours: h });
    else setHours(String(item.hours));
  };

  return (
    <li className={`ritem${item.kind === "leave" ? " is-leave" : ""}${checked ? "" : " is-off"}`}>
      <input
        type="checkbox"
        checked={checked}
        disabled={disabled}
        onChange={(e) => onToggle(e.target.checked)}
        aria-label={`Chọn ${label}`}
      />
      <span className="ritem-hours">
        <input
          type="number"
          min={0.25}
          max={24}
          step={0.25}
          value={hours}
          disabled={disabled}
          onChange={(e) => setHours(e.target.value)}
          onBlur={commitHours}
          aria-label="Số giờ"
        />
        <span aria-hidden>h</span>
      </span>
      <div className="ritem-main">
        <div className="ritem-target">
          <span className="ritem-target-text">
            {subtask && <span className="ritem-new">{item.kind === "leave" ? "Issue nghỉ mới" : "Task con mới"}</span>}
            <IssueLabel label={label} />
            {subtask && (
              <span className="ritem-parent">
                dưới <IssueLink id={subtask.parent_id} /> {subtask.parent_subject}
              </span>
            )}
          </span>
          {leaveType && settings.leave_types.length > 1 && (
            <select
              className="ritem-kind"
              value={leaveType.key}
              disabled={disabled}
              onChange={(e) => changeLeaveKind(e.target.value)}
              aria-label="Loại nghỉ"
            >
              {settings.leave_types.map((t) => (
                <option key={t.key} value={t.key}>
                  {t.label}
                </option>
              ))}
            </select>
          )}
          {item.kind === "work" && !editing && (
            <button className="ritem-edit" onClick={() => setEditing(true)} disabled={disabled} title="Đổi task cha hoặc task con">
              <Icon name="edit" size={15} />
              Đổi task
            </button>
          )}
        </div>
        {editing && (
          <TargetEditor
            item={item}
            subtask={subtask}
            shared={scope.length}
            trackerId={subtask?.tracker_id ?? trackerId}
            onCancel={() => setEditing(false)}
            onSave={async (target, sub) => {
              setEditing(false);
              if (sameTarget(item.target, target) && (!sub || (subtask && sameSubtask(subtask, sub)))) return;
              const before = draft.snapshot();
              await draft.retarget(scope, target, sub);
              toast(scope.length > 1 ? `Đã đổi task cho ${scope.length} logtime.` : "Đã đổi task.", {
                action: { label: "Hoàn tác", run: () => draft.replace(before) },
              });
            }}
          />
        )}
        <input
          className="ritem-comment"
          value={comments}
          disabled={disabled}
          placeholder="Ghi chú"
          onChange={(e) => setComments(e.target.value)}
          onBlur={() => comments !== item.comments && onChange({ comments })}
          aria-label="Ghi chú"
        />
        {item.last_error && <div className="entry-error">Lần ghi trước lỗi: {item.last_error}</div>}
      </div>
      <button className="icon-btn" onClick={onRemove} disabled={disabled} aria-label="Bỏ khỏi hàng chờ" title="Bỏ khỏi hàng chờ">
        <Icon name="close" size={16} />
      </button>
    </li>
  );
}

type IssueRef = Pick<IssueBrief, "id" | "subject" | "project_id" | "project_name">;

function sameTarget(a: Target, b: Target): boolean {
  if (a.type === "issue" && b.type === "issue") return a.issue_id === b.issue_id;
  if (a.type === "new_subtask" && b.type === "new_subtask") return a.key === b.key;
  return false;
}

function sameSubtask(a: SubtaskDraft, b: SubtaskDraft): boolean {
  return a.parent_id === b.parent_id && a.subject === b.subject && a.tracker_id === b.tracker_id;
}

function parseIssueId(raw: string): number | null {
  const id = Number(raw.trim().replace(/^#/, ""));
  return Number.isInteger(id) && id > 0 ? id : null;
}

/** "#12 Subject" → { id: 12, subject: "Subject" } (labels of issue targets). */
function refFromLabel(id: number, label: string): IssueRef {
  return { id, subject: label.replace(/^#\d+\s*/, ""), project_id: 0, project_name: "" };
}

/**
 * Where a queued logtime goes: a new sub-task (name and parent) or an issue
 * that already exists. Issue numbers are checked on Redmine before saving.
 */
function TargetEditor(props: {
  item: PlanItem;
  subtask: SubtaskDraft | undefined;
  /** How many logtimes the change applies to. */
  shared: number;
  trackerId: number | null;
  onSave: (target: Target, subtask?: SubtaskDraft) => Promise<void>;
  onCancel: () => void;
}) {
  const { item, subtask, shared, trackerId, onSave, onCancel } = props;
  const [mode, setMode] = useState<"new" | "issue">(item.target.type === "new_subtask" ? "new" : "issue");
  const [parent, setParent] = useState<IssueRef | null>(
    subtask
      ? { id: subtask.parent_id, subject: subtask.parent_subject, project_id: subtask.project_id, project_name: subtask.project_name }
      : null,
  );
  const [parentRaw, setParentRaw] = useState(subtask ? `#${subtask.parent_id}` : "");
  const [subject, setSubject] = useState(subtask?.subject ?? item.comments);
  const [issue, setIssue] = useState<IssueRef | null>(
    item.target.type === "issue" ? refFromLabel(item.target.issue_id, item.target.label) : null,
  );
  const [issueRaw, setIssueRaw] = useState(item.target.type === "issue" ? `#${item.target.issue_id}` : "");
  const [busy, setBusy] = useState<"parent" | "issue" | "save" | null>(null);
  const [error, setError] = useState<string | null>(null);

  /** The issue typed in `raw`, looked up on Redmine unless it is `known`. */
  const resolve = async (raw: string, known: IssueRef | null, which: "parent" | "issue"): Promise<IssueRef | null> => {
    const id = parseIssueId(raw);
    if (!id) {
      setError(raw.trim() ? "Số issue không hợp lệ, ví dụ #1234." : which === "parent" ? "Nhập số issue task cha." : "Nhập số issue.");
      return null;
    }
    if (known?.id === id && (which === "issue" || known.project_id)) return known;
    setBusy(which);
    try {
      const found = issueBrief(await api.lookupIssue(id));
      (which === "parent" ? setParent : setIssue)(found);
      setError(null);
      return found;
    } catch (e) {
      (which === "parent" ? setParent : setIssue)(null);
      setError(`Không tìm thấy #${id} trên Redmine: ${errorText(e)}`);
      return null;
    } finally {
      setBusy(null);
    }
  };

  const save = async () => {
    setError(null);
    if (mode === "new") {
      const p = await resolve(parentRaw, parent, "parent");
      if (!p) return;
      if (!subject.trim()) return setError("Nhập tên task con.");
      if (!trackerId) return setError("Chưa chọn tracker cho task con trong Cài đặt.");
      const key = subtask?.key ?? newId();
      setBusy("save");
      await onSave(
        { type: "new_subtask", key },
        {
          key,
          parent_id: p.id,
          parent_subject: p.subject,
          project_id: p.project_id,
          project_name: p.project_name,
          tracker_id: trackerId,
          subject: subject.trim(),
        },
      );
    } else {
      const i = await resolve(issueRaw, issue, "issue");
      if (!i) return;
      setBusy("save");
      await onSave({ type: "issue", issue_id: i.id, label: `#${i.id} ${i.subject}` });
    }
  };

  const onEnter = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") {
      e.preventDefault();
      save();
    } else if (e.key === "Escape") onCancel();
  };

  return (
    <div className="target-editor" role="group" aria-label="Đổi task">
      <div className="segmented" role="radiogroup" aria-label="Log vào đâu">
        <button role="radio" aria-checked={mode === "new"} onClick={() => setMode("new")}>
          Tạo task con mới
        </button>
        <button role="radio" aria-checked={mode === "issue"} onClick={() => setMode("issue")}>
          Log vào issue có sẵn
        </button>
      </div>

      {mode === "new" ? (
        <div className="target-fields">
          <label className="target-field">
            <span>Task cha</span>
            <span className="target-issue">
              <input
                className="issue-input"
                value={parentRaw}
                placeholder="#1234"
                onChange={(e) => setParentRaw(e.target.value)}
                onBlur={() => parentRaw.trim() && resolve(parentRaw, parent, "parent")}
                onKeyDown={onEnter}
                autoFocus={!subtask}
              />
              <span className="target-found">
                {busy === "parent" ? "Đang tìm…" : parent && parseIssueId(parentRaw) === parent.id ? parent.subject : ""}
              </span>
            </span>
          </label>
          <label className="target-field">
            <span>Tên task con</span>
            <input
              className="subject-input"
              value={subject}
              onChange={(e) => setSubject(e.target.value)}
              onKeyDown={onEnter}
              autoFocus={!!subtask}
            />
          </label>
        </div>
      ) : (
        <div className="target-fields">
          <label className="target-field">
            <span>Issue</span>
            <span className="target-issue">
              <input
                className="issue-input"
                value={issueRaw}
                placeholder="#1234"
                onChange={(e) => setIssueRaw(e.target.value)}
                onBlur={() => issueRaw.trim() && resolve(issueRaw, issue, "issue")}
                onKeyDown={onEnter}
                autoFocus
              />
              <span className="target-found">
                {busy === "issue" ? "Đang tìm…" : issue && parseIssueId(issueRaw) === issue.id ? issue.subject : ""}
              </span>
            </span>
          </label>
        </div>
      )}

      {error && <p className="notice error">{error}</p>}
      <div className="target-actions">
        <button className="btn primary small" onClick={save} disabled={busy !== null}>
          {busy === "save" ? "Đang lưu…" : "Lưu"}
        </button>
        <button className="btn ghost small" onClick={onCancel}>
          Huỷ
        </button>
        {shared > 1 && <span className="muted small">Áp dụng cho cả {shared} logtime của task con này.</span>}
      </div>
    </div>
  );
}

function Progress({ ops }: { ops: Op[] }) {
  if (ops.length === 0) return <p className="muted">Đang bắt đầu…</p>;
  return (
    <ol className="progress">
      {ops.map((op) => (
        <li key={`${op.id}-${op.status}`} className={`op is-${op.status}`}>
          <span className="op-mark" aria-hidden>
            {op.status === "done" ? "✓" : op.status === "failed" ? "✕" : "…"}
          </span>
          <span>
            {describeOp(op, ops)}
            {op.error && <span className="op-error">{op.error}</span>}
          </span>
        </li>
      ))}
    </ol>
  );
}

function DoneNote({ batch, onOpenHistory }: { batch: Batch; onOpenHistory: () => void }) {
  const entries = batch.ops.filter((o) => o.payload.op === "create_time_entry");
  const ok = entries.filter((o) => o.status === "done").length;
  const failed = entries.length - ok;
  return (
    <div className={`done-note${failed ? " has-failed" : ""}`}>
      <p>
        Đã ghi {ok}/{entries.length} logtime.
        {failed > 0 && ` ${failed} mục lỗi đã trở lại hàng chờ để sửa.`}
      </p>
      <button className="link" onClick={onOpenHistory}>
        Xem trong lịch sử ghi
      </button>
    </div>
  );
}
