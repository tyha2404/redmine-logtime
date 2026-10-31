import { fmtHours } from "./fmt";

/**
 * A day as a horizontal tank: solid = already on Redmine, hatched = waiting
 * to be written, with a tick at the daily quota. Anything past the tick is
 * overflow.
 */
export default function QuotaRail({ existing, planned, quota }: { existing: number; planned: number; quota: number }) {
  const total = existing + planned;
  const scale = Math.max(total, quota) || 1;
  const pct = (h: number) => `${(h / scale) * 100}%`;
  const over = total > quota + 1e-6;

  return (
    <div
      className="rail-bar"
      role="img"
      aria-label={`Đã có ${fmtHours(existing)}, chờ ghi ${fmtHours(planned)}, định mức ${fmtHours(quota)}`}
    >
      <span className="rail-seg existing" style={{ width: pct(existing) }} />
      <span className="rail-seg planned" style={{ left: pct(existing), width: pct(planned) }} />
      {over && <span className="rail-over" style={{ left: pct(quota), width: pct(total - quota) }} />}
      <span className="rail-tick" style={{ left: pct(quota) }} />
    </div>
  );
}
