import Anthropic from "@anthropic-ai/sdk";

export const UI_TOOL_PREFIX = "_nexus_";

export function isUiTool(name: string): boolean {
  return name.startsWith(UI_TOOL_PREFIX);
}

export function getUiTools(): Anthropic.Tool[] {
  return [
    {
      name: "_nexus_elicit",
      description:
        "Ask the user for structured input. Use when you need clarification, preferences, or decisions. Provide a message explaining why and a JSON Schema for the expected response.",
      input_schema: {
        type: "object" as const,
        properties: {
          message: {
            type: "string",
            description: "Human-readable message explaining why this input is needed",
          },
          requestedSchema: {
            type: "object",
            description:
              "JSON Schema (flat object with primitive properties) defining expected response structure",
          },
        },
        required: ["message", "requestedSchema"],
      },
    },
    {
      name: "_nexus_surface",
      description:
        "Render a rich interactive UI surface inline in the conversation. Use for data displays, tables, interactive forms, or any visual content beyond plain text. Provide an A2UI component tree.",
      input_schema: {
        type: "object" as const,
        properties: {
          title: { type: "string", description: "Surface title" },
          components: {
            type: "array",
            description: "A2UI component tree — flat list of components with ID references",
            items: {
              type: "object",
              properties: {
                id: { type: "string" },
                type: { type: "string" },
                properties: { type: "object" },
                children: { type: "array", items: { type: "string" } },
              },
              required: ["id", "type"],
            },
          },
          data: { type: "object", description: "Data model for component bindings" },
          interactive: {
            type: "boolean",
            description: "Whether this surface collects user input",
            default: false,
          },
        },
        required: ["components"],
      },
    },
    {
      name: "_nexus_clipboard_read",
      description: "Read the current contents of the user's clipboard.",
      input_schema: {
        type: "object" as const,
        properties: {},
        required: [],
      },
    },
    {
      name: "_nexus_clipboard_write",
      description: "Write text to the user's clipboard.",
      input_schema: {
        type: "object" as const,
        properties: {
          text: { type: "string", description: "Text to write to clipboard" },
        },
        required: ["text"],
      },
    },
  ];
}

/** Returns true if this UI tool is interactive (requires user response to continue) */
export function isInteractiveUiTool(name: string, input: Record<string, unknown>): boolean {
  if (name === "_nexus_elicit") return true;
  if (name === "_nexus_surface") return input.interactive === true;
  // Browser tools resolve on the frontend without user interaction
  return false;
}
