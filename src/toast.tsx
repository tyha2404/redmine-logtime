import { Icon } from "./icons";
import { createContext, useCallback, useContext, useEffect, useRef, useState, type ReactNode } from "react";

/**
 * Short confirmations at the bottom of the window. A toast can carry one
 * action (usually "Hoàn tác"), which closes it.
 */
interface Toast {
  id: number;
  text: string;
  tone: "ok" | "error";
  action?: { label: string; run: () => void };
}

type Show = (text: string, opts?: { tone?: Toast["tone"]; action?: Toast["action"] }) => void;

const ToastContext = createContext<Show>(() => {});

export function useToast(): Show {
  return useContext(ToastContext);
}

const LIFETIME_MS = 6000;

export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const nextId = useRef(1);

  const dismiss = useCallback((id: number) => setToasts((ts) => ts.filter((t) => t.id !== id)), []);

  const show = useCallback<Show>((text, opts) => {
    const id = nextId.current++;
    // Keep the stack short: the newest three are enough to read.
    setToasts((ts) => [...ts.slice(-2), { id, text, tone: opts?.tone ?? "ok", action: opts?.action }]);
  }, []);

  return (
    <ToastContext.Provider value={show}>
      {children}
      <div className="toasts" role="status" aria-live="polite">
        {toasts.map((t) => (
          <ToastItem key={t.id} toast={t} dismiss={dismiss} />
        ))}
      </div>
    </ToastContext.Provider>
  );
}

function ToastItem({ toast, dismiss }: { toast: Toast; dismiss: (id: number) => void }) {
  const [paused, setPaused] = useState(false);
  const onDone = useCallback(() => dismiss(toast.id), [dismiss, toast.id]);

  // Hovering or focusing a toast holds it open, so the action stays reachable.
  useEffect(() => {
    if (paused) return;
    const t = setTimeout(onDone, LIFETIME_MS);
    return () => clearTimeout(t);
  }, [paused, onDone]);

  return (
    <div
      className={`toast is-${toast.tone}`}
      onMouseEnter={() => setPaused(true)}
      onMouseLeave={() => setPaused(false)}
      onFocus={() => setPaused(true)}
      onBlur={() => setPaused(false)}
    >
      <span>{toast.text}</span>
      {toast.action && (
        <button
          className="toast-action"
          onClick={() => {
            toast.action!.run();
            onDone();
          }}
        >
          {toast.action.label}
        </button>
      )}
      <button className="toast-close" onClick={onDone} aria-label="Đóng thông báo">
        <Icon name="close" size={16} />
      </button>
    </div>
  );
}
