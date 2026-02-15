import http from "node:http";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { v4 as uuidv4 } from "uuid";
import { getAccessToken } from "./auth.js";
import { resolveFrontendToolResult } from "./agent.js";
import {
  listConversations,
  getConversation,
  saveConversation,
  deleteConversation,
  updateConversationTitle,
  appendRepositoryMessage,
} from "./storage.js";
import {
  listAgents,
  getAgent,
  createAgent,
  updateAgent,
  deleteAgent as removeAgent,
  getActiveAgentId,
  setActiveAgentId,
} from "./agents.js";
import {
  listProviders,
  getProvider,
  getProviderPublic,
  createProvider,
  updateProvider,
  deleteProvider as removeProvider,
} from "./providers.js";
import { getToolSettings, updateToolSettings } from "./tool-settings.js";
import { startToolEventListener } from "./tool-events.js";
import { attach as attachWebSocket } from "./ws-handler.js";
import { probeEndpoint, probeProvider } from "./discovery.js";
import { getSettings, updateSettings } from "./settings.js";
import { ToolExecutor } from "./tools/executor.js";
import { setTitleTool } from "./tools/handlers/local.js";
import { fetchMcpToolHandlers } from "./tools/handlers/remote.js";
import {
  FrontendToolBridge,
  createClipboardTools,
} from "./tools/handlers/frontend.js";
import { handleMcpCall } from "./mcp-handler.js";
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

    // MCP tool call handler — Nexus dispatches here
    if (method === "POST" && url === "/mcp/call") {
      const body = JSON.parse(await readBody(req));
      const result = await handleMcpCall(body);
      json(res, 200, result);
      return;
    }

    // Config endpoint — frontend gets token + apiUrl
    if (url === "/api/config") {
      const token = await getAccessToken();
      json(res, 200, { token, apiUrl: NEXUS_API_URL });
      return;
    }

    // Legacy chat respond (backward compat — will be removed)
    if (method === "POST" && url === "/api/chat/respond") {
      const body = JSON.parse(await readBody(req));
      const { tool_use_id, content } = body as {
        tool_use_id: string;
        content: unknown;
      };

      const resolved = resolveFrontendToolResult(
        tool_use_id,
        typeof content === "string" ? content : JSON.stringify(content),
        false,
      );
      if (resolved) {
        json(res, 200, { ok: true });
      } else {
        json(res, 404, { error: "No pending tool request with that ID" });
      }
      return;
    }

    // --- Provider routes ---

    if (method === "GET" && url === "/api/providers") {
      json(res, 200, await listProviders());
      return;
    }

    if (method === "POST" && url === "/api/providers") {
      const body = JSON.parse(await readBody(req));
      if (!body.name || !body.type) {
        json(res, 400, { error: "name and type are required" });
        return;
      }
      const provider = await createProvider(body);
      json(res, 201, provider);
      return;
    }

    // Probe provider data — optionally merges stored secrets when `id` is provided
    if (method === "POST" && url === "/api/providers/probe") {
      const body = JSON.parse(await readBody(req));
      if (!body.type) {
        json(res, 400, { error: "type is required" });
        return;
      }
      // If editing an existing provider, fill in missing secrets from storage
      if (body.id) {
        const stored = await getProvider(body.id);
        if (stored) {
          if (!body.apiKey && stored.apiKey) body.apiKey = stored.apiKey;
          if (!body.awsAccessKeyId && stored.awsAccessKeyId) body.awsAccessKeyId = stored.awsAccessKeyId;
          if (!body.awsSecretAccessKey && stored.awsSecretAccessKey) body.awsSecretAccessKey = stored.awsSecretAccessKey;
          if (!body.awsSessionToken && stored.awsSessionToken) body.awsSessionToken = stored.awsSessionToken;
        }
      }
      const status = await probeProvider(body as import("./types.js").Provider);
      json(res, 200, status);
      return;
    }

    // Probe saved provider by ID
    const probeMatch = url.match(/^\/api\/providers\/([a-f0-9-]+)\/probe$/);
    if (method === "POST" && probeMatch) {
      const provider = await getProvider(probeMatch[1]);
      if (!provider) {
        json(res, 404, { error: "Provider not found" });
        return;
      }
      const status = await probeProvider(provider);
      json(res, 200, status);
      return;
    }

    // Single provider routes
    const providerMatch = url.match(/^\/api\/providers\/([a-f0-9-]+)$/);
    if (providerMatch) {
      const id = providerMatch[1];

      if (method === "GET") {
        const provider = await getProviderPublic(id);
        if (!provider) {
          json(res, 404, { error: "Provider not found" });
          return;
        }
        json(res, 200, provider);
        return;
      }

      if (method === "PUT") {
        const body = JSON.parse(await readBody(req));
        const updated = await updateProvider(id, body);
        if (!updated) {
          json(res, 404, { error: "Provider not found" });
          return;
        }
        json(res, 200, updated);
        return;
      }

      if (method === "DELETE") {
        const deleted = await removeProvider(id);
        json(res, deleted ? 200 : 404, { ok: deleted });
        return;
      }
    }

    // --- Agent routes ---

    if (method === "GET" && url === "/api/agents") {
      json(res, 200, listAgents());
      return;
    }

    if (method === "POST" && url === "/api/agents") {
      const body = JSON.parse(await readBody(req));
      if (!body.name || !body.providerId || !body.model) {
        json(res, 400, { error: "name, providerId, and model are required" });
        return;
      }
      const agent = createAgent(body);
      json(res, 201, agent);
      return;
    }

    if (method === "GET" && url === "/api/agents/active") {
      json(res, 200, { agentId: getActiveAgentId() });
      return;
    }

    if (method === "PUT" && url === "/api/agents/active") {
      const body = JSON.parse(await readBody(req));
      const { agentId } = body as { agentId: string | null };
      setActiveAgentId(agentId);
      json(res, 200, { agentId });
      return;
    }

    const agentMatch = url.match(/^\/api\/agents\/([a-f0-9-]+)$/);
    if (agentMatch) {
      const id = agentMatch[1];

      if (method === "GET") {
        const agent = getAgent(id);
        if (!agent) {
          json(res, 404, { error: "Agent not found" });
          return;
        }
        json(res, 200, agent);
        return;
      }

      if (method === "PUT") {
        const body = JSON.parse(await readBody(req));
        const updated = updateAgent(id, body);
        if (!updated) {
          json(res, 404, { error: "Agent not found" });
          return;
        }
        json(res, 200, updated);
        return;
      }

      if (method === "DELETE") {
        const deleted = removeAgent(id);
        json(res, deleted ? 200 : 404, { ok: deleted });
        return;
      }
    }

    // --- Tool settings ---

    if (method === "GET" && url === "/api/tool-settings") {
      json(res, 200, await getToolSettings());
      return;
    }

    if (method === "PUT" && url === "/api/tool-settings") {
      const body = JSON.parse(await readBody(req));
      const updated = await updateToolSettings(body);
      json(res, 200, updated);
      return;
    }

    // --- Available tools list ---

    if (method === "GET" && url === "/api/tools") {
      const executor = new ToolExecutor();
      executor.register(setTitleTool);
      executor.registerAll(createClipboardTools(new FrontendToolBridge()));
      executor.registerAll(await fetchMcpToolHandlers());
      const tools = executor.definitions().map((d) => ({
        name: d.name,
        description: d.description,
        source: d.name.startsWith("_nexus_") ? "built-in" : "mcp",
      }));
      json(res, 200, tools);
      return;
    }

    // --- Discovery ---

    if (method === "POST" && url === "/api/discover") {
      const body = JSON.parse(await readBody(req));
      let { endpoint, apiKey } = body as { endpoint?: string; apiKey?: string };

      if (!endpoint) {
        const settings = await getSettings();
        endpoint = settings.llm_endpoint;
        if (!apiKey) apiKey = settings.llm_api_key;
      }

      const status = await probeEndpoint(endpoint, apiKey);
      json(res, 200, status);
      return;
    }

    // --- Settings (legacy, for backward compat) ---

    if (method === "GET" && url === "/api/settings") {
      const settings = await getSettings();
      json(res, 200, {
        llm_endpoint: settings.llm_endpoint,
        llm_model: settings.llm_model,
        system_prompt: settings.system_prompt,
        max_tool_rounds: settings.max_tool_rounds,
      });
      return;
    }

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

    // Append message to conversation repository (tree-structured persistence)
    const convMsgMatch = url.match(/^\/api\/conversations\/([a-f0-9-]+)\/messages$/);
    if (method === "POST" && convMsgMatch) {
      const id = convMsgMatch[1];
      const body = JSON.parse(await readBody(req));
      const { message, parentId } = body as { message: unknown; parentId: string | null };
      const ok = appendRepositoryMessage(id, message, parentId);
      json(res, ok ? 200 : 404, { ok });
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

attachWebSocket(server);

server.listen(PORT, () => {
  console.log(`Nexus Agent server running on port ${PORT}`);
  startToolEventListener();
});
