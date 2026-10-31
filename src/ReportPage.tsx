import AiLog from "./AiLog";
import type { Settings } from "./api";
import { fmtDayLong, fmtHours } from "./fmt";
import QuotaRail from "./QuotaRail";
import { summaryLine, TaskRow, useReportFlow } from "./reportFlow";
import type { DraftApi } from "./useDraft";

const EXAMPLE = `Dạ em xin gửi báo cáo công việc tuần này 26/10/2026 ạ
1. Davipharm - Salex
Sửa feedback công nợ
Review Module Sự kiện

Thứ 2 26/10:
- Sửa feedback công nợ
- Review Module Sự kiện`;

interface Props {
  settings: Settings;
  draft: DraftApi;
  onOpenReview: () => void;
  onOpenSettings: () => void;
}

/** A report covering one or several days (a weekly report, daily reports…). */
export default function ReportPage({ settings, draft, onOpenReview, onOpenSettings }: Props) {
  const f = useReportFlow(null, settings, draft);
  const { ai, view } = f;

  const queue = async () => {
    if (await f.queue()) onOpenReview();
  };

  return (
    <div className="report-page">
      <section className="report-input">
        <h1 className="page-title">Nhập báo cáo</h1>
        <p className="hint">
          Dán báo cáo cho một hoặc nhiều ngày: báo cáo tuần, các báo cáo ngày, hoặc cả hai. Claude đọc Redmine (chỉ đọc)
          để tìm task cha, rồi app chia giờ cho đủ {fmtHours(settings.hours_per_day)} mỗi ngày. Việc đã log hoặc đang chờ
          ghi sẽ được bỏ qua.
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
          className="report-text"
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
      </section>

      <section className="report-result">
        {!view ? (
          <div className="empty">
            <p>Kết quả phân tích sẽ hiện ở đây: từng việc với task cha trên Redmine, và số giờ chia cho từng ngày.</p>
          </div>
        ) : (
          <>
            <p className="month-summary">{summaryLine(f)}</p>

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

            <h2 className="section-title">Việc trong báo cáo</h2>
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

            <h2 className="section-title">Chia giờ theo ngày</h2>
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

            <div className="report-actions">
              {(f.blocker || f.notice) && <p className="notice error">{f.notice ?? f.blocker}</p>}
              <button className="btn primary" disabled={!!f.blocker || ai.running} onClick={queue}>
                Đưa vào hàng chờ để duyệt
              </button>
            </div>
          </>
        )}
      </section>
    </div>
  );
}
