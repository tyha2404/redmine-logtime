import { useState } from "react";
import { api, errorText, type ApiCall, type Batch, type Op, type WritePlan } from "./api";
import { batchWhat, describeOp, fmtDayLong, fmtDayShort, fmtHours, isMainOp, targetLabel } from "./fmt";
import { useModal } from "./useModal";

type Phase =
  | { kind: "confirm" }
  | { kind: "running"; what: "write" | "retry" | "undo"; ops: Op[] }
  | { kind: "done"; batch: Batch; undone: boolean };

/** "3 logtime, 26/10–28/10" or "Đổi tên #77": the batch name in the history. */
export function batchLabel(plan: WritePlan): string {
  const renames = plan.renames ?? [];
  const parts: string[] = [];
  if (plan.items.length) {
    const dates = plan.items.map((i) => i.date).sort();
    const span =
      dates[0] === dates.at(-1) ? fmtDayShort(dates[0]) : `${fmtDayShort(dates[0])}–${fmtDayShort(dates.at(-1)!)}`;
    parts.push(`${plan.items.length} logtime, ${span}`);
  }
  if (renames.length) parts.push(`Đổi tên ${renames.map((r) => `#${r.issue_id}`).join(", ")}`);
  return parts.join("; ");
}

function hostOf(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return url;
  }
}

interface Props {
  plan: WritePlan;
  redmineUrl: string;
  /** Called once the dialog closes, with the batch when anything was sent. */
  onClose: (batch: Batch | null) => void;
  onOpenHistory?: () => void;
}

/**
 * The last step before Redmine: what will be created and logged, an explicit
 * "Ghi ngay", then live progress, failures with a retry, and undo.
 */
