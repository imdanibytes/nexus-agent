import { useCallback, type RefObject } from "react";

export function useScrollLock(
  ref: RefObject<HTMLElement | null>,
  duration: number,
): () => void {
  return useCallback(() => {
    const el = ref.current;
    if (!el) return;

    let scrollParent: HTMLElement | null = el.parentElement;
    while (scrollParent) {
      const { overflow, overflowY } = getComputedStyle(scrollParent);
      if (
        overflow === "auto" ||
        overflow === "scroll" ||
        overflowY === "auto" ||
        overflowY === "scroll"
      ) {
        break;
      }
      scrollParent = scrollParent.parentElement;
    }
    if (!scrollParent) return;

    const scrollTop = scrollParent.scrollTop;
    const target = scrollParent;
    let frame: number;
    const start = performance.now();

    function hold() {
      target.scrollTop = scrollTop;
      if (performance.now() - start < duration) {
        frame = requestAnimationFrame(hold);
      }
    }

    frame = requestAnimationFrame(hold);
    setTimeout(() => cancelAnimationFrame(frame), duration + 50);
  }, [ref, duration]);
}
