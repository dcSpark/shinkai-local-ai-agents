import {
  type WidgetToolType,
  type WsMessage,
} from '@shinkai_network/shinkai-message-ts/api/general/types';
import {
  FunctionKeyV2,
  OPTIMISTIC_ASSISTANT_MESSAGE_ID,
} from '@shinkai_network/shinkai-node-state/v2/constants';
import {
  type FormattedMessage,
  type ChatConversationInfiniteData,
  type ToolCall,
} from '@shinkai_network/shinkai-node-state/v2/queries/getChatConversation/types';
import { useQueryClient } from '@tanstack/react-query';
import { useCallback, useEffect, useMemo, useRef } from 'react';
import { useParams } from 'react-router';
import useWebSocket from 'react-use-websocket';

import { useAuth } from '../../store/auth';
import { useStreamingStore } from './context/streaming-context';
import { useToolsStore } from './context/tools-context';

// Token buffering configuration for smooth streaming
const FLUSH_INTERVAL_MS = 50; // 20 FPS - smooth enough for human perception

type UseWebSocketMessage = {
  enabled?: boolean;
  inboxId: string;
};

/**
 * Constructs the WebSocket URL from the node address
 * Handles both local development and production scenarios
 */
const getWebSocketUrl = (nodeAddress: string): string => {
  const nodeAddressUrl = new URL(nodeAddress);
  const isLocalhost = ['localhost', '0.0.0.0', '127.0.0.1'].includes(
    nodeAddressUrl.hostname,
  );

  if (isLocalhost) {
    return `ws://${nodeAddressUrl.hostname}:${Number(nodeAddressUrl.port) + 1}/ws`;
  }

  const portSuffix =
    Number(nodeAddressUrl.port) !== 0 ? `:${Number(nodeAddressUrl.port)}` : '';
  return `ws://${nodeAddressUrl.hostname}${portSuffix}/ws`;
};