export default function WriteDialog({ plan, redmineUrl, onClose, onOpenHistory }: Props) {
  const [phase, setPhase] = useState<Phase>({ kind: "confirm" });
  const [ack, setAck] = useState(false);
  const [preview, setPreview] = useState<ApiCall[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [confirmUndo, setConfirmUndo] = useState(false);
  const running = phase.kind === "running";
  const finish = () => onClose(phase.kind === "done" ? phase.batch : null);
  const { close, ...modal } = useModal(finish, running);

  const newSubtasks = plan.subtasks.filter((s) =>
    plan.items.some((i) => i.target.type === "new_subtask" && i.target.key === s.key),
  );
  const renames = plan.renames ?? [];
  const total = plan.items.reduce((s, i) => s + i.hours, 0);
  const byDate = [...new Set(plan.items.map((i) => i.date))].sort();
  const asks = [
    plan.items.length ? (
      <>
        ghi <strong>{plan.items.length} logtime</strong>, tổng <strong>{fmtHours(total)}</strong>
      </>
    ) : null,
    renames.length ? (
      <>
        đổi tên <strong>{renames.length} task</strong>
      </>
    ) : null,
  ].filter(Boolean);

  /** Runs a write, retry or undo, showing each op as Redmine answers. */
  const run = async (what: "write" | "retry" | "undo", start: Op[], call: () => Promise<Batch>) => {
    setError(null);
    setPhase({ kind: "running", what, ops: start });
    const unlisten = await api.onProgress((e) =>
      setPhase((p) => {
        if (p.kind !== "running") return p;
        const i = p.ops.findIndex((o) => o.id === e.op.id);
        const ops = i < 0 ? [...p.ops, e.op] : p.ops.map((o, j) => (j === i ? e.op : o));
        return { ...p, ops };
      }),
    );
    try {
      const batch = await call();
      setPhase({ kind: "done", batch, undone: what === "undo" });
    } catch (e) {
      setError(errorText(e));
      setPhase((p) => (p.kind === "running" && what === "write" ? { kind: "confirm" } : p));
    } finally {
      unlisten();
    }
  };

  const write = () => run("write", [], () => api.writePlan(plan, batchLabel(plan)));

  const showPreview = async () => {
    if (preview) return setPreview(null);
    try {
      setPreview(await api.previewPlan(plan));
    } catch (e) {
      setError(`Không xem trước được: ${errorText(e)}`);
    }
  };

  const ops = phase.kind === "running" ? phase.ops : phase.kind === "done" ? phase.batch.ops : [];
  const entries = ops.filter(isMainOp);
  const ok = entries.filter((o) => o.status === "done").length;
  const failed = entries.filter((o) => o.status === "failed").length;
  const undone = entries.filter((o) => o.status === "undone").length;
  const { verb, noun } = batchWhat(phase.kind === "done" ? phase.batch.ops : ops);
  /** Undo of new entries only deletes them; a rename gets its old name back. */
  const onlyNew = entries.every((o) => o.payload.op === "create_time_entry");
  const madeSubtasks = ops.some((o) => o.payload.op === "create_subtask" && o.status === "done");

  return (
    <dialog {...modal} className="confirm-dialog write-dialog" aria-labelledby="write-title">
      {phase.kind === "confirm" ? (
        <>
          <h2 id="write-title">{plan.items.length ? "Ghi lên Redmine?" : "Đổi tên task trên Redmine?"}</h2>
          <p>
            Sẽ{" "}
            {asks.map((a, i) => (
              <span key={i}>
                {i === 0 ? "" : i === asks.length - 1 ? " và " : ", "}
                {a}
              </span>
            ))}{" "}
            trên <strong>{hostOf(redmineUrl)}</strong>.
          </p>

          <div className="write-body">
            {renames.length > 0 && (
              <table className="write-table">
                <tbody>
                  {renames.map((r) => (
                    <tr key={r.issue_id}>
                      <th scope="row">#{r.issue_id}</th>
                      <td>
                        <span className="write-kind">Tên mới</span> {r.after.trim()}
                        <span className="write-comment">Tên cũ: {r.before}</span>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
            {newSubtasks.length > 0 && (
              <section className="write-subtasks">
                <h3>Tạo {newSubtasks.length} task con mới</h3>
                <ul>
                  {newSubtasks.map((s) => (
                    <li key={s.key}>
                      <strong>{s.subject}</strong>
                      <span className="muted">
                        {" "}
                        dưới #{s.parent_id} {s.parent_subject}
                      </span>
                    </li>
                  ))}
                </ul>
              </section>
            )}

            {plan.items.length > 0 && (
              <table className="write-table">
                <tbody>
                  {byDate.map((date) =>
                    plan.items
                      .filter((i) => i.date === date)
                      .map((i, n, day) => (
                        <tr key={i.id}>
                          {n === 0 && (
                            <th scope="row" rowSpan={day.length}>
                              {fmtDayLong(date)}
                            </th>
                          )}
                          <td>
                            {targetLabel(i, plan)}
                            {i.comments && i.comments !== targetLabel(i, plan) && (
                              <span className="write-comment">{i.comments}</span>
                            )}
                          </td>
                          <td className="write-hours">{fmtHours(i.hours)}</td>
                        </tr>
                      )),
                  )}
                </tbody>
              </table>
            )}
          </div>

          {newSubtasks.length > 0 && (
            <label className="check write-ack">
              <input type="checkbox" checked={ack} onChange={(e) => setAck(e.target.checked)} />
              <span>Tôi hiểu task con đã tạo không xoá được bằng app này.</span>
            </label>
          )}

          <button className="link write-preview-toggle" onClick={showPreview}>
            {preview ? "Ẩn request" : "Xem request sẽ gửi"}
          </button>
          {preview && (
            <pre className="write-preview">
              {preview.map((c) => `${c.method} ${c.path}\n${JSON.stringify(c.body, null, 2)}`).join("\n\n")}
            </pre>
          )}
          {error && (
            <p className="notice error" role="alert">
              Chưa ghi được: {error}
            </p>
          )}

          <div className="confirm-dialog-actions">
            <button className="btn ghost" onClick={() => close()}>
              Huỷ
            </button>
            <button className="btn primary" disabled={newSubtasks.length > 0 && !ack} onClick={write} autoFocus>
              {plan.items.length ? "Ghi ngay" : "Đổi tên ngay"}
            </button>
          </div>
        </>
      ) : (
        <>
          <h2 id="write-title">
            {phase.kind === "running"
              ? { write: "Đang ghi lên Redmine…", retry: "Đang ghi lại phần lỗi…", undo: "Đang hoàn tác…" }[phase.what]
              : phase.undone
                ? onlyNew
                  ? `Đã hoàn tác ${undone} logtime`
                  : `Đã hoàn tác ${undone} thay đổi`
                : failed
                  ? `Đã ${verb} ${ok}/${entries.length} ${noun}, ${failed} mục lỗi`
                  : `Đã ${verb} ${ok} ${noun} trên Redmine`}
          </h2>
          {phase.kind === "done" && failed > 0 && !phase.undone && (
            <p>Sửa nguyên nhân bên dưới rồi bấm Thử lại, hoặc đóng để chỉnh lại.</p>
          )}
          {phase.kind === "done" && phase.undone && madeSubtasks && <p>Task con đã tạo vẫn giữ trên Redmine.</p>}

          <div className="write-body">
            {ops.length === 0 ? (
              <p className="muted">Đang bắt đầu…</p>
            ) : (
              <ol className="progress">
                {ops.map((op) => (
                  <li key={op.id} className={`op is-${op.status}`}>
                    <span className="op-mark" aria-hidden>
                      {{ done: "✓", failed: "✕", undone: "↺", pending: "…", running: "…" }[op.status]}
                    </span>
                    <span>
                      {describeOp(op, ops)}
                      {op.error && <span className="op-error">{op.error}</span>}
                    </span>
                  </li>
                ))}
              </ol>
            )}
          </div>
          {error && (
            <p className="notice error" role="alert">
              {error}
            </p>
          )}

          {phase.kind === "done" && (
            <div className="confirm-dialog-actions">
              {onOpenHistory && (
                <button
                  className="btn ghost"
                  onClick={() =>
                    close(() => {
                      finish();
                      onOpenHistory();
                    })
                  }
                >
                  Xem lịch sử ghi
                </button>
              )}
              {ok > 0 &&
                !phase.undone &&
                (confirmUndo ? (
                  <button
                    className="btn danger-solid"
                    onClick={() => {
                      setConfirmUndo(false);
                      run("undo", phase.batch.ops, () => api.undoBatch(phase.batch.id));
                    }}
                  >
                    {onlyNew ? `Xoá ${ok} logtime khỏi Redmine` : `Hoàn tác ${ok} thay đổi`}
                  </button>
                ) : (
                  <button className="btn" onClick={() => setConfirmUndo(true)}>
                    Hoàn tác lô này
                  </button>
                ))}
              {failed > 0 && !phase.undone && (
                <button
                  className="btn"
                  onClick={() => run("retry", phase.batch.ops, () => api.retryBatch(phase.batch.id))}
                >
                  Thử lại phần lỗi
                </button>
              )}
              <button className="btn primary" onClick={() => close()} autoFocus>
                Đóng
              </button>
            </div>
          )}
        </>
      )}
    </dialog>
  );
}
