import { useCallback, useEffect, useMemo, useState } from "react";
import AddEntryForm from "./AddEntryForm";
import AiLog from "./AiLog";
import LeaveForm from "./LeaveForm";
import { api, errorText, type DayStatus, type DaySummary, type MonthSummary, type PlanItem, type Settings } from "./api";
import { fmtDayLong, fmtDayShort, fmtHours, isoOf, parseIso, targetLabel, todayIso } from "./fmt";
import { Icon } from "./icons";
import IssueLink from "./IssueLink";
import QuotaRail from "./QuotaRail";
import { summaryLine, TaskRow, useReportFlow } from "./reportFlow";
import { useToast } from "./toast";
import type { DraftApi } from "./useDraft";

const WEEKDAYS = ["T2", "T3", "T4", "T5", "T6", "T7", "CN"];

export const STATUS_LABEL: Record<DayStatus, string> = {
  full: "Đủ giờ",
  partial: "Thiếu giờ",
  missing: "Chưa log",
  over: "Vượt giờ",
  leave: "Nghỉ phép",
  weekend: "Ngày nghỉ",
  holiday: "Ngày lễ",
  future: "Chưa tới",
};

const LEGEND: DayStatus[] = ["full", "partial", "missing", "over", "leave"];

interface Props {
  settings: Settings;
  draft: DraftApi;
  onOpenSettings: () => void;
  onOpenReview: () => void;
  /** Opens the report page (one or several days). */
  onOpenReport: () => void;
}

