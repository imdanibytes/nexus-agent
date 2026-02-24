import { create } from "zustand";

export type Theme = "light" | "dark" | "system";

const STORAGE_KEY = "nexus-theme";

function getStoredTheme(): Theme {
  const stored = localStorage.getItem(STORAGE_KEY);
  if (stored === "light" || stored === "dark" || stored === "system") return stored;
  return "system";
}

function resolveTheme(theme: Theme): "light" | "dark" {
  if (theme !== "system") return theme;
  return window.matchMedia("(prefers-color-scheme: dark)").matches
    ? "dark"
    : "light";
}

export function applyTheme(theme: Theme): void {
  const resolved = resolveTheme(theme);
  document.documentElement.classList.toggle("dark", resolved === "dark");
}

interface UIState {
  settingsOpen: boolean;
  settingsTab: string | null;
  openSettings: (tab?: string) => void;
  closeSettings: () => void;
  setSettingsOpen: (open: boolean) => void;
  theme: Theme;
  setTheme: (theme: Theme) => void;
}

export const useUIStore = create<UIState>((set) => ({
  settingsOpen: false,
  settingsTab: null,
  openSettings: (tab) => set({ settingsOpen: true, settingsTab: tab ?? null }),
  closeSettings: () => set({ settingsOpen: false, settingsTab: null }),
  setSettingsOpen: (open) => set(open ? { settingsOpen: true } : { settingsOpen: false, settingsTab: null }),
  theme: getStoredTheme(),
  setTheme: (theme) => {
    localStorage.setItem(STORAGE_KEY, theme);
    applyTheme(theme);
    set({ theme });
  },
}));
