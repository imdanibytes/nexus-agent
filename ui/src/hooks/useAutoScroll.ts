import { useCallback, useEffect, useRef, useState } from "react";

const THRESHOLD = 50;

export function useAutoScroll() {
  const containerRef = useRef<HTMLDivElement>(null);
  const sentinelRef = useRef<HTMLDivElement>(null);
  const [isAtBottom, setIsAtBottom] = useState(true);
  const isFollowingRef = useRef(true);
  const lastValueRef = useRef(true);

  const checkBottom = useCallback(() => {
    const el = containerRef.current;
    if (!el) return;
    const nearBottom =
      el.scrollHeight - el.scrollTop - el.clientHeight < THRESHOLD;
    isFollowingRef.current = nearBottom;
    // Only call setState when the value actually changes
    if (lastValueRef.current !== nearBottom) {
      lastValueRef.current = nearBottom;
      setIsAtBottom(nearBottom);
    }
  }, []);

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;

    el.addEventListener("scroll", checkBottom, { passive: true });

    // Also check on resize (viewport changes can affect overflow)
    const ro = new ResizeObserver(checkBottom);
    ro.observe(el);

    return () => {
      el.removeEventListener("scroll", checkBottom);
      ro.disconnect();
    };
  }, [checkBottom]);

  const scrollToBottom = useCallback(() => {
    const el = containerRef.current;
    if (el) el.scrollTo({ top: el.scrollHeight, behavior: "smooth" });
  }, []);

  const scrollToBottomIfNeeded = useCallback(() => {
    if (isFollowingRef.current) {
      const el = containerRef.current;
      if (el) el.scrollTop = el.scrollHeight;
    }
    // Re-check after scroll position is updated
    checkBottom();
  }, [checkBottom]);

  return {
    containerRef,
    sentinelRef,
    isAtBottom,
    scrollToBottom,
    scrollToBottomIfNeeded,
  };
}
