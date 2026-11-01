import { useCallback, useEffect, useRef, type MouseEvent, type PointerEvent, type SyntheticEvent } from "react";

/** How long leaving animations run (--motion in styles.css). */
export const EXIT_MS = 200;

/** Waits for a leaving animation, or not at all when motion is reduced. */
export function exitDelay(ms = EXIT_MS): number {
  return window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ? 0 : ms;
}

/**
 * A native modal `<dialog>` that closes on Esc or a click on the backdrop
 * (outside its box). While `locked`, both are ignored, e.g. during a write
 * to Redmine. Closing fades the dialog out first: buttons inside call
 * `close()` (or `close(then)` to run something else once it is gone).
 *
 *   const { close, ...dialog } = useModal(onClose);
 *   <dialog {...dialog}>…<button onClick={() => close()}>Huỷ</button></dialog>
 */
export function useModal(onClose: () => void, locked = false) {
  const ref = useRef<HTMLDialogElement>(null);
  /** Where the press started: a drag from inside the box out to the backdrop is not a backdrop click. */
  const pressedOutside = useRef(false);
  const leaving = useRef(false);

  useEffect(() => {
    const d = ref.current;
    if (d && !d.open) d.showModal();
  }, []);

  const close = useCallback(
    (then: () => void = onClose) => {
      const d = ref.current;
      if (leaving.current) return;
      leaving.current = true;
      d?.classList.add("is-closing");
      setTimeout(then, exitDelay());
    },
    [onClose],
  );

  const outside = (e: { clientX: number; clientY: number; target: EventTarget }) => {
    const d = ref.current;
    if (!d || e.target !== d) return false;
    const r = d.getBoundingClientRect();
    return e.clientX < r.left || e.clientX > r.right || e.clientY < r.top || e.clientY > r.bottom;
  };

  return {
    ref,
    close,
    onCancel: (e: SyntheticEvent) => {
      e.preventDefault();
      if (!locked) close();
    },
    onPointerDown: (e: PointerEvent) => {
      pressedOutside.current = outside(e);
    },
    onClick: (e: MouseEvent) => {
      if (pressedOutside.current && outside(e) && !locked) close();
      pressedOutside.current = false;
    },
  };
}
