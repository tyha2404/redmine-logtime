import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type KeyboardEvent } from "react";
import { fmtDayShort, isoOf, parseIso, todayIso } from "./fmt";
import { Icon } from "./icons";

const WEEK_HEAD = ["T2", "T3", "T4", "T5", "T6", "T7", "CN"];

function addDays(iso: string, n: number): string {
  const d = parseIso(iso);
  d.setDate(d.getDate() + n);
  return isoOf(d);
}

function daysBetween(a: string, b: string): number {
  return Math.round((parseIso(b).getTime() - parseIso(a).getTime()) / 86_400_000);
}

/** Monday of the week containing `iso`. */
function mondayOf(iso: string): string {
  return addDays(iso, -((parseIso(iso).getDay() + 6) % 7));
}

function monthStart(iso: string, delta = 0): string {
  const d = parseIso(iso);
  return isoOf(new Date(d.getFullYear(), d.getMonth() + delta, 1));
}

function monthEnd(iso: string): string {
  const d = parseIso(iso);
  return isoOf(new Date(d.getFullYear(), d.getMonth() + 1, 0));
}

/** "23/10 – 29/10/2026", with the year once when both ends share it. */
export function fmtRange(from: string, to: string): string {
  const [fy, ty] = [from.slice(0, 4), to.slice(0, 4)];
  const left = fy === ty ? fmtDayShort(from) : `${fmtDayShort(from)}/${fy}`;
  return from === to ? `${fmtDayShort(to)}/${ty}` : `${left} – ${fmtDayShort(to)}/${ty}`;
}

function presets(today: string): [string, string, string][] {
  const monday = mondayOf(today);
  return [
    ["7 ngày qua", addDays(today, -6), today],
    ["Tuần này", monday, today],
    ["Tuần trước", addDays(monday, -7), addDays(monday, -1)],
    ["Tháng này", monthStart(today), today],
    ["Tháng trước", monthStart(today, -1), monthEnd(monthStart(today, -1))],
  ];
}

interface Props {
  from: string;
  to: string;
  onChange: (from: string, to: string) => void;
  /** Longest range allowed, in days. */
  maxDays: number;
  disabled?: boolean;
}

const GAP = 6;
const EDGE = 8;

/**
 * A date range as one field; it opens a popover with quick ranges and a
 * month grid like the calendar page. The popover is `position: fixed` so a
 * scrolling modal dialog around it cannot clip it.
 */
