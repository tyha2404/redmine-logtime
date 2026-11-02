import { useCallback, useState } from "react";
import { api, errorText } from "./api";

/**
 * Runs one Claude job, collecting its live progress lines (the Redmine
 * requests it makes) and exposing cancel.
 */
export function useAiRun() {
  const [running, setRunning] = useState(false);
  const [log, setLog] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  /** What is running, for the live line: Claude, or a quick local match. */
  const [label, setLabel] = useState("Claude đang làm việc");

  const run = useCallback(async <T>(job: () => Promise<T>, what = "Claude đang làm việc"): Promise<T | null> => {
    setLabel(what);
    setRunning(true);
    setError(null);
    setLog([]);
    const unlisten = await api.onAiProgress((p) => setLog((l) => [...l, p.text]));
    try {
      return await job();
    } catch (e) {
      setError(errorText(e));
      return null;
    } finally {
      unlisten();
      setRunning(false);
    }
  }, []);

  const cancel = useCallback(() => api.aiCancel(), []);

  return { running, log, error, label, run, cancel };
}

export type AiRun = ReturnType<typeof useAiRun>;
