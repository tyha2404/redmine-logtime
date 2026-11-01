import type { CSSProperties } from "react";

/**
 * Placeholders shown while data loads: grey blocks with a light sweep, laid
 * out like the content they stand for, so the page does not jump when it
 * arrives.
 */
export function Skel({ w = "100%", h = 14, r, style }: { w?: number | string; h?: number | string; r?: number; style?: CSSProperties }) {
  return <span className="skel" style={{ width: w, height: h, borderRadius: r, ...style }} aria-hidden />;
}

/** Text lines of varying length. */
export function SkelLines({ lines = 3, widths = ["92%", "76%", "84%", "60%"] }: { lines?: number; widths?: string[] }) {
  return (
    <span className="skel-lines" aria-hidden>
      {Array.from({ length: lines }, (_, i) => (
        <Skel key={i} w={widths[i % widths.length]} />
      ))}
    </span>
  );
}

const WEEKDAYS = ["T2", "T3", "T4", "T5", "T6", "T7", "CN"];

/** A month grid like the real one: weekday heads, the week column, `weeks` rows. */
export function CalendarSkeleton({ weeks = 5 }: { weeks?: number }) {
  return (
    <div
      className="grid is-skeleton"
      aria-busy="true"
      aria-label="Đang tải lịch tháng"
      style={{ "--weeks": weeks } as CSSProperties}
    >
      {WEEKDAYS.map((w) => (
        <div key={w} className="grid-head" aria-hidden>
          {w}
        </div>
      ))}
      <div className="grid-head week-head" aria-hidden>
        Tuần
      </div>
      {Array.from({ length: weeks * 8 }, (_, i) =>
        i % 8 === 7 ? (
          <span key={i} className="week-skel" aria-hidden>
            <Skel w={34} h={14} />
            <Skel w={28} h={10} />
            <Skel w={40} h={3} />
          </span>
        ) : (
          <span key={i} className="day day-skel" aria-hidden>
            <Skel w={18} h={16} />
            <Skel w={24} h={12} style={{ alignSelf: "flex-end" }} />
          </span>
        ),
      )}
    </div>
  );
}

/** Cards with a title and a few lines, e.g. Settings or the leave balances. */
export function CardsSkeleton({ cards = 4, className = "" }: { cards?: number; className?: string }) {
  return (
    <div className={`cards-skel ${className}`} aria-busy="true" aria-label="Đang tải">
      {Array.from({ length: cards }, (_, i) => (
        <section key={i} className="card">
          <Skel w="40%" h={18} style={{ marginBottom: 16 }} />
          <SkelLines lines={3} />
        </section>
      ))}
    </div>
  );
}

/** Rows of a list: a short lead, a long middle, a short end. */
export function RowsSkeleton({ rows = 4, className = "" }: { rows?: number; className?: string }) {
  return (
    <div className={`rows-skel ${className}`} aria-busy="true" aria-label="Đang tải">
      {Array.from({ length: rows }, (_, i) => (
        <span key={i} className="row-skel" aria-hidden>
          <Skel w={96} />
          <Skel w={`${55 + ((i * 17) % 30)}%`} />
          <Skel w={64} />
        </span>
      ))}
    </div>
  );
}

/** The calendar's day panel: date, status, hours, the three actions. */
export function PanelSkeleton() {
  return (
    <aside className="day-panel panel-skel" aria-busy="true" aria-label="Đang tải">
      <Skel w="60%" h={26} />
      <Skel w={72} h={20} r={999} />
      <Skel w="40%" h={24} />
      <Skel h={8} r={999} />
      <span className="panel-skel-actions">
        <Skel h={36} />
        <Skel h={36} />
        <Skel h={36} />
      </span>
    </aside>
  );
}

/** The app before its settings are read: the calendar page's layout. */
export function PageSkeleton() {
  const now = new Date();
  const leading = (new Date(now.getFullYear(), now.getMonth(), 1).getDay() + 6) % 7;
  const weeks = Math.ceil((leading + new Date(now.getFullYear(), now.getMonth() + 1, 0).getDate()) / 7);
  return (
    <div className="calendar-page" aria-busy="true" aria-label="Đang tải">
      <div className="page-head">
        <div className="page-head-text">
          <Skel w={150} h={31} />
          <Skel w={340} h={14} style={{ marginTop: 8 }} />
        </div>
        <div className="page-actions">
          <Skel w={340} h={36} r={10} />
        </div>
      </div>
      <section className="calendar-col">
        <header className="month-head">
          <div className="month-stats">
            <Skel w="auto" h={60} r={0} />
            <Skel w="auto" h={60} r={0} />
            <Skel w="auto" h={60} r={0} />
            <Skel w="auto" h={60} r={0} />
          </div>
        </header>
        <CalendarSkeleton weeks={weeks} />
        <div className="legend legend-skel">
          <Skel w={420} h={14} />
        </div>
      </section>
      <PanelSkeleton />
    </div>
  );
}
