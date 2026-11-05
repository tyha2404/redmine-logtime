import { useCallback, useSyncExternalStore } from "react";
import { api, errorText } from "./api";

/** One AI job and what the user saw of it. Lives outside React so leaving the page does not lose it. */
interface Job {
  running: boolean;
  label: string;
  log: string[];
  error: string | null;
  /** When it started (`performance.now()`), so the elapsed time survives a remount. */
  startedAt: number;
  /** The job's result, null on failure. */
  done: Promise<unknown>;
}

const IDLE: Job = { running: false, label: "", log: [], error: null, startedAt: 0, done: Promise.resolve(null) };

/** The latest job of each slot (a page or a calendar day). */
const jobs = new Map<string, Job>();
const subs = new Set<() => void>();

function update(slot: string, patch: Partial<Job>) {
  jobs.set(slot, { ...(jobs.get(slot) ?? IDLE), ...patch });
  subs.forEach((f) => f());
}

function subscribe(f: () => void) {
  subs.add(f);
  return () => subs.delete(f);
}

/**
 * Runs one Claude job for `slot`, collecting its live progress lines (the
 * Redmine requests it makes) and exposing cancel. The job keeps running when
 * the page unmounts; mounting the same slot again shows it, and `pending`
 * hands back its result.
 */
export function useAiRun(slot: string) {
  const job = useSyncExternalStore(subscribe, () => jobs.get(slot) ?? IDLE);

  const run = useCallback(
    async <T>(work: () => Promise<T>, what = "AI đang làm việc"): Promise<T | null> => {
      update(slot, { running: true, label: what, log: [], error: null, startedAt: performance.now() });
      const unlisten = await api.onAiProgress((p) => update(slot, { log: [...(jobs.get(slot)?.log ?? []), p.text] }));
      const done = (async () => {
        try {
          return await work();
        } catch (e) {
          update(slot, { error: errorText(e) });
          return null;
        } finally {
          unlisten();
          update(slot, { running: false });
        }
      })();
      update(slot, { done });
      return done;
    },
    [slot],
  );

  /** The result of this slot's job if it is still running, e.g. when the page comes back. */
  const pending = useCallback(<T>() => {
    const j = jobs.get(slot);
    return j?.running ? (j.done as Promise<T | null>) : null;
  }, [slot]);

  const cancel = useCallback(() => api.aiCancel(), []);

  return { ...job, run, pending, cancel };
}

export type AiRun = ReturnType<typeof useAiRun>;
