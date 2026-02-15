import http from "node:http";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { v4 as uuidv4 } from "uuid";
import { getAccessToken } from "./auth.js";
import { runAgentTurn, resolveUiResponse } from "./agent.js";
import { createSseWriter } from "./streaming.js";
import {
  listConversations,
  getConversation,
  saveConversation,
  deleteConversation,
  updateConversationTitle,
} from "./storage.js";
import {
  listProfiles,
  getProfile,
  createProfile,
  updateProfile,
  deleteProfile as removeProfile,
  getActiveProfileId,
  setActiveProfileId,
} from "./profiles.js";
import { probeEndpoint } from "./discovery.js";
import { getSettings, updateSettings } from "./settings.js";
import type { Conversation } from "./types.js";

const PORT = 80;
const NEXUS_API_URL = process.env.NEXUS_API_URL || "http://host.docker.internal:9600";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const publicDir = path.join(__dirname, "..", "public");

const MIME_TYPES: Record<string, string> = {
  ".html": "text/html",
  ".css": "text/css",
  ".js": "application/javascript",
  ".json": "application/json",
  ".png": "image/png",
  ".svg": "image/svg+xml",
  ".ico": "image/x-icon",
  ".woff": "font/woff",
  ".woff2": "font/woff2",
};

function readBody(req: http.IncomingMessage): Promise<string> {
  return new Promise((resolve, reject) => {
    let body = "";
    req.on("data", (chunk: Buffer) => (body += chunk.toString()));
    req.on("end", () => resolve(body));
    req.on("error", reject);
  });
}

function json(res: http.ServerResponse, status: number, data: unknown): void {
  res.writeHead(status, {
    "Content-Type": "application/json",
    "Access-Control-Allow-Origin": "*",
  });
  res.end(JSON.stringify(data));
}

