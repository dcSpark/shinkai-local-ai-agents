import {
  type ToolCall,
  type TextStatus,
} from '@shinkai_network/shinkai-node-state/v2/queries/getChatConversation/types';
import type React from 'react';
import { createContext, useContext, useState } from 'react';
import { createStore, useStore } from 'zustand';

/**
 * Streaming data for a single inbox - holds ephemeral content during streaming
 * This is kept separate from React Query to avoid expensive re-renders of the entire message list
 */
export type StreamingContent = {
  content: string;
  reasoning: {
    text: string;
    status: TextStatus;
  } | null;
  toolCalls: ToolCall[];
  isStreaming: boolean;
};

type StreamingStore = {
  /**
   * Map of inboxId -> streaming content
   * Only the currently streaming message is stored here
   */
  streams: Map<string, StreamingContent>;

  /**
   * Get streaming content for a specific inbox
   */
  getStreamingContent: (inboxId: string) => StreamingContent | undefined;

  /**
   * Start streaming for an inbox - initializes the streaming state
   */
  startStreaming: (inboxId: string) => void;

  /**
   * Append content to the streaming message
   */
  appendContent: (inboxId: string, content: string) => void;

  /**
   * Append reasoning to the streaming message
   */
  appendReasoning: (inboxId: string, reasoning: string) => void;

  /**
   * Mark reasoning as complete
   */
  completeReasoning: (inboxId: string) => void;

  /**
   * Update tool calls for the streaming message
   */
  updateToolCall: (inboxId: string, toolCall: ToolCall, index: number) => void;

  /**
   * End streaming for an inbox - marks as not streaming but preserves content
   * until React Query updates with final data
   */
  endStreaming: (inboxId: string) => void;

  /**
   * Clear streaming data for a specific inbox
   * Call this after React Query has been successfully updated with final data
   */
  clearInbox: (inboxId: string) => void;

  /**
   * Clear all streaming state (useful on logout/cleanup)
   */
  clearAll: () => void;
};

const createStreamingStore = () =>
  createStore<StreamingStore>((set, get) => ({
    streams: new Map(),

    getStreamingContent: (inboxId: string) => {
      return get().streams.get(inboxId);
    },

    startStreaming: (inboxId: string) => {
      set((state) => {
        const newStreams = new Map(state.streams);
        newStreams.set(inboxId, {
          content: '',
          reasoning: null,
          toolCalls: [],
          isStreaming: true,
        });
        return { streams: newStreams };
      });
    },

    appendContent: (inboxId: string, content: string) => {
      set((state) => {
        const current = state.streams.get(inboxId);
        if (!current?.isStreaming) return state;

        const newStreams = new Map(state.streams);
        newStreams.set(inboxId, {
          ...current,
          content: current.content + content,
          // Mark reasoning as complete when content starts
          reasoning: current.reasoning
            ? {
                ...current.reasoning,
                status: { type: 'complete', reason: 'unknown' },
              }
            : null,
        });
        return { streams: newStreams };
      });
    },

    appendReasoning: (inboxId: string, reasoning: string) => {
      set((state) => {
        const current = state.streams.get(inboxId);
        if (!current?.isStreaming) return state;

        const newStreams = new Map(state.streams);
        const currentReasoning = current.reasoning ?? {
          text: '',
          status: { type: 'running' as const },
        };
        newStreams.set(inboxId, {
          ...current,
          reasoning: {
            ...currentReasoning,
            text: currentReasoning.text + reasoning,
            status: { type: 'running' },
          },
        });
        return { streams: newStreams };
      });
    },

    completeReasoning: (inboxId: string) => {
      set((state) => {
        const current = state.streams.get(inboxId);
        if (!current?.isStreaming || !current.reasoning) return state;

        const newStreams = new Map(state.streams);
        newStreams.set(inboxId, {
          ...current,
          reasoning: {
            ...current.reasoning,
            status: { type: 'complete', reason: 'unknown' },
          },
        });
        return { streams: newStreams };
      });
    },

    updateToolCall: (inboxId: string, toolCall: ToolCall, index: number) => {
      set((state) => {
        const current = state.streams.get(inboxId);
        if (!current?.isStreaming) return state;

        const newStreams = new Map(state.streams);
        const newToolCalls = [...current.toolCalls];

        if (index < newToolCalls.length) {
          // Update existing tool call
          newToolCalls[index] = { ...newToolCalls[index], ...toolCall };
        } else {
          // Add new tool call
          newToolCalls.push(toolCall);
        }

        newStreams.set(inboxId, {
          ...current,
          toolCalls: newToolCalls,
        });
        return { streams: newStreams };
      });
    },

    endStreaming: (inboxId: string) => {
      set((state) => {
        const current = state.streams.get(inboxId);
        if (!current) return state;

        // Don't delete the entry - just mark as not streaming
        // This keeps the content available for StreamingMessage to use
        // until React Query updates with the final data
        const newStreams = new Map(state.streams);
        newStreams.set(inboxId, {
          ...current,
          isStreaming: false,
        });
        return { streams: newStreams };
      });
    },

    clearInbox: (inboxId: string) => {
      set((state) => {
        if (!state.streams.has(inboxId)) return state;

        const newStreams = new Map(state.streams);
        newStreams.delete(inboxId);
        return { streams: newStreams };
      });
    },

    clearAll: () => {
      set({ streams: new Map() });
    },
  }));

const StreamingContext = createContext<ReturnType<
  typeof createStreamingStore
> | null>(null);

export const StreamingProvider = ({
  children,
}: {
  children: React.ReactNode;
}) => {
  const [store] =
    useState<ReturnType<typeof createStreamingStore>>(createStreamingStore);

  return (
    <StreamingContext.Provider value={store}>
      {children}
    </StreamingContext.Provider>
  );
};

export function useStreamingStore<T>(selector: (state: StreamingStore) => T) {
  const store = useContext(StreamingContext);
  if (!store) {
    throw new Error('Missing StreamingProvider');
  }
  const value = useStore(store, selector);
  return value;
}

/**
 * Hook to get streaming content for a specific inbox
 * Returns undefined if not streaming
 */
export function useStreamingContent(inboxId: string) {
  return useStreamingStore((state) => state.streams.get(inboxId));
}

/**
 * Hook to check if an inbox is currently streaming
 * Returns true only when actively streaming (not when content is preserved after completion)
 */
export function useIsStreaming(inboxId: string) {
  return useStreamingStore(
    (state) => state.streams.get(inboxId)?.isStreaming ?? false,
  );
}
