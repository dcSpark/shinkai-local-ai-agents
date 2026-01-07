import {
  type WidgetToolType,
  type WsMessage,
} from '@shinkai_network/shinkai-message-ts/api/general/types';
import { extractJobIdFromInbox } from '@shinkai_network/shinkai-message-ts/utils/inbox_name_handler';
import {
  FunctionKeyV2,
  generateOptimisticAssistantMessage,
  OPTIMISTIC_ASSISTANT_MESSAGE_ID,
} from '@shinkai_network/shinkai-node-state/v2/constants';
import {
  type FormattedMessage,
  type ChatConversationInfiniteData,
  type ToolCall,
} from '@shinkai_network/shinkai-node-state/v2/queries/getChatConversation/types';
import { useGetProviderFromJob } from '@shinkai_network/shinkai-node-state/v2/queries/getProviderFromJob/useGetProviderFromJob';
import { useQueryClient } from '@tanstack/react-query';
import { produce } from 'immer';
import { useCallback, useEffect, useMemo, useRef } from 'react';
import { useParams } from 'react-router';
import useWebSocket from 'react-use-websocket';

import { useAuth } from '../../store/auth';
import { useToolsStore } from './context/tools-context';

// Token buffering configuration for smooth streaming
const FLUSH_INTERVAL_MS = 50; // 20 FPS - smooth enough for human perception

type UseWebSocketMessage = {
  enabled?: boolean;
  inboxId: string;
};

