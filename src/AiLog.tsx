import { useEffect, useRef, useState } from "react";
import type { AiRun } from "./useAiRun";

/** Live view of an AI run: elapsed time, the Redmine requests, cancel. */
export default function AiLog({ ai, idleHint }: { ai: AiRun; idleHint?: string }) {
  const [seconds, setSeconds] = useState(0);
  const listRef = useRef<HTMLOListElement>(null);

  // Counted from the start, so it stays right when the page comes back mid-run.
  useEffect(() => {
    if (!ai.running) return;
    const tick = () => setSeconds(Math.max(0, Math.round((performance.now() - ai.startedAt) / 1000)));
    tick();
    const t = setInterval(tick, 1000);
    return () => clearInterval(t);
  }, [ai.running, ai.startedAt]);

  useEffect(() => {
    listRef.current?.lastElementChild?.scrollIntoView({ block: "nearest" });
  }, [ai.log.length]);

  if (!ai.running && ai.log.length === 0 && !ai.error) {
    return idleHint ? <p className="hint">{idleHint}</p> : null;
  }

  return (
    <div className="ai-log" aria-live="polite">
      {ai.running && (
        <div className="ai-log-head">
          <span className="ai-pulse" aria-hidden />
          <span>
            {ai.label}, {seconds}s{ai.log.length > 0 && `, ${ai.log.length} lần đọc Redmine`}
          </span>
          <button className="btn ghost small" onClick={ai.cancel}>
            Huỷ
          </button>
        </div>
      )}
      {ai.log.length > 0 && (
        <ol ref={listRef} className="ai-log-list">
          {ai.log.map((line, i) => (
            <li key={i}>{line}</li>
          ))}
        </ol>
      )}
      {ai.error && <p className="notice error">{ai.error}</p>}
    </div>
  );
}
