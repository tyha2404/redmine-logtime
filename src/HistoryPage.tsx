import { useCallback, useEffect, useState } from "react";
import { RowsSkeleton, Skel } from "./Skeleton";
import { api, errorText, type Batch } from "./api";
import PageHeader from "./PageHeader";
import { batchWhat, describeOp, fmtTimestamp, isMainOp } from "./fmt";

export default function HistoryPage() {
  const [batches, setBatches] = useState<Batch[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [open, setOpen] = useState<number | null>(null);
  const [busy, setBusy] = useState<number | null>(null);

  const load = useCallback(() => {
    api
      .listBatches()
      .then((b) => {
        setBatches(b);
        setOpen((o) => o ?? b[0]?.id ?? null);
      })
      .catch((e) => setError(errorText(e)));
  }, []);

  useEffect(load, [load]);

  const act = async (id: number, fn: (id: number) => Promise<Batch>) => {
    setBusy(id);
    setError(null);
    try {
      await fn(id);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(null);
      load();
    }
  };

  if (!batches)
    return error ? (
      <div className="page-pad muted">{error}</div>
    ) : (
      <div className="history page-pad">
        <Skel w={160} h={31} style={{ marginBottom: 8 }} />
        <Skel w="54%" h={14} style={{ marginBottom: 24 }} />
        <RowsSkeleton rows={5} />
      </div>
    );

  return (
    <div className="history page-pad">
      <PageHeader title="Lịch sử ghi" description="Xem kết quả từng lần ghi hoặc đổi tên task, thử lại mục lỗi." />
      {error && <p className="notice error">{error}</p>}
      {batches.length === 0 && <div className="empty">Chưa ghi lô nào.</div>}

      <ul className="batches">
        {batches.map((b) => {
          const entries = b.ops.filter(isMainOp);
          const { verb } = batchWhat(b.ops);
          const count = (s: string) => entries.filter((o) => o.status === s).length;
          const subtasks = b.ops.filter((o) => o.payload.op === "create_subtask" && o.status === "done").length;
          const interrupted = b.ops.some((o) => o.status === "pending" || o.status === "running");
          const isOpen = open === b.id;

          return (
            <li key={b.id} className={`batch${isOpen ? " is-open" : ""}`}>
              <button className="batch-head" aria-expanded={isOpen} onClick={() => setOpen(isOpen ? null : b.id)}>
                <span className="batch-time">{fmtTimestamp(b.created_at)}</span>
                <span className="batch-label">{b.label}</span>
                <span className="batch-counts">
                  {count("done") > 0 && (
                    <span className="text-ok">
                      {count("done")} đã {verb}
                    </span>
                  )}
                  {count("failed") > 0 && <span className="text-missing">{count("failed")} lỗi</span>}
                  {count("undone") > 0 && <span className="muted">{count("undone")} đã hoàn tác</span>}
                  {subtasks > 0 && <span className="muted">{subtasks} task con</span>}
                  {interrupted && <span className="text-partial">bị gián đoạn</span>}
                </span>
              </button>

              {isOpen && (
                <div className="batch-body">
                  <p className="muted small">Ghi lên {b.redmine_url}</p>
                  <ol className="progress">
                    {b.ops.map((op) => (
                      <li key={op.id} className={`op is-${op.status}`}>
                        <span className="op-mark" aria-hidden>
                          {{ done: "✓", failed: "✕", undone: "↺", pending: "…", running: "…" }[op.status]}
                        </span>
                        <span>
                          {describeOp(op, b.ops)}
                          {op.payload.op === "create_time_entry" && op.result_id && (
                            <span className="muted"> (entry {op.result_id})</span>
                          )}
                          {op.error && <span className="op-error">{op.error}</span>}
                        </span>
                      </li>
                    ))}
                  </ol>
                  <div className="row">
                    {count("failed") > 0 && !interrupted && (
                      <button
                        className="btn primary"
                        disabled={busy !== null}
                        onClick={() => act(b.id, api.retryBatch)}
                      >
                        {busy === b.id ? "Đang ghi…" : `Thử lại ${count("failed")} mục lỗi`}
                      </button>
                    )}
                    {interrupted && (
                      <button
                        className="btn primary"
                        disabled={busy !== null}
                        onClick={() => act(b.id, api.resumeBatch)}
                      >
                        {busy === b.id ? "Đang ghi…" : "Tiếp tục ghi phần còn lại"}
                      </button>
                    )}
                  </div>
                </div>
              )}
            </li>
          );
        })}
      </ul>
    </div>
  );
}