export default function DateRangePicker({ from, to, onChange, maxDays, disabled }: Props) {
  const today = todayIso();
  const [open, setOpen] = useState(false);
  const [month, setMonth] = useState(() => monthStart(to));
  /** First click of a custom range, waiting for the second. */
  const [anchor, setAnchor] = useState<string | null>(null);
  const [hover, setHover] = useState<string | null>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<CSSProperties>({ visibility: "hidden" });

  const close = () => {
    setOpen(false);
    setAnchor(null);
  };

  // Below the field, or above it when the window has no room; kept on screen.
  const place = () => {
    const t = triggerRef.current?.getBoundingClientRect();
    const panel = panelRef.current;
    if (!t || !panel) return;
    const { offsetWidth: w, offsetHeight: h } = panel;
    const left = Math.max(EDGE, Math.min(t.left, window.innerWidth - w - EDGE));
    const below = t.bottom + GAP;
    const above = t.top - GAP - h;
    const bottom = window.innerHeight - EDGE;
    // With room on neither side, slide up just enough to stay fully visible.
    const top = below + h <= bottom ? below : above >= EDGE ? above : Math.max(EDGE, bottom - h);
    setPos({ left, top });
  };

  useLayoutEffect(() => {
    if (open) place();
    else setPos({ visibility: "hidden" });
  }, [open, month]);

  useEffect(() => {
    if (!open) return;
    const outside = (e: PointerEvent) => {
      const target = e.target as Node;
      if (!panelRef.current?.contains(target) && !triggerRef.current?.contains(target)) close();
    };
    document.addEventListener("pointerdown", outside, true);
    window.addEventListener("resize", place);
    window.addEventListener("scroll", place, true);
    return () => {
      document.removeEventListener("pointerdown", outside, true);
      window.removeEventListener("resize", place);
      window.removeEventListener("scroll", place, true);
    };
  }, [open]);

  const choose = (a: string, b: string) => {
    onChange(a, b);
    setAnchor(null);
    setOpen(false);
  };

  const toggle = () => {
    setAnchor(null);
    setMonth(monthStart(to));
    setOpen((o) => !o);
  };

  const pick = (day: string) => {
    if (!anchor) {
      setAnchor(day);
      return;
    }
    choose(day < anchor ? day : anchor, day < anchor ? anchor : day);
  };

  // While picking, show the range up to the hovered day.
  const [lo, hi] = anchor
    ? [anchor, hover ?? anchor].sort()
    : [from, to];
  const reachable = (day: string) =>
    day <= today && (!anchor || Math.abs(daysBetween(anchor, day)) < maxDays);

  const first = parseIso(month);
  const lead = (first.getDay() + 6) % 7;
  const cells: (string | null)[] = [
    ...Array<null>(lead).fill(null),
    ...Array.from({ length: parseIso(monthEnd(month)).getDate() }, (_, i) => addDays(month, i)),
  ];
  const nextMonth = monthStart(month, 1);

  const onKeyDown = (e: KeyboardEvent) => {
    // Esc closes the picker first, not the dialog around it.
    if (e.key === "Escape" && open) {
      e.preventDefault();
      e.stopPropagation();
      close();
      triggerRef.current?.focus();
    }
  };

  const span = daysBetween(from, to) + 1;

  return (
    <div className="range" onKeyDown={onKeyDown}>
      <button
        ref={triggerRef}
        type="button"
        className="range-trigger"
        aria-expanded={open}
        aria-label={`Khoảng ngày: ${fmtRange(from, to)}`}
        onClick={toggle}
        disabled={disabled}
      >
        <Icon name="calendar" size={17} />
        <span className="range-value">{fmtRange(from, to)}</span>
        <span className="range-span">{span} ngày</span>
      </button>

      {open && (
        <div ref={panelRef} className="range-panel" role="dialog" aria-label="Chọn khoảng ngày" style={pos}>
          <div className="range-presets" role="group" aria-label="Khoảng nhanh">
            {presets(today).map(([label, a, b]) => (
              <button
                key={label}
                type="button"
                className="range-preset"
                aria-pressed={!anchor && a === from && b === to}
                onClick={() => choose(a, b)}
              >
                {label}
              </button>
            ))}
          </div>

          <div className="range-cal">
            <div className="range-cal-head">
              <button
                type="button"
                className="nav-btn icon-only"
                aria-label="Tháng trước"
                onClick={() => setMonth(monthStart(month, -1))}
              >
                <Icon name="chevronLeft" size={16} />
              </button>
              <span className="range-cal-title">
                Tháng {first.getMonth() + 1}, {first.getFullYear()}
              </span>
              <button
                type="button"
                className="nav-btn icon-only"
                aria-label="Tháng sau"
                onClick={() => setMonth(nextMonth)}
                disabled={nextMonth > today}
              >
                <Icon name="chevronRight" size={16} />
              </button>
            </div>
            <div className="range-grid" role="grid" onMouseLeave={() => setHover(null)}>
              {WEEK_HEAD.map((w) => (
                <span key={w} className="range-dow">
                  {w}
                </span>
              ))}
              {cells.map((day, i) =>
                day ? (
                  <button
                    key={day}
                    type="button"
                    className={[
                      "range-day",
                      day >= lo && day <= hi ? "in" : "",
                      day === lo ? "lo" : "",
                      day === hi ? "hi" : "",
                      day === today ? "today" : "",
                    ].join(" ")}
                    disabled={!reachable(day)}
                    onClick={() => pick(day)}
                    onMouseEnter={() => setHover(day)}
                    aria-label={day}
                  >
                    {parseIso(day).getDate()}
                  </button>
                ) : (
                  <span key={`pad-${i}`} />
                ),
              )}
            </div>
            <p className="range-hint">
              {anchor
                ? `Từ ${fmtDayShort(anchor)}, chọn ngày kết thúc (tối đa ${maxDays} ngày).`
                : "Chọn ngày bắt đầu, rồi ngày kết thúc."}
            </p>
          </div>
        </div>
      )}
    </div>
  );
}
