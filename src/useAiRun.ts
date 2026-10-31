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

  const run = useCallback(async <T,>(job: () => Promise<T>): Promise<T | null> => {
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

  return { running, log, error, run, cancel };
}

export type AiRun = ReturnType<typeof useAiRun>;
