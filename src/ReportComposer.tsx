import { forwardRef, useImperativeHandle, useRef, useState } from "react";
import AiLog from "./AiLog";
import type { Settings } from "./api";
import { fmtHours } from "./fmt";
import GchatImport, { appendBlocks } from "./GchatImport";
import { ReportResult, useReportFlow } from "./reportFlow";
import { useToast } from "./toast";

const EXAMPLE = `Dạ em xin gửi báo cáo công việc tuần này 27/10/2026 ạ
1. Dự án ABC
Sửa feedback công nợ
Review Module Sự kiện

Dán một hoặc nhiều báo cáo ngày, kế hoạch tuần (26/10/2026 - 30/10/2026), hoặc bấm “Lấy từ Google Chat”.`;

export interface ComposerHandle {
  focus: () => void;
}

interface Props {
  settings: Settings;
  onOpenSettings: () => void;
  onOpenHistory: () => void;
}

/**
 * "Ghi logtime": a report for one or several days → Claude finds the parent
 * tasks → the user picks tasks and fixes parent, sub-task and hours → one
 * confirmed write to Redmine.
 */
const ReportComposer = forwardRef<ComposerHandle, Props>(function ReportComposer(
  { settings, onOpenSettings, onOpenHistory },
  ref,
) {
  const f = useReportFlow(null, settings);
  const { ai, view } = f;
  const toast = useToast();
  const box = useRef<HTMLTextAreaElement>(null);
  const [importing, setImporting] = useState(false);

  const focus = () => {
    box.current?.scrollIntoView({ block: "center", behavior: "smooth" });
    box.current?.focus({ preventScroll: true });
  };
  useImperativeHandle(ref, () => ({ focus }));

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
        {fmtHours(settings.hours_per_day)} mỗi ngày. Việc đã log sẽ được bỏ qua.
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
        <button className="btn" onClick={() => setImporting(true)} disabled={ai.running}>
          Lấy từ Google Chat
        </button>
      </div>
      <AiLog ai={ai} />

      {view && <ReportResult f={f} settings={settings} onOpenHistory={onOpenHistory} />}

      {importing && (
        <GchatImport
          onClose={() => setImporting(false)}
          onOpenSettings={() => {
            setImporting(false);
            onOpenSettings();
          }}
          onInsert={(blocks) => {
            const { text, added } = appendBlocks(f.text, blocks);
            f.setText(text);
            setImporting(false);
            requestAnimationFrame(focus);
            const skipped = blocks.length - added;
            toast(
              added === 0
                ? "Các tin này đã có trong ô báo cáo."
                : `Đã đưa ${added} tin vào ô báo cáo${skipped ? `, bỏ ${skipped} tin đã có` : ""}. Kiểm tra rồi bấm Phân tích bằng AI.`,
            );
          }}
        />
      )}
    </section>
  );
});

export default ReportComposer;
