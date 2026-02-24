import { useCallback, useRef } from "react";
import { useNavigate } from "react-router";
import { useThreadListStore } from "../stores/threadListStore";
import { useThreadStore, EMPTY_CONV } from "../stores/threadStore";
import type { ChatMessage } from "../stores/threadStore";
import { eventBus } from "../runtime/event-bus";
import { startChat, branchChat, regenerateChat, abortChat } from "../api/client";
import { snowflake } from "../lib/snowflake";
import { consumeStream } from "../lib/stream-consumer";

export function useChatStream(): {
  sendMessage: (text: string) => void;
  branchMessage: (messageId: string, text: string) => void;
  regenerate: (userMessageId: string) => void;
  abort: () => void;
  isStreaming: boolean;
} {
  const navigate = useNavigate();
  const abortRef = useRef<AbortController | null>(null);
  const activeThreadId = useThreadListStore((s) => s.activeThreadId);
  const isStreaming = useThreadStore(
    (s) => s.conversations[activeThreadId ?? ""]?.isStreaming ?? false,
  );

  const sendMessage = useCallback(async (text: string) => {
    let conversationId = useThreadListStore.getState().activeThreadId;
    if (!conversationId) {
      conversationId = await useThreadListStore.getState().createThread();
      navigate(`/c/${conversationId}`, { replace: true });
    }

    const userMsgId = snowflake();
    const assistantMsgId = snowflake();

    const store = useThreadStore.getState();
    const parentId = store.getLastMessageId(conversationId);
    const userMessage = store.appendUserMessage(conversationId, text, userMsgId);
    store.startStreaming(conversationId, assistantMsgId);

    await eventBus.ensureConnected();

    const controller = new AbortController();
    abortRef.current = controller;

    try {
      await startChat(conversationId, text, userMsgId, assistantMsgId);
    } catch (err: unknown) {
      console.error("Turn start failed:", err);
      useThreadStore.getState().finalizeStreaming(conversationId, {
        type: "incomplete",
        reason: "error",
        error: String(err),
      });
      return;
    }

    consumeStream(conversationId, controller.signal, {
      conversationId,
      userMessage,
      parentId,
      assistantMessageId: assistantMsgId,
    });
  }, []);

  const branchMessage = useCallback(async (messageId: string, text: string) => {
    const conversationId = useThreadListStore.getState().activeThreadId;
    if (!conversationId) return;

    const store = useThreadStore.getState();
    const conv = store.conversations[conversationId] ?? { ...EMPTY_CONV };
    const branchIdx = conv.messages.findIndex((m) => m.id === messageId);
    if (branchIdx === -1) return;

    const parentId = branchIdx > 0 ? conv.messages[branchIdx - 1].id : null;
    const kept = conv.messages.slice(0, branchIdx);

    const userMsgId = snowflake();
    const assistantMsgId = snowflake();

    const userMessage: ChatMessage = {
      id: userMsgId,
      role: "user",
      parts: [{ type: "text", text }],
      createdAt: new Date(),
    };
    const streamingMsg: ChatMessage = {
      id: assistantMsgId,
      role: "assistant",
      parts: [],
      createdAt: new Date(),
      status: { type: "streaming" },
    };

    useThreadStore.getState().replaceMessages(
      conversationId,
      [...kept, userMessage, streamingMsg],
      true,
    );

    await eventBus.ensureConnected();

    const controller = new AbortController();
    abortRef.current = controller;

    try {
      await branchChat(conversationId, messageId, text, userMsgId, assistantMsgId);
    } catch (err: unknown) {
      console.error("Branch start failed:", err);
      useThreadStore.getState().finalizeStreaming(conversationId, {
        type: "incomplete",
        reason: "error",
        error: String(err),
      });
      return;
    }

    consumeStream(conversationId, controller.signal, {
      conversationId,
      userMessage,
      parentId,
      assistantMessageId: assistantMsgId,
    });
  }, []);

  const regenerate = useCallback(async (userMessageId: string) => {
    const conversationId = useThreadListStore.getState().activeThreadId;
    if (!conversationId) return;

    const store = useThreadStore.getState();
    const conv = store.conversations[conversationId] ?? { ...EMPTY_CONV };

    const userIdx = conv.messages.findIndex((m) => m.id === userMessageId);
    if (userIdx === -1) return;
    const userMessage = conv.messages[userIdx];

    const assistantMsgId = snowflake();
    const messagesUpToUser = conv.messages.slice(0, userIdx + 1);
    const streamingMsg: ChatMessage = {
      id: assistantMsgId,
      role: "assistant",
      parts: [],
      createdAt: new Date(),
      status: { type: "streaming" },
    };

    useThreadStore.getState().replaceMessages(
      conversationId,
      [...messagesUpToUser, streamingMsg],
      true,
    );

    await eventBus.ensureConnected();

    const controller = new AbortController();
    abortRef.current = controller;

    try {
      await regenerateChat(conversationId, userMessageId, assistantMsgId);
    } catch (err: unknown) {
      console.error("Regenerate failed:", err);
      useThreadStore.getState().finalizeStreaming(conversationId, {
        type: "incomplete",
        reason: "error",
        error: String(err),
      });
      return;
    }

    consumeStream(conversationId, controller.signal, {
      conversationId,
      userMessage,
      parentId: userIdx > 0 ? conv.messages[userIdx - 1].id : null,
      assistantMessageId: assistantMsgId,
      skipUserPersist: true,
    });
  }, []);

  const abort = useCallback(() => {
    const conversationId = useThreadListStore.getState().activeThreadId;
    if (conversationId) {
      abortChat(conversationId);
    }
    abortRef.current?.abort();
  }, []);

  return { sendMessage, branchMessage, regenerate, abort, isStreaming };
}