export const useWebSocketMessage = ({
  enabled,
  inboxId: defaultInboxId,
}: UseWebSocketMessage) => {
  const auth = useAuth((state) => state.auth);
  const nodeAddressUrl = new URL(auth?.node_address ?? 'http://localhost:9850');
  const socketUrl = ['localhost', '0.0.0.0', '127.0.0.1'].includes(
    nodeAddressUrl.hostname,
  )
    ? `ws://${nodeAddressUrl.hostname}:${Number(nodeAddressUrl.port) + 1}/ws`
    : `ws://${nodeAddressUrl.hostname}${Number(nodeAddressUrl.port) !== 0 ? `:${Number(nodeAddressUrl.port)}` : ''}/ws`;
  const queryClient = useQueryClient();
  const isStreamSupported = useRef(false);
  const hasStreamCompletedRef = useRef(false);

  // Token buffering refs for smooth streaming updates
  const tokenBufferRef = useRef('');
  const reasoningBufferRef = useRef('');
  const flushScheduledRef = useRef(false);

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

  const { data: provider } = useGetProviderFromJob({
    jobId: inboxId ? extractJobIdFromInbox(inboxId) : '',
    token: auth?.api_v2_key ?? '',
    nodeAddress: auth?.node_address ?? '',
  });

  // Flush buffered tokens to React Query state
  const flushTokenBuffer = useCallback(
    (options?: { finalMessageId?: string; markComplete?: boolean }) => {
      flushScheduledRef.current = false;
      const hasTokens = tokenBufferRef.current || reasoningBufferRef.current;
      const shouldFinalize = options?.markComplete;

      if (!hasTokens && !shouldFinalize) return;

      queryClient.setQueryData(
        queryKey,
        produce((draft: ChatConversationInfiniteData | undefined) => {
          if (!draft?.pages?.[0]) return;
          const lastMessage = draft.pages.at(-1)?.at(-1) as
            | FormattedMessage
            | undefined;
          if (
            lastMessage?.messageId === OPTIMISTIC_ASSISTANT_MESSAGE_ID &&
            lastMessage?.role === 'assistant' &&
            (lastMessage?.status?.type === 'running' || shouldFinalize)
          ) {
            // Apply reasoning buffer first
            if (reasoningBufferRef.current) {
              if (!lastMessage.reasoning) {
                lastMessage.reasoning = {
                  text: '',
                  status: { type: 'running' },
                };
              }
              lastMessage.reasoning.text += reasoningBufferRef.current;
              lastMessage.reasoning.status = { type: 'running' };
              reasoningBufferRef.current = '';
            }
            // Apply content buffer
            if (tokenBufferRef.current) {
              // Mark reasoning as complete when content starts
              if (lastMessage.reasoning) {
                lastMessage.reasoning.status = {
                  type: 'complete',
                  reason: 'unknown',
                };
              }
              lastMessage.content += tokenBufferRef.current;
              tokenBufferRef.current = '';
            }

            // Finalize the message if streaming is complete
            if (shouldFinalize) {
              // Update to the real message ID if provided
              if (options?.finalMessageId) {
                lastMessage.messageId = options.finalMessageId;
              }
              lastMessage.status = { type: 'complete', reason: 'unknown' };
              if (lastMessage.reasoning?.status?.type === 'running') {
                lastMessage.reasoning.status = {
                  type: 'complete',
                  reason: 'unknown',
                };
              }
            }
          }
        }),
      );
    },
    [queryClient, queryKey],
  );

  useEffect(() => {
    if (!enabled || !auth) return;
    if (lastMessage?.data) {
      try {
        const parseData: WsMessage = JSON.parse(lastMessage.data);
        if (parseData.inbox !== inboxId) return;

        const isUserMessage =
          parseData.message_type === 'ShinkaiMessage' &&
          parseData.message &&
          JSON.parse(parseData.message)?.external_metadata.sender ===
            auth.shinkai_identity &&
          JSON.parse(parseData.message)?.body.unencrypted.internal_metadata
            .sender_subidentity === auth.profile;

        const isAssistantMessage =
          parseData.message_type === 'ShinkaiMessage' &&
          parseData.message &&
          !(
            JSON.parse(parseData.message)?.external_metadata.sender ===
              auth.shinkai_identity &&
            JSON.parse(parseData.message)?.body.unencrypted.internal_metadata
              .sender_subidentity === auth.profile
          );

        if (isUserMessage) {
          // Reset streaming state for new message
          hasStreamCompletedRef.current = false;
          tokenBufferRef.current = '';
          reasoningBufferRef.current = '';

          queryClient.setQueryData(
            queryKey,
            produce((draft: ChatConversationInfiniteData) => {
              if (!draft?.pages?.[0]) return;

              const lastPage = draft.pages[draft.pages.length - 1];
              const lastMessage = lastPage?.[lastPage.length - 1];

              // validate if optimistic message is already there
              if (
                lastMessage &&
                lastMessage.messageId === OPTIMISTIC_ASSISTANT_MESSAGE_ID &&
                lastMessage.role === 'assistant' &&
                lastMessage.status?.type === 'running'
              ) {
                lastMessage.content = '';
              } else {
                const newMessages = [
                  generateOptimisticAssistantMessage(provider),
                ];
                if (lastPage) {
                  lastPage.push(...newMessages);
                } else {
                  draft.pages.push(newMessages);
                }
              }
            }),
          );
          return;
        }
        if (isAssistantMessage && !isStreamSupported.current) {
          void queryClient.invalidateQueries({ queryKey: queryKey });
          return;
        }
        isStreamSupported.current = false;

        // finalize the optimistic assistant message immediately when the final assistant message arrives
        if (isAssistantMessage) {
          // If streaming already completed via is_done, skip redundant processing
          if (hasStreamCompletedRef.current) {
            return;
          }

          // Flush any remaining buffered tokens before finalizing
          if (tokenBufferRef.current || reasoningBufferRef.current) {
            flushTokenBuffer({ markComplete: true });
          } else {
            // Only update status if no tokens to flush
            queryClient.setQueryData(
              queryKey,
              produce((draft: ChatConversationInfiniteData | undefined) => {
                if (!draft?.pages?.[0]) return;
                const lastMessage = draft.pages.at(-1)?.at(-1);
                if (
                  lastMessage &&
                  lastMessage.messageId === OPTIMISTIC_ASSISTANT_MESSAGE_ID &&
                  lastMessage.role === 'assistant' &&
                  lastMessage.status?.type === 'running'
                ) {
                  // Mark as complete so the UI stops "thinking"
                  lastMessage.status = { type: 'complete', reason: 'unknown' };
                  // Optional: also close reasoning if it's still marked running
                  if (lastMessage.reasoning?.status?.type === 'running') {
                    lastMessage.reasoning.status = {
                      type: 'complete',
                      reason: 'unknown',
                    };
                  }
                }
              }),
            );
          }

          // Mark streaming as completed to prevent duplicate processing
          hasStreamCompletedRef.current = true;
          // Don't refetch - the streamed content is already correct and visible
          // This prevents the UI flash that occurs when invalidateQueries replaces the cache
          return;
        }

        if (parseData.message_type !== 'Stream') return;
        isStreamSupported.current = true;

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
          // Don't refetch - the streamed content is already correct
          // The message will be properly synced on next conversation load or pagination
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
    auth?.shinkai_identity,
    auth?.profile,
    enabled,
    inboxId,
    lastMessage?.data,
    queryClient,
    queryKey,
    flushTokenBuffer,
  ]);

  useEffect(() => {
    if (!enabled) return;
    const wsMessage = {
      bearer_auth: auth?.api_v2_key ?? '',
      message: {
        subscriptions: [{ topic: 'inbox', subtopic: inboxId }],
        unsubscriptions: [],
      },
    };
    const wsMessageString = JSON.stringify(wsMessage);
    sendMessage(wsMessageString);
  }, [auth?.api_v2_key, auth?.shinkai_identity, enabled, inboxId, sendMessage]);

  return {
    readyState,
  };
};

export const useWebSocketTools = ({
  enabled,
  inboxId: defaultInboxId,
}: UseWebSocketMessage) => {
  const auth = useAuth((state) => state.auth);
  const nodeAddressUrl = new URL(auth?.node_address ?? 'http://localhost:9850');
  const socketUrl = ['localhost', '0.0.0.0', '127.0.0.1'].includes(
    nodeAddressUrl.hostname,
  )
    ? `ws://${nodeAddressUrl.hostname}:${Number(nodeAddressUrl.port) + 1}/ws`
    : `ws://${nodeAddressUrl.hostname}${Number(nodeAddressUrl.port) !== 0 ? `:${Number(nodeAddressUrl.port)}` : ''}/ws`;
  const { sendMessage, lastMessage, readyState } = useWebSocket(
    socketUrl,
    { share: true },
    enabled,
  );
  const { inboxId: encodedInboxId = '' } = useParams();
  const inboxId = defaultInboxId || decodeURIComponent(encodedInboxId);
  const queryClient = useQueryClient();

  const setWidget = useToolsStore((state) => state.setWidget);

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
          queryClient.setQueryData(
            queryKey,
            produce((draft: ChatConversationInfiniteData | undefined) => {
              if (!draft?.pages?.[0]) return;
              const lastMessage = draft.pages.at(-1)?.at(-1);
              if (
                lastMessage &&
                lastMessage.messageId === OPTIMISTIC_ASSISTANT_MESSAGE_ID &&
                lastMessage.role === 'assistant' &&
                lastMessage.status?.type === 'running'
              ) {
                const existingToolCall: ToolCall | undefined =
                  lastMessage.toolCalls?.[tool.index];

                if (existingToolCall) {
                  lastMessage.toolCalls[tool.index] = {
                    ...lastMessage.toolCalls[tool.index],
                    status: tool.status.type_,
                    result: tool.result?.data.message,
                  };
                } else {
                  lastMessage.toolCalls.push({
                    name: tool.tool_name,
                    // TODO: fix this based on backend
                    args: tool?.args ?? tool?.args?.arguments,
                    status: tool.status.type_,
                    toolRouterKey: tool?.tool_router_key ?? '',
                    result: tool.result?.data.message,
                  });
                }
              }
            }),
          );
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
  }, [enabled, inboxId, lastMessage?.data, queryClient]);

  useEffect(() => {
    if (!enabled) return;
    const wsMessage = {
      bearer_auth: auth?.api_v2_key ?? '',
      message: {
        subscriptions: [{ topic: 'widget', subtopic: inboxId }],
        unsubscriptions: [],
      },
    };
    const wsMessageString = JSON.stringify(wsMessage);
    sendMessage(wsMessageString);
  }, [auth?.api_v2_key, auth?.shinkai_identity, enabled, inboxId, sendMessage]);

  return { readyState };
};
