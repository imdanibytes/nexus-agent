import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { TooltipProvider } from "./components/ui/tooltip.js";
import { App } from "./App.js";
import "./styles/global.css";

// Load Nexus theme CSS from Host API
fetch("/api/config")
  .then((r) => r.json())
  .then((config: { apiUrl: string }) => {
    const link = document.createElement("link");
    link.rel = "stylesheet";
    link.href = `${config.apiUrl}/api/v1/theme.css`;
    document.head.appendChild(link);
  })
  .catch(() => {
    // Theme loading is best-effort — CSS fallbacks apply
  });

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <TooltipProvider delayDuration={300}>
      <App />
    </TooltipProvider>
  </StrictMode>
);
