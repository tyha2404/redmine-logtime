import { useMemo, useState } from "react";
import type { Settings } from "./api";
import { fmtDayLong, fmtHours } from "./fmt";
import { Icon } from "./icons";
import { fmtDays, leaveYear, shortLabel, useLeaveEntries, type LeaveBalance, type LeaveDay } from "./leave";
import type { DraftApi } from "./useDraft";
import IssueLink from "./IssueLink";

/**
 * A year of leave at a glance: for each kind, the allowance as a row of day
 * tanks (taken, waiting in the queue, left), then every day off by month.
 */
export default function LeavePage(props: {
  settings: Settings;
  draft: DraftApi;
  onOpenCalendar: () => void;
  onOpenSettings: () => void;
}) {
  const { settings, draft, onOpenCalendar, onOpenSettings } = props;
  const thisYear = new Date().getFullYear();
  const [year, setYear] = useState(thisYear);
  const [reloads, setReloads] = useState(0);
  const { entries, error } = useLeaveEntries(year, reloads);
  const data = useMemo(
    () => (entries ? leaveYear(settings, year, entries, draft.draft) : null),
    [entries, settings, year, draft.draft],
  );

  if (settings.leave_types.length === 0) {
    return (
      <div className="page-pad">
        <h1 className="page-title">Ngày nghỉ</h1>
        <div className="empty">
          <p>Chưa cài loại nghỉ. Vào Cài đặt, mục Nghỉ phép, bấm “Tìm trên Redmine”.</p>
          <button className="btn primary" onClick={onOpenSettings}>
            Mở cài đặt
          </button>
        </div>
      </div>
    );
  }

  const total = data?.days.filter((d) => !d.queued).reduce((s, d) => s + d.days, 0) ?? 0;

  return (
    <div className="page-pad leave-page">
      <header className="leave-head">
        <h1 className="page-title">Ngày nghỉ</h1>
        <div className="month-nav">
          <div className="nav-group" role="group" aria-label="Chọn năm">
            <button className="nav-btn icon-only" onClick={() => setYear(year - 1)} aria-label="Năm trước" title="Năm trước">
              <Icon name="chevronLeft" />
            </button>
            <span className="nav-btn leave-year" aria-live="polite">
              {year}
            </span>
            <button
              className="nav-btn icon-only"
              onClick={() => setYear(year + 1)}
              disabled={year >= thisYear + 1}
              aria-label="Năm sau"
              title="Năm sau"
            >
              <Icon name="chevronRight" />
            </button>
          </div>
          <button
            className={`nav-btn icon-only refresh${entries === null && !error ? " is-loading" : ""}`}
            onClick={() => setReloads((n) => n + 1)}
            aria-label="Tải lại từ Redmine"
            title="Tải lại từ Redmine"
          >
            <Icon name="refresh" />
          </button>
        </div>
      </header>

      {error ? (
        <p className="notice error">{error}</p>
      ) : !data ? (
        <p className="muted">Đang đọc ngày nghỉ năm {year} từ Redmine…</p>
      ) : (
        <>
          <p className="month-summary">
            {total > 0 ? (
              <>
                Năm {year} bạn đã nghỉ <strong>{fmtDays(total)} ngày</strong>
                {data.balances.filter((b) => b.used > 0).length > 1 &&
                  `: ${data.balances
                    .filter((b) => b.used > 0)
                    .map((b) => `${fmtDays(b.used)} ngày ${shortLabel(b.type.label).toLowerCase()}`)
                    .join(", ")}`}
                .
              </>
            ) : (
              `Năm ${year} chưa có ngày nghỉ nào trên Redmine.`
            )}{" "}
            Xin nghỉ: chọn ngày trên{" "}
            <button className="link" onClick={onOpenCalendar}>
              Lịch tháng
            </button>
            .
          </p>

          <div className="leave-balances">
            {data.balances.map((b) => (
              <BalanceCard key={b.type.project_id} b={b} />
            ))}
          </div>

          <h2 className="section-title">Từng ngày nghỉ</h2>
          {data.days.length === 0 ? (
            <p className="muted">Không có ngày nghỉ nào.</p>
          ) : (
            <DayList days={data.days} hoursPerDay={settings.hours_per_day} />
          )}
        </>
      )}
    </div>
  );
}