const server = http.createServer(async (req, res) => {
  const method = req.method || "GET";
  const url = req.url || "/";

  // CORS preflight
  if (method === "OPTIONS") {
    res.writeHead(204, {
      "Access-Control-Allow-Origin": "*",
      "Access-Control-Allow-Methods": "GET, POST, PUT, PATCH, DELETE, OPTIONS",
      "Access-Control-Allow-Headers": "Content-Type, Authorization",
    });
    res.end();
    return;
  }

  try {
    // Health check
    if (url === "/health") {
      json(res, 200, { status: "ok" });
      return;
    }

    // Config endpoint — frontend gets token + apiUrl
    if (url === "/api/config") {
      const token = await getAccessToken();
      json(res, 200, { token, apiUrl: NEXUS_API_URL });
      return;
    }

    // Chat — start agent turn (SSE stream)
    if (method === "POST" && url === "/api/chat") {
      const body = JSON.parse(await readBody(req));
      const { conversationId, message, profileId } = body as {
        conversationId?: string;
        message: string;
        profileId?: string;
      };

      const convId = conversationId || uuidv4();
      const sse = createSseWriter(res);

      // Run agent loop (async — streams SSE events as it goes)
      runAgentTurn(convId, message, sse, profileId).catch((err) => {
        console.error("Agent turn error:", err);
        try {
          sse.writeEvent("error", {
            message: err instanceof Error ? err.message : String(err),
          });
          sse.close();
        } catch {
          // Response may already be closed
        }
      });
      return;
    }

    // Chat respond — user responds to elicitation/A2UI
    if (method === "POST" && url === "/api/chat/respond") {
      const body = JSON.parse(await readBody(req));
      const { tool_use_id, action, content } = body as {
        tool_use_id: string;
        action: string;
        content: unknown;
      };

      const resolved = resolveUiResponse(tool_use_id, action, content);
      if (resolved) {
        json(res, 200, { ok: true });
      } else {
        json(res, 404, { error: "No pending UI surface with that ID" });
      }
      return;
    }

    // --- Profile routes (order matters: /active before /:id) ---

    // List profiles
    if (method === "GET" && url === "/api/profiles") {
      json(res, 200, listProfiles());
      return;
    }

    // Create profile
    if (method === "POST" && url === "/api/profiles") {
      const body = JSON.parse(await readBody(req));
      const { name, model, systemPrompt, avatar } = body as {
        name: string;
        model: string;
        systemPrompt: string;
        avatar?: string;
      };
      if (!name || !model) {
        json(res, 400, { error: "name and model are required" });
        return;
      }
      const profile = createProfile({ name, model, systemPrompt: systemPrompt || "", avatar });
      json(res, 201, profile);
      return;
    }

    // Get active profile ID
    if (method === "GET" && url === "/api/profiles/active") {
      json(res, 200, { profileId: getActiveProfileId() });
      return;
    }

    // Set active profile ID
    if (method === "PUT" && url === "/api/profiles/active") {
      const body = JSON.parse(await readBody(req));
      const { profileId } = body as { profileId: string | null };
      setActiveProfileId(profileId);
      json(res, 200, { profileId });
      return;
    }

    // Single profile routes
    const profileMatch = url.match(/^\/api\/profiles\/([a-f0-9-]+)$/);
    if (profileMatch) {
      const id = profileMatch[1];

      if (method === "GET") {
        const profile = getProfile(id);
        if (!profile) {
          json(res, 404, { error: "Profile not found" });
          return;
        }
        json(res, 200, profile);
        return;
      }

      if (method === "PUT") {
        const body = JSON.parse(await readBody(req));
        const updated = updateProfile(id, body);
        if (!updated) {
          json(res, 404, { error: "Profile not found" });
          return;
        }
        json(res, 200, updated);
        return;
      }

      if (method === "DELETE") {
        const deleted = removeProfile(id);
        json(res, deleted ? 200 : 404, { ok: deleted });
        return;
      }
    }

    // --- Discovery ---

    if (method === "POST" && url === "/api/discover") {
      const body = JSON.parse(await readBody(req));
      let { endpoint, apiKey } = body as { endpoint?: string; apiKey?: string };

      // If no endpoint provided, use current settings
      if (!endpoint) {
        const settings = await getSettings();
        endpoint = settings.llm_endpoint;
        if (!apiKey) apiKey = settings.llm_api_key;
      }

      const status = await probeEndpoint(endpoint, apiKey);
      json(res, 200, status);
      return;
    }

    // --- Settings (read-only, for frontend) ---

    if (method === "GET" && url === "/api/settings") {
      const settings = await getSettings();
      // Don't expose the API key to the frontend
      json(res, 200, {
        llm_endpoint: settings.llm_endpoint,
        llm_model: settings.llm_model,
        system_prompt: settings.system_prompt,
        max_tool_rounds: settings.max_tool_rounds,
      });
      return;
    }

    // Update settings
    if (method === "PUT" && url === "/api/settings") {
      const body = JSON.parse(await readBody(req));
      await updateSettings(body);
      json(res, 200, { ok: true });
      return;
    }

    // List conversations
    if (method === "GET" && url === "/api/conversations") {
      json(res, 200, listConversations());
      return;
    }

    // Create conversation
    if (method === "POST" && url === "/api/conversations") {
      const conv: Conversation = {
        id: uuidv4(),
        title: "New conversation",
        createdAt: Date.now(),
        updatedAt: Date.now(),
        messages: [],
      };
      saveConversation(conv);
      json(res, 201, { id: conv.id, title: conv.title });
      return;
    }

    // Conversation by ID routes
    const convMatch = url.match(/^\/api\/conversations\/([a-f0-9-]+)$/);
    if (convMatch) {
      const id = convMatch[1];

      if (method === "GET") {
        const conv = getConversation(id);
        if (!conv) {
          json(res, 404, { error: "Conversation not found" });
          return;
        }
        json(res, 200, conv);
        return;
      }

      if (method === "DELETE") {
        const deleted = deleteConversation(id);
        json(res, deleted ? 200 : 404, { ok: deleted });
        return;
      }

      if (method === "PATCH") {
        const body = JSON.parse(await readBody(req));
        const { title } = body as { title: string };
        const updated = updateConversationTitle(id, title);
        json(res, updated ? 200 : 404, { ok: updated });
        return;
      }
    }

    // Static files — serve built frontend
    let filePath = url === "/" ? "/index.html" : url;
    // Remove query string
    filePath = filePath.split("?")[0];
    const fullPath = path.join(publicDir, filePath);

    // Prevent directory traversal
    if (!fullPath.startsWith(publicDir)) {
      json(res, 403, { error: "Forbidden" });
      return;
    }

    if (fs.existsSync(fullPath) && fs.statSync(fullPath).isFile()) {
      const ext = path.extname(fullPath);
      const contentType = MIME_TYPES[ext] || "application/octet-stream";
      const data = fs.readFileSync(fullPath);
      res.writeHead(200, { "Content-Type": contentType });
      res.end(data);
      return;
    }

    // SPA fallback — serve index.html for unmatched routes
    const indexPath = path.join(publicDir, "index.html");
    if (fs.existsSync(indexPath)) {
      const data = fs.readFileSync(indexPath);
      res.writeHead(200, { "Content-Type": "text/html" });
      res.end(data);
      return;
    }

    json(res, 404, { error: "Not found" });
  } catch (err) {
    console.error("Request error:", err);
    json(res, 500, {
      error: err instanceof Error ? err.message : "Internal server error",
    });
  }
});

server.listen(PORT, () => {
  console.log(`Nexus Agent server running on port ${PORT}`);
});
