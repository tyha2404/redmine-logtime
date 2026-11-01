import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { api, errorText, issueBrief, type IssueBrief } from "./api";
import { RowsSkeleton } from "./Skeleton";
import { exitDelay } from "./useModal";

interface Props {
  value: IssueBrief | null;
  onPick: (issue: IssueBrief) => void;
  label: string;
  disabled?: boolean;
}

/**
 * An issue field: shows the chosen issue; typing `#1234` or words from the
 * subject lists matching issues in a popover. Esc or a click outside closes
 * it without changing anything.
 */
export default function IssuePicker({ value, onPick, label, disabled }: Props) {
  const [open, setOpen] = useState(false);
  const [q, setQ] = useState("");
  const [results, setResults] = useState<IssueBrief[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [active, setActive] = useState(0);
  const box = useRef<HTMLDivElement>(null);
  const input = useRef<HTMLInputElement>(null);

  /** Fades the list out, then puts the field back. */
  const [closing, setClosing] = useState(false);
  const close = () => {
    const reset = () => {
      setOpen(false);
      setClosing(false);
      setQ("");
      setResults(null);
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

  // Search as the user types, after a short pause; a newer query wins.
  useEffect(() => {
    if (!open) return;
    const text = q.trim();
    if (text.length < 2 && !/^#?\d+$/.test(text)) {
      setResults(null);
      return;
    }
    let live = true;
    const t = setTimeout(async () => {
      try {
        const found = (await api.searchIssues(text)).map(issueBrief);
        if (!live) return;
        setResults(found);
        setActive(0);
        setError(null);
      } catch (e) {
        if (live) setError(errorText(e));
      }
    }, 300);
    return () => {
      live = false;
      clearTimeout(t);
    };
  }, [q, open]);

  const pick = (issue: IssueBrief) => {
    onPick(issue);
    close();
  };

  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      close();
    } else if (e.key === "ArrowDown" && results?.length) {
      e.preventDefault();
      setActive((a) => (a + 1) % results.length);
    } else if (e.key === "ArrowUp" && results?.length) {
      e.preventDefault();
      setActive((a) => (a - 1 + results.length) % results.length);
    } else if (e.key === "Enter" && results?.[active]) {
      e.preventDefault();
      pick(results[active]);
    }
  };

  return (
    <div className="ipick" ref={box}>
      {open ? (
        <input
          ref={input}
          className="ipick-input"
          value={q}
          onChange={(e) => setQ(e.target.value)}
          onKeyDown={onKeyDown}
          placeholder="#1234 hoặc từ khoá trong tên issue"
          aria-label={label}
          aria-expanded
          aria-controls="ipick-list"
          role="combobox"
        />
      ) : (
        <button type="button" className="ipick-value" onClick={() => setOpen(true)} disabled={disabled} aria-label={`${label}: đổi`}>
          {value ? (
            <>
              <span className="ipick-id">#{value.id}</span>
              <span className="ipick-subject">{value.subject}</span>
              <span className="ipick-meta">
                {value.tracker}, {value.project_name}
              </span>
            </>
          ) : (
            <span className="text-missing">Chưa có, bấm để chọn</span>
          )}
        </button>
      )}

      {open && (q.trim() || error) && (
        <div className={`ipick-pop${closing ? " is-closing" : ""}`} id="ipick-list" role="listbox" aria-label="Issue tìm được">
          {error ? (
            <p className="ipick-note text-missing">{error}</p>
          ) : results === null ? (
            q.trim().length < 2 && !/^#?\d+$/.test(q.trim()) ? (
              <p className="ipick-note muted">Gõ thêm để tìm…</p>
            ) : (
              <RowsSkeleton rows={3} className="is-compact" />
            )
          ) : results.length === 0 ? (
            <p className="ipick-note muted">Không tìm thấy issue đang mở nào khớp.</p>
          ) : (
            results.map((r, i) => (
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
                <span className="ipick-meta">
                  {r.tracker} · {r.status}, {r.project_name}
                </span>
              </button>
            ))
          )}
        </div>
      )}
    </div>
  );
}