export default function CalendarPage({ settings, draft, onOpenSettings, onOpenReview, onOpenReport }: Props) {
  const now = new Date();
  const [ym, setYm] = useState({ year: now.getFullYear(), month: now.getMonth() + 1 });
  const [summary, setSummary] = useState<MonthSummary | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [selected, setSelected] = useState<string>(todayIso());
  const hpd = settings.hours_per_day;

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setSummary(await api.monthSummary(ym.year, ym.month));
    } catch (e) {
      setSummary(null);
      setError(errorText(e));
    } finally {
      setLoading(false);
    }
  }, [ym]);

  useEffect(() => {
    load();
  }, [load]);

  const plannedByDate = useMemo(() => {
    const m = new Map<string, PlanItem[]>();
    for (const i of draft.draft.items) m.set(i.date, [...(m.get(i.date) ?? []), i]);
    return m;
  }, [draft.draft.items]);

  const shift = (delta: number) => {
    setYm(({ year, month }) => {
      const d = new Date(year, month - 1 + delta, 1);
      return { year: d.getFullYear(), month: d.getMonth() + 1 };
    });
  };

  const goToday = () => {
    setYm({ year: now.getFullYear(), month: now.getMonth() + 1 });
    setSelected(todayIso());
  };

  // Arrow keys move the selection like a desktop calendar; crossing the
  // month edge flips the month.
  const onGridKey = (e: React.KeyboardEvent) => {
    const step = { ArrowLeft: -1, ArrowRight: 1, ArrowUp: -7, ArrowDown: 7 }[e.key];
    if (!step) return;
    e.preventDefault();
    const d = parseIso(selected);
    d.setDate(d.getDate() + step);
    const next = isoOf(d);
    setSelected(next);
    if (d.getMonth() + 1 !== ym.month || d.getFullYear() !== ym.year) {
      setYm({ year: d.getFullYear(), month: d.getMonth() + 1 });
    }
    requestAnimationFrame(() => document.querySelector<HTMLElement>(`[data-date="${next}"]`)?.focus());
  };

  // Monday-first grid: blank cells before the 1st, then the month in weeks.
  const leading = useMemo(() => (new Date(ym.year, ym.month - 1, 1).getDay() + 6) % 7, [ym]);
  const weeks = useMemo(() => {
    if (!summary) return [];
    const cells: (DaySummary | null)[] = [...Array(leading).fill(null), ...summary.days];
    while (cells.length % 7) cells.push(null);
    return Array.from({ length: cells.length / 7 }, (_, i) => cells.slice(i * 7, i * 7 + 7));
  }, [summary, leading]);
  const selectedDay = summary?.days.find((d) => d.date === selected) ?? null;
  const today = todayIso();
  const isCurrentMonth = ym.year === now.getFullYear() && ym.month === now.getMonth() + 1;
  const pendingCount = draft.draft.items.length;

  return (
    <div className="calendar-page">
      <section className="calendar-col">
        <header className="month-head">
          <h1 className="month-title" aria-label={`Tháng ${ym.month} năm ${ym.year}`}>
            Tháng {ym.month}
            <span className="month-year">{ym.year}</span>
          </h1>
          <div className="month-nav">
            <div className="nav-group" role="group" aria-label="Chọn tháng">
              <button className="nav-btn icon-only" onClick={() => shift(-1)} aria-label="Tháng trước" title="Tháng trước">
                <Icon name="chevronLeft" />
              </button>
              <button className="nav-btn" onClick={goToday} disabled={isCurrentMonth && selected === today}>
                Hôm nay
              </button>
              <button className="nav-btn icon-only" onClick={() => shift(1)} aria-label="Tháng sau" title="Tháng sau">
                <Icon name="chevronRight" />
              </button>
            </div>
            <button
              className={`nav-btn icon-only refresh${loading ? " is-loading" : ""}`}
              onClick={load}
              disabled={loading}
              aria-label={loading ? "Đang tải lại từ Redmine" : "Tải lại từ Redmine"}
              title="Tải lại từ Redmine"
            >
              <Icon name="refresh" />
            </button>
          </div>
        </header>

        {error ? (
          <div className="empty">
            <p>{error}</p>
            <button className="btn primary" onClick={onOpenSettings}>
              Mở cài đặt
            </button>
          </div>
        ) : (
          <>
            <p className="month-summary">
              {summary ? <MonthLine s={summary} onOpenReport={onOpenReport} /> : "Đang tải dữ liệu từ Redmine…"}
              {pendingCount > 0 && (
                <>
                  {" "}
                  <button className="link" onClick={onOpenReview}>
                    {pendingCount} mục đang chờ ghi
                  </button>
                  .
                </>
              )}
            </p>

            <div
              className="grid"
              role="grid"
              aria-busy={loading}
              onKeyDown={onGridKey}
            >
              {WEEKDAYS.map((w) => (
                <div key={w} className="grid-head" role="columnheader">
                  {w}
                </div>
              ))}
              <div className="grid-head week-head" role="columnheader">
                Tuần
              </div>
              {weeks.map((week, wi) => (
                <WeekRow
                  key={wi}
                  week={week}
                  plannedByDate={plannedByDate}
                  hoursPerDay={hpd}
                  today={today}
                  selected={selected}
                  onSelect={setSelected}
                />
              ))}
            </div>

            <ul className="legend">
              {LEGEND.map((s) => (
                <li key={s}>
                  <span className={`swatch s-${s}`} aria-hidden />
                  {STATUS_LABEL[s]}
                </li>
              ))}
              <li>
                <span className="swatch pending" aria-hidden />
                Chờ ghi
              </li>
            </ul>
          </>
        )}
      </section>

      <DayPanel
        key={selected}
        day={selectedDay}
        planned={plannedByDate.get(selected) ?? []}
        settings={settings}
        draft={draft}
        onOpenSettings={onOpenSettings}
        onOpenReview={onOpenReview}
      />
    </div>
  );
}

function sumHours(items: { hours: number }[] | undefined): number {
  return (items ?? []).reduce((s, i) => s + i.hours, 0);
}