export const useWebSocketMessage = ({
  enabled,
  inboxId: defaultInboxId,
}: UseWebSocketMessage) => {
  const auth = useAuth((state) => state.auth);
  const socketUrl = getWebSocketUrl(
    auth?.node_address ?? 'http://localhost:9850',
  );
  const queryClient = useQueryClient();
  const isStreamSupported = useRef(false);
  // Tracks whether we've received an is_done: true from Stream messages
  // Used in combination with ShinkaiMessage is_stream: false for completion detection
  const hasReceivedStreamDone = useRef(false);

  // Streaming store actions - only subscribe to the methods we need
  const startStreaming = useStreamingStore((state) => state.startStreaming);
  const appendContent = useStreamingStore((state) => state.appendContent);
  const appendReasoning = useStreamingStore((state) => state.appendReasoning);
  const endStreaming = useStreamingStore((state) => state.endStreaming);
  const clearInbox = useStreamingStore((state) => state.clearInbox);
  const saveReasoningDuration = useStreamingStore(
    (state) => state.saveReasoningDuration,
  );
  const getStreamingContent = useStreamingStore(
    (state) => state.getStreamingContent,
  );

  // Token buffering refs for batched updates to streaming store
  const tokenBufferRef = useRef('');
  const reasoningBufferRef = useRef('');
  const flushScheduledRef = useRef(false);
  const flushTimeoutRef = useRef<NodeJS.Timeout | null>(null);
  const clearInboxTimeoutRef = useRef<NodeJS.Timeout | null>(null);
  const invalidateTimeoutRef = useRef<NodeJS.Timeout | null>(null);

  // Track the previous inboxId to handle unsubscription
  const previousInboxIdRef = useRef<string | null>(null);

  const { sendMessage, lastMessage, readyState } = useWebSocket(
    socketUrl,
    { share: true },
    enabled,
  );
  const { inboxId: encodedInboxId = '' } = useParams();
  const inboxId = defaultInboxId || decodeURIComponent(encodedInboxId);

  const queryKey = useMemo(() => {
    return [FunctionKeyV2.GET_CHAT_CONVERSATION_PAGINATION, { inboxId }];
  }, [inboxId]);

  // Flush buffered tokens to streaming store (NOT React Query)
  const flushTokenBuffer = useCallback(
    (options?: {
      finalMessageId?: string;
      markComplete?: boolean;
      realContent?: string;
      realReasoning?: string;
      toolCalls?: ToolCall[];
      createdAt?: string;
      metadata?: { tps?: number; durationMs?: number };
    }) => {
      if (flushTimeoutRef.current) {
        clearTimeout(flushTimeoutRef.current);
        flushTimeoutRef.current = null;
      }
      flushScheduledRef.current = false;

      const hasTokens = tokenBufferRef.current || reasoningBufferRef.current;
      const shouldFinalize = options?.markComplete;

      if (!hasTokens && !shouldFinalize) return;

      // Apply buffered tokens to streaming store
      if (reasoningBufferRef.current) {
        appendReasoning(inboxId, reasoningBufferRef.current);
        reasoningBufferRef.current = '';
      }
      if (tokenBufferRef.current) {
        appendContent(inboxId, tokenBufferRef.current);
        tokenBufferRef.current = '';
      }

      // Finalize if streaming is complete
      if (shouldFinalize) {
        // Get the accumulated content BEFORE clearing the streaming store
        const streamedContent = getStreamingContent(inboxId);

        // Use real data from ShinkaiMessage if available, otherwise use streamed content
        const finalContent =
          options?.realContent ?? streamedContent?.content ?? '';
        const finalReasoning = options?.realReasoning
          ? {
              text: options.realReasoning,
              status: { type: 'complete' as const, reason: 'unknown' as const },
            }
          : streamedContent?.reasoning
            ? {
                ...streamedContent.reasoning,
                status: {
                  type: 'complete' as const,
                  reason: 'unknown' as const,
                },
              }
            : undefined;
        const finalToolCalls =
          options?.toolCalls ??
          (streamedContent?.toolCalls && streamedContent.toolCalls.length > 0
            ? streamedContent.toolCalls
            : []);

        // IMPORTANT: Update React Query FIRST, then clear streaming store
        // This prevents a brief flash where StreamingMessage falls back to
        // stale React Query data before the update propagates
        queryClient.setQueryData(
          queryKey,
          (old: ChatConversationInfiniteData | undefined) => {
            if (!old?.pages?.length) return old;

            const pages = old.pages.slice();
            const lastPageIdx = pages.length - 1;
            const lastPage = pages[lastPageIdx].slice();
            const lastMsgIdx = lastPage.length - 1;
            const lastMsg = lastPage[lastMsgIdx];

            if (lastMsg?.messageId !== OPTIMISTIC_ASSISTANT_MESSAGE_ID)
              return old;
            if (
              lastMsg.role !== 'assistant' ||
              lastMsg.status?.type !== 'running'
            )
              return old;

            const updated: FormattedMessage = {
              ...lastMsg,
              messageId: options?.finalMessageId ?? lastMsg.messageId,
              createdAt: options?.createdAt ?? lastMsg.createdAt,
              content: finalContent || lastMsg.content,
              status: { type: 'complete', reason: 'unknown' },
              reasoning: finalReasoning ?? lastMsg.reasoning,
              toolCalls:
                finalToolCalls.length > 0 ? finalToolCalls : lastMsg.toolCalls,
              metadata: {
                ...lastMsg.metadata,
                // Convert tps to string since BaseMessage.metadata.tps expects string
                tps:
                  options?.metadata?.tps?.toString() ?? lastMsg.metadata?.tps,
              },
            };

            lastPage[lastMsgIdx] = updated;
            pages[lastPageIdx] = lastPage;

            return { ...old, pages };
          },
        );

        // Save reasoning duration for the last message (by inboxId) before clearing inbox
        // This preserves the duration after clearInbox is called
        if (streamedContent) {
          // Calculate final duration (already calculated in endStreaming, but get it from content)
          let durationToSave = streamedContent.reasoningDuration;
          if (durationToSave === 0 && streamedContent.reasoningStartTime) {
            // Fallback calculation if not already calculated
            durationToSave = Math.round(
              (Date.now() - streamedContent.reasoningStartTime) / 1000,
            );
          }
          if (durationToSave > 0) {
            // Save by inboxId - only tracks the last message duration
            saveReasoningDuration(inboxId, durationToSave);
          }
        }

        // Mark streaming as ended but KEEP the data (especially reasoningDuration)
        // The data will be reset when a new stream starts in this inbox
        endStreaming(inboxId);

        // Cancel any pending cleanup timeouts for this inbox
        if (clearInboxTimeoutRef.current) {
          clearTimeout(clearInboxTimeoutRef.current);
          clearInboxTimeoutRef.current = null;
        }
        if (invalidateTimeoutRef.current) {
          clearTimeout(invalidateTimeoutRef.current);
          invalidateTimeoutRef.current = null;
        }

        clearInboxTimeoutRef.current = setTimeout(() => {
          clearInboxTimeoutRef.current = null;
          clearInbox(inboxId);
        }, 2000); // 2s delay ensures React Query update has propagated
      }
    },
    [
      appendContent,
      appendReasoning,
      endStreaming,
      clearInbox,
      saveReasoningDuration,
      getStreamingContent,
      inboxId,
      queryClient,
      queryKey,
    ],
  );

  useEffect(() => {
    if (!enabled || !auth) return;
    if (lastMessage?.data) {
      try {
        const parseData: WsMessage = JSON.parse(lastMessage.data);
        if (parseData.inbox !== inboxId) return;

        // Parse the ShinkaiMessage once to avoid repeated JSON.parse calls

        const isShinkaiMessage =
          parseData.message_type === 'ShinkaiMessage' && parseData.message;
        const shinkaiMsg = isShinkaiMessage
          ? JSON.parse(parseData.message)
          : null;

        // Determine if this is a user message or assistant message
        const isFromCurrentUser =
          shinkaiMsg?.external_metadata.sender === auth.shinkai_identity &&
          shinkaiMsg?.body.unencrypted.internal_metadata.sender_subidentity ===
            auth.profile;

        const isUserMessage = isShinkaiMessage && isFromCurrentUser;
        const isAssistantMessage = isShinkaiMessage && !isFromCurrentUser;

        if (isUserMessage) {
          clearInbox(inboxId);

          hasReceivedStreamDone.current = false;

          tokenBufferRef.current = '';
          reasoningBufferRef.current = '';

          if (flushTimeoutRef.current) {
            clearTimeout(flushTimeoutRef.current);
            flushTimeoutRef.current = null;
          }
          if (clearInboxTimeoutRef.current) {
            clearTimeout(clearInboxTimeoutRef.current);
            clearInboxTimeoutRef.current = null;
          }
          if (invalidateTimeoutRef.current) {
            clearTimeout(invalidateTimeoutRef.current);
            invalidateTimeoutRef.current = null;
          }
          flushScheduledRef.current = false;

          // Start streaming in the dedicated store
          startStreaming(inboxId);
          return;
        }
        if (isAssistantMessage && !isStreamSupported.current) {
          void queryClient.invalidateQueries({ queryKey: queryKey });
          return;
        }
        isStreamSupported.current = false;

        const isStreamEnded =
          (parseData as WsMessage & { is_stream?: boolean }).is_stream ===
          false;

        if (isAssistantMessage && isStreamEnded) {
          if (!hasReceivedStreamDone.current) {
            console.warn(
              '[WS Debug] ⚠️ Received ShinkaiMessage is_stream:false without prior is_done:true',
            );
          }

          if (tokenBufferRef.current || reasoningBufferRef.current) {
            flushTokenBuffer();
          }

          void queryClient.invalidateQueries({ queryKey });

          setTimeout(() => {
            endStreaming(inboxId);
          }, 1000);

          return;
        }

        if (isAssistantMessage && !isStreamEnded) {
          void queryClient.invalidateQueries({ queryKey });
          return;
        }

        if (parseData.message_type !== 'Stream') return;

        isStreamSupported.current = true;

        // Ensure streaming is started - handles cases where user message wasn't echoed via WebSocket
        // or when the stream starts before we detect the user message
        if (!getStreamingContent(inboxId)?.isStreaming) {
          startStreaming(inboxId);
        }

        // Buffer tokens instead of updating state directly for smoother streaming
        if (parseData.metadata?.is_reasoning) {
          reasoningBufferRef.current += parseData.message;
        } else {
          tokenBufferRef.current += parseData.message;
        }

        if (parseData.metadata?.is_done) {
          hasReceivedStreamDone.current = true;
          flushTokenBuffer();
          return;
        }

        // Schedule flush at ~20 FPS for smooth updates
        if (!flushScheduledRef.current) {
          flushScheduledRef.current = true;
          flushTimeoutRef.current = setTimeout(() => {
            flushTimeoutRef.current = null;
            flushTokenBuffer();
          }, FLUSH_INTERVAL_MS);
        }
      } catch (error) {
        console.error('Failed to parse ws message', error);
      }
    }
  }, [
    auth,
    enabled,
    inboxId,
    lastMessage?.data,
    queryClient,
    queryKey,
    flushTokenBuffer,
    startStreaming,
    clearInbox,
    endStreaming,
    getStreamingContent,
  ]);

  // Cleanup all timeouts on unmount
  useEffect(() => {
    return () => {
      if (flushTimeoutRef.current) {
        clearTimeout(flushTimeoutRef.current);
        flushTimeoutRef.current = null;
      }
      if (clearInboxTimeoutRef.current) {
        clearTimeout(clearInboxTimeoutRef.current);
        clearInboxTimeoutRef.current = null;
      }
      if (invalidateTimeoutRef.current) {
        clearTimeout(invalidateTimeoutRef.current);
        invalidateTimeoutRef.current = null;
      }
    };
  }, []);

  // Subscribe/unsubscribe to inbox topic with proper cleanup
  useEffect(() => {
    if (!enabled || !auth?.api_v2_key) return;

    const subscribe = (targetInboxId: string) => {
      const wsMessage = {
        bearer_auth: auth.api_v2_key,
        message: {
          subscriptions: [{ topic: 'inbox', subtopic: targetInboxId }],
          unsubscriptions: [],
        },
      };
      sendMessage(JSON.stringify(wsMessage));
    };

    const unsubscribe = (targetInboxId: string) => {
      const wsMessage = {
        bearer_auth: auth.api_v2_key,
        message: {
          subscriptions: [],
          unsubscriptions: [{ topic: 'inbox', subtopic: targetInboxId }],
        },
      };
      sendMessage(JSON.stringify(wsMessage));
    };

    // Subscribe to current inbox
    subscribe(inboxId);
    previousInboxIdRef.current = inboxId;

    // Cleanup: unsubscribe on unmount or when disabled
    return () => {
      unsubscribe(inboxId);
      previousInboxIdRef.current = null;
    };
  }, [auth?.api_v2_key, enabled, inboxId, sendMessage]);

  return {
    readyState,
  };
};

