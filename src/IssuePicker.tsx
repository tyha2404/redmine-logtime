import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { api, errorText, issueBrief, type IssueBrief } from "./api";
import { Icon } from "./icons";
import { RowsSkeleton } from "./Skeleton";
import { exitDelay } from "./useModal";

interface Props {
  value: IssueBrief | null;
  onPick: (issue: IssueBrief) => void;
  label: string;
  disabled?: boolean;
  /** The list, shown first. */
  options?: IssueBrief[];
  /** More of the list, loaded the first time it opens. */
  load?: () => Promise<IssueBrief[]>;
  /** Typed text also searches open issues on Redmine. */
  remote?: boolean;
  /** A first row that is not an issue, e.g. "Tạo task con mới"; `chosen` when it is the current value. */
  extra?: { label: string; chosen: boolean; onPick: () => void };
  /** The field's text when nothing is chosen. */
  empty?: string;
}

/** Lower case without Vietnamese accents, so "cong no" finds "Công nợ". */
function fold(s: string): string {
  return s.normalize("NFD").replace(/[̀-ͯ]/g, "").replace(/đ/gi, "d").toLowerCase();
}

/** `#12` or `12` matches ids starting with 12; words match the subject in any order. */
function matches(issue: IssueBrief, q: string): boolean {
  const id = q.replace(/^#/, "");
  if (/^\d+$/.test(id) && String(issue.id).startsWith(id)) return true;
  const subject = fold(`${issue.subject} ${issue.project_name}`);
  return fold(q)
    .split(/\s+/)
    .filter(Boolean)
    .every((w) => subject.includes(w));
}

/** "Task · New, Dự án ABC"; parts the app does not know (an entry's issue) are left out. */
function issueMeta(i: IssueBrief, status: boolean): string {
  const kind = [i.tracker, status ? i.status : ""].filter(Boolean).join(" · ");
  return [kind, i.project_name].filter(Boolean).join(", ");
}

/** Worth asking Redmine: a number, or two letters or more. */
const searchable = (q: string) => /^#?\d+$/.test(q) || q.length >= 2;

/**
 * A select with search: opens on a list of issues (`options`, then `load`),
 * typing `#1234` or words from the subject narrows it, and with `remote` also
 * finds open issues on Redmine. Esc or a click outside closes it without
 * changing anything.
 */
export default function IssuePicker({ value, onPick, label, disabled, options, load, remote, extra, empty }: Props) {
  const [open, setOpen] = useState(false);
  const [q, setQ] = useState("");
  const [loaded, setLoaded] = useState<IssueBrief[] | null>(null);
  const [found, setFound] = useState<IssueBrief[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [active, setActive] = useState(0);
  const box = useRef<HTMLDivElement>(null);
  const input = useRef<HTMLInputElement>(null);
  const list = useRef<HTMLDivElement>(null);

  /** Fades the list out, then puts the field back. */
  const [closing, setClosing] = useState(false);
  const close = () => {
    const reset = () => {
      setOpen(false);
      setClosing(false);
      setQ("");
      setFound(null);
      setError(null);
    };
    if (!box.current?.querySelector(".ipick-pop")) return reset();
    setClosing(true);
    setTimeout(reset, exitDelay(120));
  };

  useEffect(() => {
    if (!open) return;
    input.current?.focus();
    const outside = (e: PointerEvent) => {
      if (!box.current?.contains(e.target as Node)) close();
    };
    document.addEventListener("pointerdown", outside, true);
    return () => document.removeEventListener("pointerdown", outside, true);
  }, [open]);

  // The rest of the list, once.
  useEffect(() => {
    if (!open || !load || loaded) return;
    let live = true;
    load()
      .then((l) => live && setLoaded(l))
      .catch((e) => {
        if (!live) return;
        setLoaded([]);
        setError(errorText(e));
      });
    return () => {
      live = false;
    };
  }, [open, load, loaded]);

  // Redmine search as the user types, after a short pause; a newer query wins.
  const text = q.trim();
  useEffect(() => {
    setFound(null);
    if (!open || !remote || !searchable(text)) return;
    let live = true;
    const t = setTimeout(async () => {
      try {
        const list = (await api.searchIssues(text)).map(issueBrief);
        if (live) setFound(list);
      } catch (e) {
        if (live) setError(errorText(e));
      }
    }, 300);
    return () => {
      live = false;
      clearTimeout(t);
    };
  }, [text, open, remote]);

  const listed = useMemo(() => {
    const all = [...(options ?? []), ...(loaded ?? [])];
    const seen = new Set<number>();
    return all.filter((i) => !seen.has(i.id) && seen.add(i.id) && (!text || matches(i, text)));
  }, [options, loaded, text]);
  const more = (found ?? []).filter((i) => !listed.some((x) => x.id === i.id));
  const showExtra = !!extra && !text;
  /** Every row in order, for the arrow keys: the extra row is -1. */
  const rows: (IssueBrief | null)[] = [...(showExtra ? [null] : []), ...listed, ...more];

  // Start on the current value, else the first row.
  useEffect(() => {
    if (!open) return;
    const at = rows.findIndex((r) => (r ? r.id === value?.id : extra?.chosen));
    setActive(text || at < 0 ? 0 : at);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, text, loaded]);

  useEffect(() => {
    list.current?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: "nearest" });
  }, [active]);

  const pick = (row: IssueBrief | null) => {
    if (row) onPick(row);
    else extra?.onPick();
    close();
  };

  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      close();
    } else if (e.key === "ArrowDown" && rows.length) {
      e.preventDefault();
      setActive((a) => (a + 1) % rows.length);
    } else if (e.key === "ArrowUp" && rows.length) {
      e.preventDefault();
      setActive((a) => (a - 1 + rows.length) % rows.length);
    } else if (e.key === "Enter" && active < rows.length) {
      e.preventDefault();
      pick(rows[active]);
    }
  };

  const option = (r: IssueBrief, i: number) => (
    <button
      key={r.id}
      type="button"
      role="option"
      aria-selected={i === active}
      className="ipick-option"
      onMouseEnter={() => setActive(i)}
      onClick={() => pick(r)}
    >
      <span className="ipick-id">#{r.id}</span>
      <span className="ipick-subject">{r.subject}</span>
      {r.id === value?.id && <Icon name="check" size={16} className="ipick-check" />}
      <span className="ipick-meta">{issueMeta(r, true)}</span>
    </button>
  );

  const waiting = !!load && loaded === null;
  const searching = remote && searchable(text) && found === null && !error;
  const offset = showExtra ? 1 : 0;

  return (
    <div className="ipick" ref={box}>
      {open ? (
        <input
          ref={input}
          className="ipick-input"
          value={q}
          onChange={(e) => setQ(e.target.value)}
          onKeyDown={onKeyDown}
          placeholder="Lọc theo #id hoặc tên task"
          aria-label={label}
          aria-expanded
          aria-controls="ipick-list"
          role="combobox"
        />
      ) : (
        <button
          type="button"
          className="ipick-value"
          onClick={() => setOpen(true)}
          disabled={disabled}
          aria-label={`${label}: đổi`}
          aria-haspopup="listbox"
        >
          {value ? (
            <>
              <span className="ipick-id">#{value.id}</span>
              <span className="ipick-subject">{value.subject}</span>
              <span className="ipick-meta">{issueMeta(value, false)}</span>
            </>
          ) : extra?.chosen ? (
            <span className="ipick-subject">{extra.label}</span>
          ) : (
            <span className="text-missing">{empty ?? "Chưa có, bấm để chọn"}</span>
          )}
          <Icon name="chevronDown" size={16} className="ipick-caret" />
        </button>
      )}

      {open && (
        <div
          ref={list}
          className={`ipick-pop${closing ? " is-closing" : ""}`}
          id="ipick-list"
          role="listbox"
          aria-label={label}
        >
          {showExtra && (
            <button
              type="button"
              role="option"
              aria-selected={active === 0}
              className="ipick-option is-extra"
              onMouseEnter={() => setActive(0)}
              onClick={() => pick(null)}
            >
              <Icon name="plus" size={16} />
              <span className="ipick-subject">{extra.label}</span>
              {extra.chosen && <Icon name="check" size={16} className="ipick-check" />}
            </button>
          )}
          {listed.map((r, i) => option(r, i + offset))}
          {waiting && <RowsSkeleton rows={3} className="is-compact" />}
          {more.length > 0 && <p className="ipick-group">Tìm trên Redmine</p>}
          {more.map((r, i) => option(r, i + offset + listed.length))}
          {searching && <p className="ipick-note muted">Đang tìm trên Redmine…</p>}
          {error && <p className="ipick-note text-missing">{error}</p>}
          {!waiting && !searching && rows.length === 0 && (
            <p className="ipick-note muted">
              {text ? "Không có task nào khớp." : remote ? "Gõ #id hoặc tên task để tìm." : "Chưa có task nào."}
            </p>
          )}
        </div>
      )}
    </div>
  );
}