function WeekRow(props: {
  week: (DaySummary | null)[];
  plannedByDate: Map<string, PlanItem[]>;
  hoursPerDay: number;
  today: string;
  selected: string;
  onSelect: (date: string) => void;
}) {
  const { week, plannedByDate, hoursPerDay, today, selected, onSelect } = props;
  const days = week.filter((d): d is DaySummary => !!d);
  const logged = days.reduce((s, d) => s + d.hours, 0);
  const planned = days.reduce((s, d) => s + sumHours(plannedByDate.get(d.date)), 0);
  // Quota for this week's workdays inside the month; days off and holidays don't count.
  const expected = days.filter((d) => d.status !== "weekend" && d.status !== "holiday").length * hoursPerDay;
  const short = days.some((d) => d.status === "missing" || d.status === "partial");
  const tone = short ? "is-short" : expected > 0 && logged >= expected ? "is-full" : "";

  return (
    <>
      {week.map((day, i) =>
        day ? (
          <DayCell
            key={day.date}
            day={day}
            planned={sumHours(plannedByDate.get(day.date))}
            hoursPerDay={hoursPerDay}
            isToday={day.date === today}
            isSelected={day.date === selected}
            onSelect={() => onSelect(day.date)}
          />
        ) : (
          <div key={`b${i}`} aria-hidden />
        ),
      )}
      <div
        className={`week-sum ${tone}`}
        role="gridcell"
        aria-label={`Tuần: ${fmtHours(logged)} trên ${fmtHours(expected)}${planned ? `, ${fmtHours(planned)} chờ ghi` : ""}`}
      >
        {expected > 0 && (
          <>
            <span className="week-logged">{fmtHours(logged)}</span>
            <span className="week-expected">/ {fmtHours(expected)}</span>
            {planned > 0 && <span className="week-plus">+{fmtHours(planned)}</span>}
            <span className="week-bar" aria-hidden>
              <span style={{ width: `${Math.min(logged / expected, 1) * 100}%` }} />
            </span>
          </>
        )}
      </div>
    </>
  );
}

function MonthLine({ s, onOpenReport }: { s: MonthSummary; onOpenReport: () => void }) {
  return (
    <>
      Đã log <strong>{fmtHours(s.total_hours)}</strong> trên {fmtHours(s.expected_hours)} của tháng.{" "}
      {s.missing_days > 0 ? (
        <>
          Tính đến hôm nay còn thiếu <strong className="text-missing">{fmtHours(s.missing_hours)}</strong> ở{" "}
          {s.missing_days} ngày.{" "}
          <button className="link" onClick={onOpenReport}>
            Lấp bằng báo cáo công việc
          </button>
          .
        </>
      ) : (
        "Tính đến hôm nay không thiếu ngày nào."
      )}
    </>
  );
}

function DayCell(props: {
  day: DaySummary;
  planned: number;
  hoursPerDay: number;
  isToday: boolean;
  isSelected: boolean;
  onSelect: () => void;
}) {
  const { day, planned, hoursPerDay, isToday, isSelected, onSelect } = props;
  const logged = Math.min(day.hours / hoursPerDay, 1) * 100;
  const withPlan = Math.min((day.hours + planned) / hoursPerDay, 1) * 100;
  const dayNum = Number(day.date.slice(8));
  const off = day.status === "weekend" || day.status === "holiday";
  const showHours = day.hours > 0 || day.status === "missing" || day.status === "partial";

  return (
    <button
      role="gridcell"
      data-date={day.date}
      tabIndex={isSelected ? 0 : -1}
      className={`day s-${day.status}${off ? " off" : ""}${isToday ? " today" : ""}`}
      aria-selected={isSelected}
      aria-label={`Ngày ${dayNum}: ${fmtHours(day.hours)}, ${STATUS_LABEL[day.status]}${planned ? `, ${fmtHours(planned)} chờ ghi` : ""}`}
      onClick={onSelect}
    >
      {day.hours > 0 && <span className="day-fill" style={{ height: `${logged}%` }} aria-hidden />}
      {planned > 0 && (
        <span className="day-plan" style={{ bottom: `${logged}%`, height: `${Math.max(withPlan - logged, 6)}%` }} aria-hidden />
      )}
      <span className="day-top">
        <span className="day-num">{dayNum}</span>
        {day.status === "holiday" && <span className="day-tag">Lễ</span>}
      </span>
      <span className="day-hours">
        {showHours && <span className="day-logged">{fmtHours(day.hours)}</span>}
        {planned > 0 && <span className="day-plus">+{fmtHours(planned)}</span>}
      </span>
    </button>
  );
}


type PanelMode = "idle" | "report" | "add" | "leave";

