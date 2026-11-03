/** One tab: `id` is what the tab switches to. */
export interface TabItem<T extends string> {
  id: T;
  label: string;
}

/**
 * The app's one tab strip (underlined, like the editor's): for switching what a
 * page or a form shows. Arrow keys, Home and End move between tabs. With
 * `idPrefix`, each tab is `<prefix>-tab-<id>` and controls the panel `<prefix>-<id>`.
 */
export default function Tabs<T extends string>({
  items,
  value,
  onChange,
  label,
  idPrefix,
}: {
  items: readonly TabItem<T>[];
  value: T;
  onChange: (id: T) => void;
  label: string;
  idPrefix?: string;
}) {
  const tabId = (id: T) => (idPrefix ? `${idPrefix}-tab-${id}` : undefined);
  return (
    <div className="tabs" role="tablist" aria-label={label}>
      {items.map(({ id, label: text }, index) => (
        <button
          key={id}
          id={tabId(id)}
          type="button"
          role="tab"
          aria-selected={value === id}
          aria-controls={idPrefix ? `${idPrefix}-${id}` : undefined}
          tabIndex={value === id ? 0 : -1}
          onClick={() => onChange(id)}
          onKeyDown={(event) => {
            let next: number;
            if (event.key === "ArrowRight") next = (index + 1) % items.length;
            else if (event.key === "ArrowLeft") next = (index + items.length - 1) % items.length;
            else if (event.key === "Home") next = 0;
            else if (event.key === "End") next = items.length - 1;
            else return;
            event.preventDefault();
            onChange(items[next].id);
            const strip = event.currentTarget.parentElement;
            (strip?.children[next] as HTMLElement | undefined)?.focus();
          }}
        >
          {text}
        </button>
      ))}
    </div>
  );
}
