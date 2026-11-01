import { forwardRef, useImperativeHandle, useRef } from "react";
import AiLog from "./AiLog";
import type { Settings } from "./api";
import { fmtDayLong, fmtHours } from "./fmt";
import QuotaRail from "./QuotaRail";
import { summaryLine, TaskRow, useReportFlow } from "./reportFlow";
import { useToast } from "./toast";
import type { DraftApi } from "./useDraft";

const EXAMPLE = `Dạ em xin gửi báo cáo công việc tuần này 26/10/2026 ạ
1. Dự án ABC
Sửa feedback công nợ
Review Module Sự kiện

Thứ 2 26/10:
- Sửa feedback công nợ
- Review Module Sự kiện`;

export interface ComposerHandle {
  focus: () => void;
}

interface Props {
  settings: Settings;
  draft: DraftApi;
  onOpenSettings: () => void;
}

/**
 * Step one of "Ghi logtime": a report for one or several days becomes
 * logtimes in the queue below. Claude finds the parent tasks; the user can
 * change them here and again on each queued logtime.
 */
const ReportComposer = forwardRef<ComposerHandle, Props>(function ReportComposer(
  { settings, draft, onOpenSettings },
  ref,
) {
  const f = useReportFlow(null, settings, draft);
  const { ai, view } = f;
  const toast = useToast();
  const box = useRef<HTMLTextAreaElement>(null);

  useImperativeHandle(ref, () => ({
    focus: () => {
      box.current?.scrollIntoView({ block: "center", behavior: "smooth" });
      box.current?.focus({ preventScroll: true });
    },
  }));

  const queue = async () => {
    const n = await f.queue();
    if (!n) return;
    await f.finish();
    toast(`Đã thêm ${n} logtime vào hàng chờ bên dưới.`);
  };

  return (
    <section className="composer" aria-labelledby="composer-title">
      <header className="composer-head">
        <h2 id="composer-title" className="section-title">
          Từ báo cáo công việc
        </h2>
        {view && (
          <button className="link" onClick={f.finish} disabled={ai.running}>
            Bỏ kết quả
          </button>
        )}
      </header>
      <p className="hint">
        Dán báo cáo tuần hoặc các báo cáo ngày. Claude đọc Redmine (chỉ đọc) để tìm task cha, rồi app chia giờ cho đủ{" "}
        {fmtHours(settings.hours_per_day)} mỗi ngày. Việc đã log hoặc đang chờ ghi sẽ được bỏ qua.
      </p>
      {!f.profile && (
        <p className="hint warn-line">
          Nên chạy “Phân tích cấu trúc Redmine” trong{" "}
          <button className="link" onClick={onOpenSettings}>
            Cài đặt
          </button>{" "}
          trước để AI tìm task cha chính xác hơn.
        </p>
      )}
      <textarea
        ref={box}
        className={`report-text${view ? " is-compact" : ""}`}
        value={f.text}
        onChange={(e) => f.setText(e.target.value)}
        placeholder={EXAMPLE}
        disabled={ai.running}
        aria-label="Báo cáo công việc"
      />
      <div className="row">
        <button className="btn primary" onClick={f.analyze} disabled={ai.running || !f.text.trim()}>
          {ai.running ? "Đang phân tích…" : view ? "Phân tích lại" : "Phân tích bằng AI"}
        </button>
      </div>
      <AiLog ai={ai} />

      {view && (
        <div className="composer-result">
          <p className="composer-sum">{summaryLine(f)}</p>

          {view.analysis.questions.length > 0 && (
            <div className="ai-questions">
              <p>Claude cần anh/chị làm rõ (sửa báo cáo rồi phân tích lại):</p>
              <ul>
                {view.analysis.questions.map((q) => (
                  <li key={q}>{q}</li>
                ))}
              </ul>
            </div>
          )}

          <ul className="rtasks">
            {f.tasks.map((t) => (
              <TaskRow
                key={t.task.key}
                view={t}
                choice={f.choices[t.task.key]}
                hours={f.hoursOf(t.task.key)}
                onChoice={(patch) => f.setChoice(t.task.key, patch)}
                onParent={(raw) => f.changeParent(t.task.key, raw)}
              />
            ))}
          </ul>

          <h3 className="composer-sub">Chia giờ theo ngày</h3>
          <div className="rplan">
            {f.plans.map((p) => (
              <div key={p.date} className="rplan-day">
                <div className="rplan-head">
                  <span className="rplan-date">{fmtDayLong(p.date)}</span>
                  <QuotaRail
                    existing={p.existing_hours + p.queued_hours + p.leave_hours}
                    planned={p.allocations.reduce((s, a) => s + a.hours, 0)}
                    quota={settings.hours_per_day}
                  />
                </div>
                <ul className="rplan-allocs">
                  {p.leave_hours > 0 && <li className="is-leave">{fmtHours(p.leave_hours)} nghỉ phép</li>}
                  {p.existing_hours > 0 && <li className="muted">{fmtHours(p.existing_hours)} đã log trước đó</li>}
                  {p.queued_hours > 0 && <li className="muted">{fmtHours(p.queued_hours)} đang chờ ghi</li>}
                  {p.allocations.map((a) => (
                    <li key={a.task}>
                      <strong>{fmtHours(a.hours)}</strong> {f.tasks.find((t) => t.task.key === a.task)?.task.text}
                    </li>
                  ))}
                </ul>
                {p.warnings.map((w) => (
                  <p key={w} className="rwarn-line">
                    {w}
                  </p>
                ))}
              </div>
            ))}
          </div>

          <div className="composer-actions">
            {(f.blocker || f.notice) && <p className="notice error">{f.notice ?? f.blocker}</p>}
            <button className="btn primary" disabled={!!f.blocker || ai.running} onClick={queue}>
              {f.totalHours > 0 ? `Thêm ${fmtHours(f.totalHours)} vào hàng chờ` : "Thêm vào hàng chờ"}
            </button>
          </div>
        </div>
      )}
    </section>
  );
});

export default ReportComposer;