function DayPanel(props: {
  day: DaySummary | null;
  planned: PlanItem[];
  settings: Settings;
  draft: DraftApi;
  onOpenSettings: () => void;
  onOpenReview: () => void;
}) {
  const { day, planned, settings, draft, onOpenSettings, onOpenReview } = props;
  const toast = useToast();
  const [mode, setMode] = useState<PanelMode>(() =>
    // Browser mock mode can open a composer directly: ?panel=report
    import.meta.env.VITE_MOCK === "1" ? ((new URLSearchParams(location.search).get("panel") as PanelMode) ?? "idle") : "idle",
  );

  if (!day) {
    return (
      <aside className="day-panel">
        <p className="muted">Chọn một ngày trên lịch để xem và thêm logtime.</p>
      </aside>
    );
  }

  const hpd = settings.hours_per_day;
  const plannedHours = sumHours(planned);
  const remaining = Math.max(hpd - day.hours - plannedHours, 0);
  const off = day.status === "weekend" || day.status === "holiday";

  const removeItem = async (item: PlanItem) => {
    const removed = await draft.remove([item.id]);
    toast(`Đã bỏ ${fmtHours(item.hours)} ngày ${fmtDayShort(item.date)} khỏi hàng chờ.`, {
      action: { label: "Hoàn tác", run: () => draft.restore(removed) },
    });
  };


  return (
    <aside className="day-panel">
      <header className="panel-head">
        <h2 className="panel-title">{fmtDayLong(day.date)}</h2>
        <p className={`status-pill s-${day.status}`}>{STATUS_LABEL[day.status]}</p>
        <div className="panel-meter">
          <div className="panel-numbers">
            <span>
              <strong>{fmtHours(day.hours)}</strong> / {fmtHours(hpd)}
              {plannedHours > 0 && <span className="text-pending"> +{fmtHours(plannedHours)} chờ ghi</span>}
            </span>
            {remaining > 0 && !off && <span className="muted">còn trống {fmtHours(remaining)}</span>}
          </div>
          <QuotaRail existing={day.hours} planned={plannedHours} quota={hpd} />
        </div>
      </header>

      {mode === "report" && (
        <DayReport date={day.date} settings={settings} draft={draft} onClose={() => setMode("idle")} />
      )}

      {mode === "add" && (
        <section className="panel-block">
          <header className="block-head">
            <h3>Thêm logtime vào issue</h3>
            <button className="link" onClick={() => setMode("idle")}>
              Đóng
            </button>
          </header>
          <AddEntryForm
            date={day.date}
            settings={settings}
            remainingHours={remaining}
            onAdd={async (items, subtasks) => {
              await draft.add(items, subtasks);
              setMode("idle");
              toast(`Đã thêm ${fmtHours(sumHours(items))} vào hàng chờ.`, {
                action: { label: "Hoàn tác", run: () => draft.remove(items.map((i) => i.id)) },
              });
            }}
            onCancel={() => setMode("idle")}
          />
        </section>
      )}

      {mode === "leave" && (
        <section className="panel-block">
          <header className="block-head">
            <h3>Xin nghỉ ngày {fmtDayShort(day.date)}</h3>
            <button className="link" onClick={() => setMode("idle")}>
              Đóng
            </button>
          </header>
          <LeaveForm
            date={day.date}
            settings={settings}
            draft={draft.draft}
            onOpenSettings={onOpenSettings}
            onCancel={() => setMode("idle")}
            onAdd={async (item, subtask) => {
              await draft.add([item], [subtask]);
              setMode("idle");
              const kind = settings.leave_types.find((t) => t.project_id === subtask.project_id)?.label ?? "Nghỉ";
              toast(`Đã thêm ${fmtHours(item.hours)} ${kind.toLowerCase()} ngày ${fmtDayShort(day.date)} vào hàng chờ.`, {
                action: { label: "Hoàn tác", run: () => draft.remove([item.id]) },
              });
            }}
          />
        </section>
      )}

      {mode === "idle" && (
        <section className="panel-block panel-actions">
          <button className="btn primary" onClick={() => setMode("report")}>
            <Icon name="report" size={16} />
            Nhập báo cáo ngày này
          </button>
          <button className="btn" onClick={() => setMode("add")}>
            <Icon name="plus" size={16} />
            Thêm logtime vào issue
          </button>
          <button className="btn" onClick={() => setMode("leave")}>
            <Icon name="leave" size={16} />
            Xin nghỉ ngày này
          </button>
        </section>
      )}

      {planned.length > 0 && (
        <section className="panel-block">
          <header className="block-head">
            <h3 className="text-pending">Chờ ghi</h3>
            <button className="link" onClick={onOpenReview}>
              Xem hàng chờ
            </button>
          </header>
          <ul className="entries">
            {planned.map((i) => (
              <li key={i.id} className="entry pending-entry">
                <span className="entry-hours">{fmtHours(i.hours)}</span>
                <div className="entry-body">
                  <div className="entry-title">{targetLabel(i, draft.draft)}</div>
                  {i.comments && <div className="entry-comment">{i.comments}</div>}
                  {i.last_error && <div className="entry-error">Lần ghi trước lỗi: {i.last_error}</div>}
                </div>
                <button className="icon-btn" onClick={() => removeItem(i)} aria-label="Bỏ khỏi hàng chờ" title="Bỏ khỏi hàng chờ">
                  <Icon name="close" size={16} />
                </button>
              </li>
            ))}
          </ul>
        </section>
      )}

      <section className="panel-block">
        <header className="block-head">
          <h3>Đã log trên Redmine</h3>
        </header>
        {day.entries.length === 0 ? (
          <p className="muted">Chưa có logtime ngày này.</p>
        ) : (
          <ul className="entries">
            {day.entries.map((e) => (
              <li key={e.id} className="entry">
                <span className="entry-hours">{fmtHours(e.hours)}</span>
                <div className="entry-body">
                  <div className="entry-title">
                    {e.issue ? (
                      <span className="issue-ref">
                        <IssueLink id={e.issue.id} />
                      </span>
                    ) : null}
                    {e.project.name}
                  </div>
                  <div className="entry-meta">{e.activity.name}</div>
                  {e.comments ? <div className="entry-comment">{e.comments}</div> : null}
                </div>
              </li>
            ))}
          </ul>
        )}
      </section>
    </aside>
  );
}

