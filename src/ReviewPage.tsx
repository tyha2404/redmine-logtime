import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  api,
  errorText,
  type ApiCall,
  type Batch,
  type DayReview,
  type Op,
  type PlanItem,
  type ReviewView,
} from "./api";
import { describeOp, fmtDayShort, fmtHours, parseIso, targetLabel, weekdayShort } from "./fmt";
import QuotaRail from "./QuotaRail";
import type { DraftApi } from "./useDraft";

type Phase = { kind: "idle" } | { kind: "confirm" } | { kind: "running"; ops: Op[] } | { kind: "done"; batch: Batch };

export default function ReviewPage({ draft, onOpenHistory }: { draft: DraftApi; onOpenHistory: () => void }) {
  const [view, setView] = useState<ReviewView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [ack, setAck] = useState(false);
  const [preview, setPreview] = useState<ApiCall[] | null>(null);
  const [phase, setPhase] = useState<Phase>({ kind: "idle" });
  const [redmineUrl, setRedmineUrl] = useState("");

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
    api.getSettings().then((v) => setRedmineUrl(v.settings.redmine_url));
  }, []);

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
    try {
      setPreview(await api.previewItems(items.map((i) => i.id)));
    } catch (e) {
      setError(errorText(e));
    }
  };

  const execute = async () => {
    const ids = items.map((i) => i.id);
    const dates = items.map((i) => i.date).sort();
    const label = `${ids.length} logtime, ${fmtDayShort(dates[0])}${dates.length > 1 && dates[0] !== dates.at(-1) ? `–${fmtDayShort(dates.at(-1)!)}` : ""}`;
    setPhase({ kind: "running", ops: [] });
    setPreview(null);
    const unlisten = await api.onProgress((e) =>
      setPhase((p) => (p.kind === "running" ? { kind: "running", ops: [...p.ops, e.op] } : p)),
    );
    try {
      const batch = await api.executeItems(ids, label);
      setPhase({ kind: "done", batch });
      setAck(false);
    } catch (e) {
      setError(errorText(e));
      setPhase({ kind: "idle" });
    } finally {
      unlisten();
      draft.reload();
    }
  };

  if (!view && !error) return <div className="page-pad muted">Đang đối chiếu với Redmine…</div>;

  return (
    <div className="review-page">
      <section className="review-main">
        <header className="review-head">
          <h1 className="page-title">Duyệt và ghi</h1>
          {view && view.days.length > 0 && (
            <div className="row">
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

        {view && view.days.length === 0 ? (
          <div className="empty">
            <p>Hàng chờ đang trống. Vào Lịch tháng, chọn một ngày và bấm “Thêm logtime” hoặc “Nghỉ cả ngày”.</p>
          </div>
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
                    dưới #{s.parent_id} {s.parent_subject}
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
}) {
  const { day, quota, draft, selected, onToggle, disabled } = props;
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
            {day.existing.map((e) => `${fmtHours(e.hours)} #${e.issue?.id ?? "?"}`).join(", ")}
          </p>
        )}

        <ul className="ritems">
          {day.items.map((i) => (
            <ItemRow
              key={i.id}
              item={i}
              label={targetLabel(i, draft.draft)}
              checked={selected.has(i.id)}
              disabled={disabled}
              onToggle={(on) => onToggle([i.id], on)}
              onChange={(patch) => draft.update(i.id, patch)}
              onRemove={() => draft.remove([i.id])}
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
  label: string;
  checked: boolean;
  disabled: boolean;
  onToggle: (on: boolean) => void;
  onChange: (patch: Partial<PlanItem>) => void;
  onRemove: () => void;
}) {
  const { item, label, checked, disabled, onToggle, onChange, onRemove } = props;
  const [hours, setHours] = useState(String(item.hours));
  const [comments, setComments] = useState(item.comments);

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
        <div className="ritem-target">{label}</div>
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
      <button className="icon-btn" onClick={onRemove} disabled={disabled} aria-label="Bỏ khỏi hàng chờ">
        ×
      </button>
    </li>
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
