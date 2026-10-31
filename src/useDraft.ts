import { useCallback, useEffect, useState } from "react";
import { api, errorText, type Draft, type PlanItem, type SubtaskDraft } from "./api";

const EMPTY: Draft = { subtasks: [], items: [] };

/** The review draft, shared by every page; every change is persisted. */
export function useDraft() {
  const [draft, setDraft] = useState<Draft>(EMPTY);
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(() => {
    api
      .getDraft()
      .then(setDraft)
      .catch((e) => setError(errorText(e)));
  }, []);

  useEffect(reload, [reload]);

  const commit = useCallback(async (next: Draft) => {
    setDraft(next);
    try {
      await api.saveDraft(next);
      setError(null);
    } catch (e) {
      setError(errorText(e));
    }
  }, []);

  const add = useCallback(
    (items: PlanItem[], subtasks: SubtaskDraft[] = []) =>
      commit({ subtasks: [...draft.subtasks, ...subtasks], items: [...draft.items, ...items] }),
    [draft, commit],
  );

  const remove = useCallback(
    (ids: string[]) => {
      const items = draft.items.filter((i) => !ids.includes(i.id));
      const used = new Set(items.flatMap((i) => (i.target.type === "new_subtask" ? [i.target.key] : [])));
      return commit({ subtasks: draft.subtasks.filter((s) => used.has(s.key)), items });
    },
    [draft, commit],
  );

  const update = useCallback(
    (id: string, patch: Partial<PlanItem>) =>
      commit({
        ...draft,
        items: draft.items.map((i) => (i.id === id ? { ...i, ...patch, last_error: null } : i)),
      }),
    [draft, commit],
  );

  return { draft, error, reload, add, remove, update };
}

export type DraftApi = ReturnType<typeof useDraft>;
