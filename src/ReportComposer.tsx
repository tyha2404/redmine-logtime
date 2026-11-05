import { forwardRef, useEffect, useImperativeHandle, useRef, useState } from "react";
import AiLog from "./AiLog";
import type { Settings } from "./api";
import { fmtDayShort, fmtHours, isoOf, parseIso, todayIso } from "./fmt";
import GchatImport, { appendBlocks, chatBlocks, rangeChatMessages, sentOn } from "./GchatImport";
import { ReportResult, useReportFlow } from "./reportFlow";
import { useToast } from "./toast";

const EXAMPLE = `Dạ em xin gửi báo cáo công việc tuần này 27/10/2026 ạ
1. Dự án ABC
Sửa feedback công nợ
Review Module Sự kiện

Dán một hoặc nhiều báo cáo ngày, kế hoạch tuần (26/10/2026 - 30/10/2026), hoặc bấm “Lấy từ Google Chat”.`;

/** A request from the calendar: report these short days (ISO dates) in one go. */
export type FillAsk = { dates: string[]; n: number };

/**
 * Which messages belong to the short days: sent on one of them, or the day
 * after (a report sent the next morning). The range ends today at the latest.
 */
export function fillRange(dates: string[], today: string) {
  const sorted = [...dates].sort();
  const days = new Set(sorted.flatMap((d) => [d, nextDay(d)]));
  const last = nextDay(sorted[sorted.length - 1]);
  return { from: sorted[0], to: last > today ? today : last, keep: (sent: string) => days.has(sent) };
}

/** Short days no message was sent on (nor the day after): they need a report typed in. */
export function uncoveredDays(dates: string[], sent: string[]): string[] {
  const on = new Set(sent);
  return [...dates].sort().filter((d) => !on.has(d) && !on.has(nextDay(d)));
}

function nextDay(iso: string): string {
  const d = parseIso(iso);
  d.setDate(d.getDate() + 1);
  return isoOf(d);
}

export interface ComposerHandle {
  focus: () => void;
}

interface Props {
  settings: Settings;
  onOpenSettings: () => void;
  onOpenHistory: () => void;
  fill?: FillAsk | null;
  /** The composer has started on `fill`. */
  onFillTaken?: () => void;
}

/**
 * "Ghi logtime": a report for one or several days → Claude finds the parent
 * tasks → the user picks tasks and fixes parent, sub-task and hours → one
 * confirmed write to Redmine.
 */
const ReportComposer = forwardRef<ComposerHandle, Props>(function ReportComposer(
  { settings, onOpenSettings, onOpenHistory, fill, onFillTaken },
  ref,
) {
  const f = useReportFlow(null, settings);
  const { ai, view } = f;
  const toast = useToast();
  const box = useRef<HTMLTextAreaElement>(null);
  /** The Google Chat picker; `fill` when it stands in for fetching the short days. */
  const [importing, setImporting] = useState<{ from?: string; to?: string; fill?: boolean } | null>(null);
  const [filling, setFilling] = useState(false);
  /** Short days the fill found no message for. */
  const [uncovered, setUncovered] = useState<string[]>([]);
  const busy = ai.running || filling;

  /** The short days' messages replace the report text, then a quick match (no Claude quota). */
  const fillWith = (blocks: string[]) => {
    const text = blocks.join("\n\n");
    f.setText(text);
    f.quick(text);
  };

  // From the remembered group without asking; the picker for the same days when that cannot run.
  const fillTaken = useRef(0);
  useEffect(() => {
    if (!fill || !f.restored || fillTaken.current === fill.n) return;
    fillTaken.current = fill.n;
    onFillTaken?.();
    const r = fillRange(fill.dates, todayIso());
    setFilling(true);
    setUncovered([]);
    rangeChatMessages(r.from, r.to, r.keep)
      .then((list) => {
        if (!list) return setImporting({ from: r.from, to: r.to, fill: true });
        setUncovered(uncoveredDays(fill.dates, list.map(sentOn)));
        fillWith(chatBlocks(list));
      })
      .finally(() => setFilling(false));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [fill?.n, f.restored]);

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
          <button className="link" onClick={f.finish} disabled={busy}>
            Bỏ kết quả
          </button>
        )}
      </header>
      <p className="hint">
        Dán báo cáo tuần hoặc các báo cáo ngày. AI đọc Redmine (chỉ đọc) để tìm task cha, rồi app chia giờ cho đủ{" "}
        {fmtHours(settings.hours_per_day)} mỗi ngày. Việc đã log sẽ được bỏ qua. <strong>Khớp nhanh</strong> không cần
        AI, dùng các việc bạn đã ghi trước đây.
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
        disabled={busy}
        aria-label="Báo cáo công việc"
      />
      <div className="row composer-tools">
        <button className="btn primary" onClick={f.analyze} disabled={busy || !f.text.trim()}>
          {ai.running ? "Đang phân tích…" : view ? "Phân tích lại" : "Phân tích bằng AI"}
        </button>
        <button
          className="btn"
          onClick={() => f.quick()}
          disabled={busy || !f.text.trim()}
          title="Không dùng AI: khớp các dòng với những việc bạn đã ghi trước đây"
        >
          Khớp nhanh
        </button>
        <button className="btn" onClick={() => setImporting({})} disabled={busy}>
          Lấy từ Google Chat
        </button>
      </div>
      {filling && <p className="hint">Đang lấy tin Google Chat của các ngày thiếu giờ…</p>}
      {uncovered.length > 0 && (
        <p className="hint">
          Không có tin nhắn cho {uncovered.map(fmtDayShort).join(", ")}: nhập báo cáo các ngày này vào ô trên, hoặc ghi
          trên Lịch tháng.
        </p>
      )}
      <AiLog ai={ai} />

      {view && <ReportResult f={f} settings={settings} onOpenHistory={onOpenHistory} />}

      {importing && (
        <GchatImport
          from={importing.from}
          to={importing.to}
          onClose={() => setImporting(null)}
          onOpenSettings={() => {
            setImporting(null);
            onOpenSettings();
          }}
          onInsert={(blocks) => {
            setImporting(null);
            if (importing.fill) return fillWith(blocks);
            const { text, added } = appendBlocks(f.text, blocks);
            f.setText(text);
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
