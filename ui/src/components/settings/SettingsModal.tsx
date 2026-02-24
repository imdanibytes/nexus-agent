import { useState, useCallback, useEffect, type FC } from "react";
import {
  BotIcon,
  ServerIcon,
  XIcon,
  SunIcon,
  MoonIcon,
  MonitorIcon,
  CloudIcon,
  CpuIcon,
} from "lucide-react";
import { LazyMotion, domAnimation, m, AnimatePresence } from "framer-motion";
import { useUIStore, type Theme } from "../../stores/uiStore";
import { cn } from "../../lib/utils";
import { ProvidersTab } from "./ProvidersTab";
import { AgentsTab } from "./AgentsTab";

interface SettingsTab {
  id: string;
  label: string;
  icon: FC<{ size?: number; strokeWidth?: number; className?: string }>;
}

const TABS: SettingsTab[] = [
  { id: "general", label: "General", icon: BotIcon },
  { id: "providers", label: "Providers", icon: CloudIcon },
  { id: "agents", label: "Agents", icon: CpuIcon },
  { id: "mcp", label: "MCP Servers", icon: ServerIcon },
];

const THEME_OPTIONS: { value: Theme; label: string; icon: FC<{ className?: string }> }[] = [
  { value: "light", label: "Light", icon: SunIcon },
  { value: "dark", label: "Dark", icon: MoonIcon },
  { value: "system", label: "System", icon: MonitorIcon },
];

const GeneralTab: FC = () => {
  const { theme, setTheme } = useUIStore();

  return (
    <div className="space-y-6">
      <div>
        <h3 className="text-sm font-medium text-foreground mb-3">Appearance</h3>
        <div className="flex gap-2">
          {THEME_OPTIONS.map((opt) => {
            const Icon = opt.icon;
            const isActive = theme === opt.value;
            return (
              <button
                key={opt.value}
                type="button"
                onClick={() => setTheme(opt.value)}
                className={cn(
                  "flex items-center gap-2 px-3 py-1.5 rounded-lg text-xs font-medium transition-colors",
                  isActive
                    ? "bg-foreground text-background"
                    : "bg-default-200/50 text-default-600 hover:bg-default-200",
                )}
              >
                <Icon className="size-3.5" />
                {opt.label}
              </button>
            );
          })}
        </div>
      </div>
      <div>
        <p className="text-xs text-default-400">
          Configure providers and agents in their respective tabs.
        </p>
      </div>
    </div>
  );
};

const McpTab: FC = () => (
  <div className="space-y-6">
    <div>
      <h3 className="text-sm font-medium text-foreground mb-1">MCP Servers</h3>
      <p className="text-xs text-default-500">
        Configured in <code className="text-[11px] px-1 py-0.5 rounded bg-default-200/50">~/.nexus/config.toml</code>
      </p>
    </div>
  </div>
);

const TAB_COMPONENTS: Record<string, FC> = {
  general: GeneralTab,
  providers: ProvidersTab,
  agents: AgentsTab,
  mcp: McpTab,
};

export const SettingsModal: FC = () => {
  const { settingsOpen, setSettingsOpen } = useUIStore();
  const [activeTab, setActiveTab] = useState("general");

  const onClose = useCallback(() => setSettingsOpen(false), [setSettingsOpen]);

  useEffect(() => {
    if (!settingsOpen) return;
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", handleKey);
    return () => document.removeEventListener("keydown", handleKey);
  }, [settingsOpen, onClose]);

  if (!settingsOpen) return null;

  const ActiveComponent = TAB_COMPONENTS[activeTab] ?? GeneralTab;

  return (
    <LazyMotion features={domAnimation}>
      <AnimatePresence>
        {settingsOpen && (
          <m.div
            className="fixed inset-0 z-50 flex items-center justify-center"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.2 }}
          >
            {/* Backdrop */}
            <div
              className="absolute inset-0 bg-black/30 dark:bg-black/40 backdrop-blur-sm"
              onClick={onClose}
            />

            {/* Modal */}
            <m.div
              className="relative z-10 flex h-[70vh] w-[min(90vw,48rem)] gap-2 p-2"
              initial={{ opacity: 0, scale: 0.96, y: 12 }}
              animate={{ opacity: 1, scale: 1, y: 0 }}
              exit={{ opacity: 0, scale: 0.96, y: 12 }}
              transition={{ duration: 0.25, ease: "easeOut" }}
            >
              {/* Nav */}
              <nav className="w-[180px] shrink-0 rounded-xl bg-white/95 dark:bg-default-50/80 backdrop-blur-2xl border border-default-200/50 shadow-sm dark:shadow-none p-4 flex flex-col gap-1">
                <div className="flex items-center justify-between mb-3">
                  <h2 className="text-sm font-semibold text-foreground">
                    Settings
                  </h2>
                  <button
                    type="button"
                    onClick={onClose}
                    className="p-1 rounded hover:bg-default-200/40 transition-colors text-default-400 hover:text-default-900"
                  >
                    <XIcon className="size-4" />
                  </button>
                </div>
                {TABS.map((tab) => {
                  const Icon = tab.icon;
                  const isActive = activeTab === tab.id;
                  return (
                    <button
                      key={tab.id}
                      type="button"
                      onClick={() => setActiveTab(tab.id)}
                      className={cn(
                        "relative w-full flex items-center gap-3 px-3 py-2 rounded-xl text-sm text-left transition-colors duration-200",
                        isActive
                          ? "text-foreground font-medium"
                          : "text-default-500 hover:text-foreground hover:bg-default-200/40",
                      )}
                    >
                      {isActive && (
                        <m.div
                          layoutId="settings-nav"
                          className="absolute inset-0 rounded-xl bg-default-200/50"
                          transition={{
                            type: "spring",
                            bounce: 0.15,
                            duration: 0.4,
                          }}
                        />
                      )}
                      <span className="relative flex items-center gap-3 w-full">
                        <Icon size={15} strokeWidth={1.5} />
                        <span className="flex-1">{tab.label}</span>
                      </span>
                    </button>
                  );
                })}
              </nav>

              {/* Content */}
              <div className="flex-1 min-h-0 rounded-xl bg-white/95 dark:bg-default-50/80 backdrop-blur-2xl border border-default-200/50 shadow-sm dark:shadow-none overflow-y-auto p-8">
                <AnimatePresence mode="wait">
                  <m.div
                    key={activeTab}
                    initial={{ opacity: 0, y: 8 }}
                    animate={{ opacity: 1, y: 0 }}
                    exit={{ opacity: 0, y: -8 }}
                    transition={{ duration: 0.2, ease: "easeOut" }}
                  >
                    <ActiveComponent />
                  </m.div>
                </AnimatePresence>
              </div>
            </m.div>
          </m.div>
        )}
      </AnimatePresence>
    </LazyMotion>
  );
};
