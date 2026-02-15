import { useCallback } from "react";
import { useChatStore } from "../stores/chatStore.js";
import {
  streamChat,
  fetchConversations,
  fetchConversation,
  respondToUiSurface,
} from "../api/client.js";
import type { Message, ToolCallInfo } from "../api/client.js";

export function useStreamingChat() {
  const store = useChatStore();

  const sendMessage = useCallback(
    async (message: string) => {
      const state = useChatStore.getState();
      if (state.isStreaming) return;

      const conversationId = state.activeId;
      const profileId = state.activeProfileId;

      // Add user message to UI immediately
      const userMsg: Message = {
        id: crypto.randomUUID(),
        role: "user",
        content: message,
        timestamp: Date.now(),
      };
      store.addUserMessage(userMsg);
      store.startStreaming();

      try {
        let assistantText = "";
        const toolCalls: ToolCallInfo[] = [];

        for await (const event of streamChat(conversationId, message, profileId)) {
          const data = event.data as Record<string, unknown>;

          switch (event.event) {
            case "text_delta":
              assistantText += (data.text as string) || "";
              store.appendStreamingText((data.text as string) || "");
              break;

            case "tool_start":
              store.addToolCall({
                id: data.id as string,
                name: data.name as string,
                args: {},
              });
              break;

            case "tool_result": {
              const id = data.id as string;
              const content = data.content as string;
              const isError = (data.is_error as boolean) || false;
              store.updateToolCallResult(id, content, isError);
              toolCalls.push({
                id,
                name: data.name as string,
                args: {},
                result: content,
                isError,
              });
              break;
            }

            case "ui_surface":
              store.addUiSurface({
                toolUseId: data.tool_use_id as string,
                name: data.name as string,
                input: data.input as Record<string, unknown>,
                responded: false,
              });
              break;

            case "title_update":
              if (conversationId) {
                store.updateTitle(conversationId, data.title as string);
              }
              break;

            case "turn_end": {
              const finalMsg: Message = {
                id: crypto.randomUUID(),
                role: "assistant",
                content: assistantText,
                timestamp: Date.now(),
                toolCalls: toolCalls.length > 0 ? toolCalls : undefined,
              };
              store.finishStreaming(finalMsg);

              // Refresh conversation list
              const convos = await fetchConversations();
              store.setConversations(convos);

              // If we didn't have an activeId, set it now
              if (!conversationId && convos.length > 0) {
                const newest = convos[0];
                store.setActiveId(newest.id);
              }
              break;
            }

            case "error":
              console.error("Stream error:", data.message);
              store.finishStreaming();
              break;
          }
        }
      } catch (err) {
        console.error("Chat error:", err);
        store.finishStreaming();
      }
    },
    [store]
  );

  const respondToSurface = useCallback(
    async (toolUseId: string, action: string, content: unknown) => {
      store.resolveUiSurface(toolUseId);
      await respondToUiSurface(toolUseId, action, content);
    },
    [store]
  );

  const loadConversation = useCallback(
    async (id: string) => {
      store.setActiveId(id);
      const conv = await fetchConversation(id);
      if (conv) {
        store.setMessages(conv.messages);
      }
    },
    [store]
  );

  return { sendMessage, respondToSurface, loadConversation };
}
