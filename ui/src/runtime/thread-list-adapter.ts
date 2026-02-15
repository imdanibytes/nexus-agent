import type {
  unstable_RemoteThreadListAdapter as RemoteThreadListAdapter,
} from "@assistant-ui/react";
import {
  fetchConversations,
  fetchConversation,
  deleteConversation,
  renameConversation,
  createConversation,
} from "@/api/client.js";

/**
 * Shared state between the thread list adapter and chat adapter.
 * Tracks which server conversation ID corresponds to the active thread.
 */
export const threadState = {
  /** Map from assistant-ui thread ID → server conversation ID */
  threadToConversation: new Map<string, string>(),

  /** Currently active thread's server conversation ID */
  activeConversationId: null as string | null,
};

/**
 * Adapter that connects assistant-ui's ThreadList to our server API.
 */
export class NexusThreadListAdapter implements RemoteThreadListAdapter {
  async list() {
    const convos = await fetchConversations();
    return {
      threads: convos.map((c) => ({
        remoteId: c.id,
        status: "regular" as const,
        title: c.title,
      })),
    };
  }

  async rename(remoteId: string, newTitle: string) {
    await renameConversation(remoteId, newTitle);
  }

  async archive(_remoteId: string) {}
  async unarchive(_remoteId: string) {}

  async delete(remoteId: string) {
    await deleteConversation(remoteId);
  }

  async initialize(threadId: string) {
    // Create a new server-side conversation
    const conv = await createConversation();
    threadState.threadToConversation.set(threadId, conv.id);
    threadState.activeConversationId = conv.id;
    return {
      remoteId: conv.id,
      externalId: undefined,
    };
  }

  async generateTitle(remoteId: string, _messages: any) {
    // The server generates titles via LLM after the first exchange.
    // By the time this is called (after runEnd), the title is already saved.
    // Fetch it and stream it back to assistant-ui.
    const { createAssistantStream } = await import("assistant-stream");
    const conv = await fetchConversation(remoteId);
    const title = conv?.title && conv.title !== "New conversation" ? conv.title : null;

    return createAssistantStream((controller) => {
      if (title) {
        controller.appendText(title);
      }
    });
  }

  async fetch(threadId: string) {
    // When switching to an existing thread, set it as active
    threadState.activeConversationId = threadId;
    return {
      remoteId: threadId,
      status: "regular" as const,
      title: undefined,
    };
  }
}
