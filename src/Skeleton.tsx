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

/** A month grid: seven day columns and the week total, five weeks. */
export function CalendarSkeleton() {
  return (
    <div className="grid is-skeleton" aria-busy="true" aria-label="Đang tải lịch tháng">
      {Array.from({ length: 5 * 8 }, (_, i) =>
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
  return (
    <div className="calendar-page" aria-busy="true" aria-label="Đang tải">
      <section className="calendar-col">
        <Skel w={260} h={40} style={{ marginBottom: 16 }} />
        <Skel w="62%" h={16} style={{ marginBottom: 26 }} />
        <CalendarSkeleton />
      </section>
      <PanelSkeleton />
    </div>
  );
}
