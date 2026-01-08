import {
  type ToolCall,
  type TextStatus,
} from '@shinkai_network/shinkai-node-state/v2/queries/getChatConversation/types';
import type React from 'react';
import { createContext, useCallback, useContext, useState } from 'react';
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
  // reasoning client-side only for now
  /** Timestamp when reasoning started (for duration calculation) */
  reasoningStartTime: number | null;
  /** Calculated reasoning duration in seconds */
  reasoningDuration: number;
};

type StreamingStore = {
  /**
   * Map of inboxId -> streaming content
   * Only the currently streaming message is stored here
   */
  streams: Map<string, StreamingContent>;

  /**
   * Map of inboxId -> reasoning duration (in seconds) for the last message
   * Persists after clearInbox to preserve duration for the last completed message
   */
  reasoningDurations: Map<string, number>;

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
   * Save reasoning duration for the last message in an inbox
   * This persists after clearInbox to preserve duration for the last completed message
   */
  saveReasoningDuration: (inboxId: string, duration: number) => void;

  /**
   * Get reasoning duration for the last message in an inbox
   */
  getReasoningDuration: (inboxId: string) => number | undefined;

  /**
   * Clear all streaming state (useful on logout/cleanup)
   */
  clearAll: () => void;
};

const calculateReasoningDuration = (content: StreamingContent): number => {
  if (content.reasoningDuration > 0) return content.reasoningDuration;
  if (!content.reasoningStartTime) return 0;
  return Math.round((Date.now() - content.reasoningStartTime) / 1000);
};

// Maximum number of inboxes to keep reasoning durations for
const MAX_REASONING_DURATIONS = 5;

const createStreamingStore = () =>
  createStore<StreamingStore>((set, get) => ({
    streams: new Map(),
    reasoningDurations: new Map(),

    getStreamingContent: (inboxId: string) => {
      return get().streams.get(inboxId);
    },

    startStreaming: (inboxId: string) => {
      set((state) => {
        const newStreams = new Map(state.streams);
        const newDurations = new Map(state.reasoningDurations);
        newDurations.delete(inboxId);
        newStreams.set(inboxId, {
          content: '',
          reasoning: null,
          toolCalls: [],
          isStreaming: true,
          reasoningStartTime: null,
          reasoningDuration: 0,
        });
        return { streams: newStreams, reasoningDurations: newDurations };
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
          // Calculate reasoning duration when content starts (reasoning ends)
          reasoningDuration: calculateReasoningDuration(current),
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
        // Track start time when first reasoning token arrives
        const reasoningStartTime = current.reasoningStartTime ?? Date.now();
        newStreams.set(inboxId, {
          ...current,
          reasoning: {
            ...currentReasoning,
            text: currentReasoning.text + reasoning,
            status: { type: 'running' },
          },
          reasoningStartTime,
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
          reasoningDuration: calculateReasoningDuration(current),
        });
        return { streams: newStreams };
      });
    },

    updateToolCall: (inboxId: string, toolCall: ToolCall, index: number) => {
      set((state) => {
        const current = state.streams.get(inboxId);
        if (!current) return state;

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
          reasoningDuration: calculateReasoningDuration(current),
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

    saveReasoningDuration: (inboxId: string, duration: number) => {
      set((state) => {
        const newDurations = new Map(state.reasoningDurations);
        // Remove the inbox if it already exists (to update its position to "most recent")
        if (newDurations.has(inboxId)) {
          newDurations.delete(inboxId);
        }
        // Add the new/updated inbox duration
        newDurations.set(inboxId, duration);

        // Keep only the last MAX_REASONING_DURATIONS inboxes
        // Map iterates in insertion order, so remove oldest entries first
        if (newDurations.size > MAX_REASONING_DURATIONS) {
          const entriesToRemove = newDurations.size - MAX_REASONING_DURATIONS;
          const keysToRemove = Array.from(newDurations.keys()).slice(
            0,
            entriesToRemove,
          );
          for (const key of keysToRemove) {
            newDurations.delete(key);
          }
        }

        return { reasoningDurations: newDurations };
      });
    },

    getReasoningDuration: (inboxId: string) => {
      return get().reasoningDurations.get(inboxId);
    },

    clearAll: () => {
      set({ streams: new Map(), reasoningDurations: new Map() });
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
  const selector = useCallback(
    (state: StreamingStore) => state.streams.get(inboxId),
    [inboxId],
  );
  return useStreamingStore(selector);
}

/**
 * Hook to check if an inbox is currently streaming
 * Returns true only when actively streaming (not when content is preserved after completion)
 */
export function useIsStreaming(inboxId: string) {
  const selector = useCallback(
    (state: StreamingStore) => state.streams.get(inboxId)?.isStreaming ?? false,
    [inboxId],
  );
  return useStreamingStore(selector);
}

/**
 * Hook to get the reasoning duration for the last message in an inbox
 * Returns the calculated duration in seconds, or 0 if not available
 * Only tracks the last message duration (persists after clearInbox)
 * Returns 0 if inboxId is empty string (to avoid showing duration for non-last messages)
 */
export function useReasoningDuration(inboxId: string) {
  const selector = useCallback(
    (state: StreamingStore) => {
      // Return 0 if inboxId is empty (means it's not the last message)
      if (!inboxId) return 0;
      // Check persistent storage for the last message duration
      const persisted = state.reasoningDurations.get(inboxId);
      if (persisted !== undefined) return persisted;
      // Fallback to 0 if not found
      return 0;
    },
    [inboxId],
  );
  return useStreamingStore(selector);
}