/** The selected day's report, analysed and queued without leaving the calendar. */
function DayReport(props: { date: string; settings: Settings; draft: DraftApi; onClose: () => void }) {
  const { date, settings, draft, onClose } = props;
  const f = useReportFlow(date, settings, draft);
  const [queued, setQueued] = useState<number | null>(null);
  const { ai, view } = f;

  const queue = async () => {
    const n = await f.queue();
    if (n) setQueued(n);
  };

  return (
    <section className="panel-block day-report">
      <header className="block-head">
        <h3>Báo cáo ngày {fmtDayShort(date)}</h3>
        <button className="link" onClick={onClose} disabled={ai.running}>
          Đóng
        </button>
      </header>
      <textarea
        className="day-report-text"
        rows={4}
        value={f.text}
        onChange={(e) => {
          f.setText(e.target.value);
          setQueued(null);
        }}
        placeholder={"- Sửa feedback công nợ\n- Review Module Sự kiện"}
        disabled={ai.running}
        aria-label={`Báo cáo ngày ${fmtDayShort(date)}`}
      />
      <p className="hint">
        Mỗi dòng một việc. Việc đã log hoặc đang chờ ghi của ngày này sẽ tự bỏ qua.
      </p>
      <button className="btn primary" onClick={f.analyze} disabled={ai.running || !f.text.trim()}>
        {ai.running ? "Đang phân tích…" : view ? "Phân tích lại" : "Phân tích bằng AI"}
      </button>
      <AiLog ai={ai} />

      {view && (
        <>
          <p className="day-report-sum">{summaryLine(f)}</p>
          {view.analysis.questions.length > 0 && (
            <div className="ai-questions">
              <ul>
                {view.analysis.questions.map((q) => (
                  <li key={q}>{q}</li>
                ))}
              </ul>
            </div>
          )}
          <ul className="rtasks compact">
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
          {(f.blocker || f.notice) && <p className="notice error">{f.notice ?? f.blocker}</p>}
          {queued ? (
            <p className="notice ok">Đã thêm {queued} logtime vào hàng chờ.</p>
          ) : (
            <button className="btn primary" disabled={!!f.blocker || ai.running} onClick={queue}>
              Thêm {fmtHours(f.totalHours)} vào hàng chờ
            </button>
          )}
        </>
      )}
    </section>
  );
}
