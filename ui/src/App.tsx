import { useRef, useState, useEffect } from "react";
import { Sidebar } from "./components/Sidebar.js";
import { ChatArea } from "./components/ChatArea.js";
import { SettingsPanel } from "./components/SettingsPanel.js";
import { useChatStore } from "./stores/chatStore.js";

function useIsCompact(ref: React.RefObject<HTMLElement | null>, breakpoint = 640) {
  const [compact, setCompact] = useState(false);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(([entry]) => {
      setCompact(entry.contentRect.width < breakpoint);
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, [ref, breakpoint]);
  return compact;
}

export function App() {
  const { chatOpen, settingsOpen } = useChatStore();
  const containerRef = useRef<HTMLDivElement>(null);
  const compact = useIsCompact(containerRef);

  if (compact) {
    return (
      <div ref={containerRef} className="flex h-full">
        {settingsOpen ? (
          <SettingsPanel compact />
        ) : chatOpen ? (
          <ChatArea compact />
        ) : (
          <Sidebar compact />
        )}
      </div>
    );
  }

  return (
    <div ref={containerRef} className="flex h-full">
      <Sidebar />
      {settingsOpen ? <SettingsPanel /> : <ChatArea />}
    </div>
  );
}
