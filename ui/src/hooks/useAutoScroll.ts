import { useCallback, useEffect, useRef, useState } from "react";

export function useAutoScroll() {
  const containerRef = useRef<HTMLDivElement>(null);
  const sentinelRef = useRef<HTMLDivElement>(null);
  const [isAtBottom, setIsAtBottom] = useState(true);

  useEffect(() => {
    const sentinel = sentinelRef.current;
    if (!sentinel) return;

    const observer = new IntersectionObserver(
      ([entry]) => setIsAtBottom(entry.isIntersecting),
      { root: containerRef.current, threshold: 0 },
    );
    observer.observe(sentinel);
    return () => observer.disconnect();
  }, []);

  const scrollToBottom = useCallback(() => {
    sentinelRef.current?.scrollIntoView({ behavior: "smooth" });
  }, []);

  const scrollToBottomIfNeeded = useCallback(() => {
    if (isAtBottom) {
      sentinelRef.current?.scrollIntoView({ behavior: "instant" as ScrollBehavior });
    }
  }, [isAtBottom]);

  return { containerRef, sentinelRef, isAtBottom, scrollToBottom, scrollToBottomIfNeeded };
}
