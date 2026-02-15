import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App.js";
import "./styles/global.css";

// Load Nexus theme CSS
const link = document.createElement("link");
link.rel = "stylesheet";
link.href = "/api/config"; // We'll fetch and apply theme separately
document.head.appendChild(link);

// Fetch config and load theme
fetch("/api/config")
  .then((r) => r.json())
  .then((config: { apiUrl: string }) => {
    const themeLink = document.createElement("link");
    themeLink.rel = "stylesheet";
    themeLink.href = `${config.apiUrl}/api/v1/theme.css`;
    document.head.appendChild(themeLink);
  })
  .catch(() => {
    // Theme loading is best-effort — defaults in CSS will apply
  });

// Remove the incorrect link we added above
link.remove();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>
);