export const useWebSocketTools = ({
  enabled,
  inboxId: defaultInboxId,
}: UseWebSocketMessage) => {
  const auth = useAuth((state) => state.auth);
  const socketUrl = getWebSocketUrl(
    auth?.node_address ?? 'http://localhost:9850',
  );
  const { sendMessage, lastMessage, readyState } = useWebSocket(
    socketUrl,
    { share: true },
    enabled,
  );
  const { inboxId: encodedInboxId = '' } = useParams();
  const inboxId = defaultInboxId || decodeURIComponent(encodedInboxId);
  const queryClient = useQueryClient();

  const setWidget = useToolsStore((state) => state.setWidget);

  // Streaming store for tool calls
  const updateToolCall = useStreamingStore((state) => state.updateToolCall);
  const getStreamingContent = useStreamingStore(
    (state) => state.getStreamingContent,
  );

  // Track the previous inboxId for unsubscription
  const previousInboxIdRef = useRef<string | null>(null);

  const queryKey = useMemo(() => {
    return [FunctionKeyV2.GET_CHAT_CONVERSATION_PAGINATION, { inboxId }];
  }, [inboxId]);

  useEffect(() => {
    if (!enabled) return;
    if (lastMessage?.data) {
      try {
        const parseData: WsMessage = JSON.parse(lastMessage.data);
        if (parseData.inbox !== inboxId) return;

        if (
          parseData.message_type === 'Widget' &&
          parseData?.widget?.ToolRequest
        ) {
          const tool = parseData.widget.ToolRequest;

          // Check if streaming content exists (even if streaming has ended)
          // The store now allows tool call updates after streaming ends
          const streamingContent = getStreamingContent(inboxId);
          if (streamingContent) {
            const toolCall: ToolCall = {
              name: tool.tool_name,
              args: tool?.args?.arguments ?? {},
              status: tool.status.type_,
              toolRouterKey: tool?.tool_router_key ?? '',
              result: tool.result?.data.message,
            };
            updateToolCall(inboxId, toolCall, tool.index);
          } else {
            // Fallback to React Query update for non-streaming scenarios
            // Use shallow copy instead of immer
            queryClient.setQueryData(
              queryKey,
              (old: ChatConversationInfiniteData | undefined) => {
                if (!old?.pages?.[0]) return old;

                const pages = old.pages.slice();
                const lastPageIdx = pages.length - 1;
                const lastPage = pages[lastPageIdx].slice();
                const lastMsgIdx = lastPage.length - 1;
                const lastMsg = lastPage[lastMsgIdx];

                if (
                  lastMsg &&
                  lastMsg.messageId === OPTIMISTIC_ASSISTANT_MESSAGE_ID &&
                  lastMsg.role === 'assistant' &&
                  lastMsg.status?.type === 'running'
                ) {
                  const existingToolCall: ToolCall | undefined =
                    lastMsg.toolCalls?.[tool.index];

                  const newToolCalls = [...(lastMsg.toolCalls || [])];

                  if (existingToolCall) {
                    newToolCalls[tool.index] = {
                      ...newToolCalls[tool.index],
                      status: tool.status.type_,
                      result: tool.result?.data.message,
                    };
                  } else {
                    newToolCalls.push({
                      name: tool.tool_name,
                      args: tool?.args?.arguments ?? {},
                      status: tool.status.type_,
                      toolRouterKey: tool?.tool_router_key ?? '',
                      result: tool.result?.data.message,
                    });
                  }

                  const updated: FormattedMessage = {
                    ...lastMsg,
                    toolCalls: newToolCalls,
                  };
                  lastPage[lastMsgIdx] = updated;
                  pages[lastPageIdx] = lastPage;
                  return { ...old, pages };
                }
                return old;
              },
            );
          }
        }

        if (
          parseData.message_type === 'Widget' &&
          parseData?.widget?.PaymentRequest
        ) {
          const widgetName = Object.keys(parseData.widget)[0];
          setWidget({
            name: widgetName as WidgetToolType,
            data: parseData.widget[widgetName as WidgetToolType],
          });
        }
      } catch (error) {
        console.error('Failed to parse ws message', error);
      }
    }
  }, [
    enabled,
    inboxId,
    lastMessage?.data,
    queryClient,
    queryKey,
    getStreamingContent,
    updateToolCall,
    setWidget,
  ]);

  // Subscribe/unsubscribe to widget topic with proper cleanup
  useEffect(() => {
    if (!enabled || !auth?.api_v2_key) return;

    const subscribe = (targetInboxId: string) => {
      const wsMessage = {
        bearer_auth: auth.api_v2_key,
        message: {
          subscriptions: [{ topic: 'widget', subtopic: targetInboxId }],
          unsubscriptions: [],
        },
      };
      sendMessage(JSON.stringify(wsMessage));
    };

    const unsubscribe = (targetInboxId: string) => {
      const wsMessage = {
        bearer_auth: auth.api_v2_key,
        message: {
          subscriptions: [],
          unsubscriptions: [{ topic: 'widget', subtopic: targetInboxId }],
        },
      };
      sendMessage(JSON.stringify(wsMessage));
    };

    // Unsubscribe from previous inbox if different
    if (previousInboxIdRef.current && previousInboxIdRef.current !== inboxId) {
      unsubscribe(previousInboxIdRef.current);
    }

    // Subscribe to current inbox
    subscribe(inboxId);
    previousInboxIdRef.current = inboxId;

    // Cleanup: unsubscribe on unmount or when disabled
    return () => {
      unsubscribe(inboxId);
      previousInboxIdRef.current = null;
    };
  }, [auth?.api_v2_key, enabled, inboxId, sendMessage]);

  return { readyState };
};
