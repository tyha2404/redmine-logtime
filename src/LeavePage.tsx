import { useMemo, useState } from "react";
import { CardsSkeleton, RowsSkeleton } from "./Skeleton";
import type { Settings } from "./api";
import { fmtDayLong, fmtHours } from "./fmt";
import { Icon } from "./icons";
import PageHeader, { RefreshButton } from "./PageHeader";
import { fmtDays, leaveYear, useLeaveEntries, type LeaveBalance, type LeaveDay } from "./leave";

/**
 * A year of leave at a glance: for each kind, the allowance as a row of day
 * tanks (taken, left), then every day off by month.
 */
export default function LeavePage(props: { settings: Settings; onOpenSettings: () => void }) {
  const { settings, onOpenSettings } = props;
  const thisYear = new Date().getFullYear();
  const [year, setYear] = useState(thisYear);
  const [reloads, setReloads] = useState(0);
  const { entries, error, loading } = useLeaveEntries(year, reloads);
  const data = useMemo(() => (entries ? leaveYear(settings, year, entries) : null), [entries, settings, year]);

  if (settings.leave_types.length === 0) {
    return (
      <div className="page-pad">
        <PageHeader title="Ngày nghỉ" />
        <div className="empty">
          <p>Chưa cài loại nghỉ. Vào Cài đặt, mục Nghỉ phép, bấm “Tìm trên Redmine”.</p>
          <button className="btn primary" onClick={onOpenSettings}>
            Mở cài đặt
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="page-pad leave-page">
      <PageHeader
        title="Ngày nghỉ"
        description="Số ngày đã nghỉ và quỹ phép còn lại trong năm. Muốn xin nghỉ, chọn ngày trên Lịch tháng."
        actions={
          <>
            <div className="nav-group" role="group" aria-label="Chọn năm">
              <button
                className="nav-btn icon-only"
                onClick={() => setYear(year - 1)}
                aria-label="Năm trước"
                title="Năm trước"
              >
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
            <RefreshButton loading={loading} onClick={() => setReloads((n) => n + 1)} />
          </>
        }
      />

      {error ? (
        <p className="notice error">{error}</p>
      ) : !data ? (
        <>
          <CardsSkeleton cards={3} className="is-leave" />
          <RowsSkeleton rows={4} />
        </>
      ) : (
        <>
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
  const slots = Math.ceil(Math.max(b.allowance ?? 0, b.used));
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
            const beyond = b.allowance !== null && i >= b.allowance;
            return (
              <span key={i} className={`ltank${beyond ? " is-over" : ""}`}>
                {used > 0 && <span className="ltank-used" style={{ height: `${used * 100}%` }} />}
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
      </p>
    </section>
  );
}

function tankLabel(b: LeaveBalance): string {
  const parts = [`Đã nghỉ ${fmtDays(b.used)} ngày`];
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
              <li key={`${d.date}-${d.type.project_id}`}>
                <span className="leave-date">{fmtDayLong(d.date)}</span>
                <span className="leave-kind" data-kind={d.type.key}>
                  {d.type.label}
                </span>
                <span className="leave-span">
                  {d.days >= 1 ? "Cả ngày" : "Nửa ngày"}
                  {Math.abs(d.hours - d.days * hoursPerDay) > 1e-6 && (
                    <span className="muted"> ({fmtHours(d.hours)})</span>
                  )}
                </span>
                <span className="leave-ref muted">{d.issueId ? `#${d.issueId}` : ""}</span>
              </li>
            ))}
          </ul>
        </section>
      ))}
    </div>
  );
}
