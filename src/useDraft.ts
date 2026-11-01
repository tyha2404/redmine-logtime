import { useCallback, useEffect, useRef, useState } from "react";
import { api, errorText, type Draft, type PlanItem, type SubtaskDraft, type Target } from "./api";

const EMPTY: Draft = { subtasks: [], items: [] };

/** What `remove` took out, so it can be put back. */
export interface Removed {
  items: PlanItem[];
  subtasks: SubtaskDraft[];
}

/**
 * The review draft, shared by every page; every change is persisted.
 *
 * Mutations read the latest draft from a ref, not from the render they were
 * created in, so a callback held by a toast ("Hoàn tác") still applies to
 * the current queue.
 */
export function useDraft() {
  const [draft, setDraft] = useState<Draft>(EMPTY);
  const [error, setError] = useState<string | null>(null);
  const current = useRef<Draft>(EMPTY);

  const reload = useCallback(() => {
    api
      .getDraft()
      .then((d) => {
        current.current = d;
        setDraft(d);
      })
      .catch((e) => setError(errorText(e)));
  }, []);

  useEffect(reload, [reload]);

  const commit = useCallback(async (next: Draft) => {
    current.current = next;
    setDraft(next);
    try {
      await api.saveDraft(next);
      setError(null);
    } catch (e) {
      setError(errorText(e));
    }
  }, []);

  const add = useCallback(
    (items: PlanItem[], subtasks: SubtaskDraft[] = []) => {
      const d = current.current;
      const haveSub = new Set(d.subtasks.map((s) => s.key));
      const haveItem = new Set(d.items.map((i) => i.id));
      return commit({
        subtasks: [...d.subtasks, ...subtasks.filter((s) => !haveSub.has(s.key))],
        items: [...d.items, ...items.filter((i) => !haveItem.has(i.id))],
      });
    },
    [commit],
  );

  const remove = useCallback(
    async (ids: string[]): Promise<Removed> => {
      const d = current.current;
      const gone = new Set(ids);
      const items = d.items.filter((i) => !gone.has(i.id));
      const used = new Set(items.flatMap((i) => (i.target.type === "new_subtask" ? [i.target.key] : [])));
      const removed: Removed = {
        items: d.items.filter((i) => gone.has(i.id)),
        subtasks: d.subtasks.filter((s) => !used.has(s.key)),
      };
      await commit({ subtasks: d.subtasks.filter((s) => used.has(s.key)), items });
      return removed;
    },
    [commit],
  );

  const restore = useCallback((r: Removed) => add(r.items, r.subtasks), [add]);

  const update = useCallback(
    (id: string, patch: Partial<PlanItem>) => {
      const d = current.current;
      return commit({
        ...d,
        items: d.items.map((i) => (i.id === id ? { ...i, ...patch, last_error: null } : i)),
      });
    },
    [commit],
  );

  /**
   * Points `ids` at `target`. Passing `subtask` adds or replaces that new
   * sub-task (same key = every logtime on it changes). Sub-tasks no logtime
   * uses any more are dropped.
   */
  const retarget = useCallback(
    (ids: string[], target: Target, subtask?: SubtaskDraft, patch?: Partial<PlanItem>) => {
      const d = current.current;
      const on = new Set(ids);
      const items = d.items.map((i) => (on.has(i.id) ? { ...i, ...patch, target, last_error: null } : i));
      const used = new Set(items.flatMap((i) => (i.target.type === "new_subtask" ? [i.target.key] : [])));
      const subtasks = (subtask ? [...d.subtasks.filter((s) => s.key !== subtask.key), subtask] : d.subtasks).filter((s) =>
        used.has(s.key),
      );
      return commit({ items, subtasks });
    },
    [commit],
  );

  /** The queue as it is now, to put back with `replace` (undo). */
  const snapshot = useCallback(() => current.current, []);
  const replace = useCallback((d: Draft) => commit(d), [commit]);

  return { draft, error, reload, add, remove, restore, update, retarget, snapshot, replace };
}

export type DraftApi = ReturnType<typeof useDraft>;
