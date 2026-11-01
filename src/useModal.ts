import { useEffect, useRef, type MouseEvent, type PointerEvent, type SyntheticEvent } from "react";

/**
 * A native modal `<dialog>` that closes on Esc or a click on the backdrop
 * (outside its box). While `locked`, both are ignored, e.g. during a write
 * to Redmine. Spread the result onto the `<dialog>`.
 */
export function useModal(onClose: () => void, locked = false) {
  const ref = useRef<HTMLDialogElement>(null);
  /** Where the press started: a drag from inside the box out to the backdrop is not a backdrop click. */
  const pressedOutside = useRef(false);

  useEffect(() => {
    const d = ref.current;
    if (d && !d.open) d.showModal();
  }, []);

  const outside = (e: { clientX: number; clientY: number; target: EventTarget }) => {
    const d = ref.current;
    if (!d || e.target !== d) return false;
    const r = d.getBoundingClientRect();
    return e.clientX < r.left || e.clientX > r.right || e.clientY < r.top || e.clientY > r.bottom;
  };

  return {
    ref,
    onCancel: (e: SyntheticEvent) => {
      e.preventDefault();
      if (!locked) onClose();
    },
    onPointerDown: (e: PointerEvent) => {
      pressedOutside.current = outside(e);
    },
    onClick: (e: MouseEvent) => {
      if (pressedOutside.current && outside(e) && !locked) onClose();
      pressedOutside.current = false;
    },
  };
}