function BalanceCard({ b }: { b: LeaveBalance }) {
  const slots = Math.ceil(Math.max(b.allowance ?? 0, b.used + b.queued));
  const over = b.left !== null && b.left < 0;
  return (
    <section className="card leave-card" data-kind={b.type.key}>
      <h2>{b.type.label}</h2>
      <p className="leave-count">
        <strong>{fmtDays(b.used)}</strong>
        {b.allowance !== null && <span> / {fmtDays(b.allowance)} ngày</span>}
        {b.allowance === null && <span> ngày</span>}
      </p>
      {slots > 0 && (
        <div className="ltanks" role="img" aria-label={tankLabel(b)}>
          {Array.from({ length: slots }, (_, i) => {
            const used = clamp01(b.used - i);
            const queued = clamp01(b.used + b.queued - i) - used;
            const beyond = b.allowance !== null && i >= b.allowance;
            return (
              <span key={i} className={`ltank${beyond ? " is-over" : ""}`}>
                {used > 0 && <span className="ltank-used" style={{ height: `${used * 100}%` }} />}
                {queued > 0 && (
                  <span className="ltank-queued" style={{ bottom: `${used * 100}%`, height: `${queued * 100}%` }} />
                )}
              </span>
            );
          })}
        </div>
      )}
      <p className={`leave-left${over ? " is-over" : ""}`}>
        {b.left === null
          ? "Không giới hạn."
          : over
            ? `Vượt ${fmtDays(-b.left)} ngày.`
            : b.left === 0
              ? "Đã dùng hết."
              : `Còn ${fmtDays(b.left)} ngày.`}
        {b.queued > 0 && <span className="text-pending"> {fmtDays(b.queued)} ngày đang chờ ghi.</span>}
      </p>
    </section>
  );
}

function tankLabel(b: LeaveBalance): string {
  const parts = [`Đã nghỉ ${fmtDays(b.used)} ngày`];
  if (b.queued) parts.push(`${fmtDays(b.queued)} ngày chờ ghi`);
  if (b.allowance !== null) parts.push(`trên ${fmtDays(b.allowance)} ngày mỗi năm`);
  return parts.join(", ");
}

function clamp01(n: number): number {
  return Math.min(1, Math.max(0, n));
}

/** Days off by month, newest first. */
function DayList({ days, hoursPerDay }: { days: LeaveDay[]; hoursPerDay: number }) {
  const months = new Map<string, LeaveDay[]>();
  for (const d of [...days].reverse()) {
    const m = d.date.slice(0, 7);
    months.set(m, [...(months.get(m) ?? []), d]);
  }
  return (
    <div className="leave-months">
      {[...months].map(([m, list]) => (
        <section key={m} className="leave-month">
          <h3>Tháng {Number(m.slice(5))}</h3>
          <ul>
            {list.map((d) => (
              <li key={`${d.date}-${d.type.project_id}-${d.queued}`} className={d.queued ? "is-queued" : ""}>
                <span className="leave-date">{fmtDayLong(d.date)}</span>
                <span className="leave-kind" data-kind={d.type.key}>
                  {d.type.label}
                </span>
                <span className="leave-span">
                  {d.days >= 1 ? "Cả ngày" : "Nửa ngày"}
                  {Math.abs(d.hours - d.days * hoursPerDay) > 1e-6 && <span className="muted"> ({fmtHours(d.hours)})</span>}
                </span>
                <span className="leave-ref muted">{d.queued ? "Đang chờ ghi" : d.issueId ? <IssueLink id={d.issueId} /> : ""}</span>
              </li>
            ))}
          </ul>
        </section>
      ))}
    </div>
  );
}
