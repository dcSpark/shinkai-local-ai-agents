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
  FileTypeSupported,
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
  const hasStreamCompletedRef = useRef(false);

  // Streaming store actions - only subscribe to the methods we need
  const startStreaming = useStreamingStore((state) => state.startStreaming);
  const appendContent = useStreamingStore((state) => state.appendContent);
  const appendReasoning = useStreamingStore((state) => state.appendReasoning);
  const endStreaming = useStreamingStore((state) => state.endStreaming);
  const getStreamingContent = useStreamingStore(
    (state) => state.getStreamingContent,
  );

  // Token buffering refs for batched updates to streaming store
  const tokenBufferRef = useRef('');
  const reasoningBufferRef = useRef('');
  const flushScheduledRef = useRef(false);

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

        // Clear streaming store AFTER React Query is updated
        // This way when StreamingMessage falls back to React Query data,
        // it already has the correct content
        endStreaming(inboxId);

        // Only invalidate if there are tool calls that might have generated files
        // For non-tool messages, the content from setQueryData is sufficient
        // and invalidation would cause unnecessary flash
        const hasToolCalls = finalToolCalls.length > 0;
        if (hasToolCalls) {
          setTimeout(() => {
            void queryClient.invalidateQueries({
              queryKey,
            });
          }, 1000);
        }
      }
    },
    [
      appendContent,
      appendReasoning,
      endStreaming,
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
          // Reset streaming state for new message
          // NOTE: The optimistic assistant message is already created in useSendMessageToJob.onMutate
          // We just need to initialize the streaming store here
          hasStreamCompletedRef.current = false;
          tokenBufferRef.current = '';
          reasoningBufferRef.current = '';

          // Start streaming in the dedicated store
          startStreaming(inboxId);
          return;
        }
        if (isAssistantMessage && !isStreamSupported.current) {
          void queryClient.invalidateQueries({ queryKey: queryKey });
          return;
        }
        isStreamSupported.current = false;

        // finalize the optimistic assistant message immediately when the final assistant message arrives
        if (isAssistantMessage && shinkaiMsg) {
          // If streaming already completed via is_done, skip redundant processing
          if (hasStreamCompletedRef.current) {
            return;
          }

          // Extract metadata from the already-parsed ShinkaiMessage
          // This contains the real message ID (signature), content, reasoning, and tool calls
          try {
            const messageRawContent = JSON.parse(
              shinkaiMsg.body?.unencrypted?.message_data?.unencrypted
                ?.message_raw_content ?? '{}',
            );

            // Extract the real message ID from internal signature (this is the node_message_hash)
            const realMessageId =
              shinkaiMsg.body?.unencrypted?.internal_metadata?.signature ??
              shinkaiMsg.external_metadata?.signature;

            // Extract timestamp for createdAt
            const createdAt =
              shinkaiMsg.external_metadata?.scheduled_time ??
              new Date().toISOString();

            // Extract the real content and metadata
            const realContent = messageRawContent?.content ?? '';
            const realReasoning = messageRawContent?.reasoning_content;
            const functionCalls =
              messageRawContent?.metadata?.function_calls ?? [];

            // Extract TPS and duration metadata
            const tps = messageRawContent?.metadata?.tps;
            const durationMs = messageRawContent?.metadata?.duration_ms;

            // Convert function calls to tool calls format, extracting generated files
            const toolCalls: ToolCall[] = functionCalls.map(
              (fc: {
                name: string;
                arguments: Record<string, unknown>;
                tool_router_key: string;
                response?: string;
              }) => {
                // Extract __created_files__ from the response
                let generatedFiles: ToolCall['generatedFiles'];
                if (fc.response) {
                  try {
                    const responseData = JSON.parse(fc.response);
                    // Check for __created_files__ in multiple locations
                    const files: string[] | undefined =
                      responseData?.data?.__created_files__ ??
                      responseData?.__created_files__;

                    if (files && files.length > 0) {
                      // Create attachment objects with file paths
                      // Full previews will be loaded by the invalidation or lazily
                      generatedFiles = files.map((filePath: string) => {
                        const fileName =
                          filePath.split('/').pop() ?? 'unknown_file';
                        const extension = fileName.split('.').pop() ?? '';
                        return {
                          id: filePath,
                          path: filePath,
                          name: fileName,
                          extension,
                          type: FileTypeSupported.Unknown,
                          mimeType: 'application/octet-stream',
                        };
                      });
                    }
                  } catch {
                    // Failed to parse response, no generated files
                  }
                }

                return {
                  toolRouterKey: fc.tool_router_key,
                  name: fc.name,
                  args: fc.arguments,
                  status: 'Complete' as const,
                  result: fc.response ?? '',
                  generatedFiles,
                };
              },
            );

            // Flush any remaining buffered tokens and finalize with real data
            flushTokenBuffer({
              markComplete: true,
              finalMessageId: realMessageId,
              createdAt,
              realContent,
              realReasoning,
              toolCalls: toolCalls.length > 0 ? toolCalls : undefined,
              metadata: { tps, durationMs },
            });
          } catch (parseError) {
            console.error('Failed to parse ShinkaiMessage:', parseError);
            // Fallback to basic finalization
            flushTokenBuffer({ markComplete: true });
          }

          // Mark streaming as completed to prevent duplicate processing
          hasStreamCompletedRef.current = true;
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

        // Check if streaming is complete via metadata
        if (parseData.metadata?.is_done && !hasStreamCompletedRef.current) {
          hasStreamCompletedRef.current = true;
          // Flush any remaining tokens and finalize with the real message ID
          flushTokenBuffer({
            finalMessageId: parseData.metadata?.id,
            markComplete: true,
          });
          return;
        }

        // Schedule flush at ~20 FPS for smooth updates
        if (!flushScheduledRef.current) {
          flushScheduledRef.current = true;
          setTimeout(() => flushTokenBuffer(), FLUSH_INTERVAL_MS);
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
    getStreamingContent,
  ]);

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

    // Unsubscribe from previous inbox if different
    if (previousInboxIdRef.current && previousInboxIdRef.current !== inboxId) {
      unsubscribe(previousInboxIdRef.current);
      // NOTE: Don't clear streaming state here! User might switch back to this chat
    }

    // Subscribe to current inbox
    subscribe(inboxId);
    previousInboxIdRef.current = inboxId;

    // Cleanup: unsubscribe on unmount or when disabled
    return () => {
      unsubscribe(inboxId);
      previousInboxIdRef.current = null;
      // NOTE: Don't clear streaming state on navigation/unmount!
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

          // Check if we're currently streaming - if so, update the streaming store
          const streamingContent = getStreamingContent(inboxId);
          if (streamingContent?.isStreaming) {
            const toolCall: ToolCall = {
              name: tool.tool_name,
              args: tool?.args ?? tool?.args?.arguments,
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
                      args: tool?.args ?? tool?.args?.arguments,
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
