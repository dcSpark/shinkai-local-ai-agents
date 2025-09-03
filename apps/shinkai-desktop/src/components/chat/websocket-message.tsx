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
  type AssistantMessage,
  type ChatConversationInfiniteData,
  type ToolCall,
} from '@shinkai_network/shinkai-node-state/v2/queries/getChatConversation/types';
import { useGetProviderFromJob } from '@shinkai_network/shinkai-node-state/v2/queries/getProviderFromJob/useGetProviderFromJob';
import { useQueryClient } from '@tanstack/react-query';
import { produce } from 'immer';
import { createContext, useEffect, useMemo, useRef, useState } from 'react';
import { useParams } from 'react-router';
import useWebSocket from 'react-use-websocket';
import { create } from 'zustand';

import { useAuth } from '../../store/auth';
import { useToolsStore } from './context/tools-context';

type UseWebSocketMessage = {
  enabled?: boolean;
  inboxId: string;
};

const START_ANIMATION_SPEED = 4;
const END_ANIMATION_SPEED = 15;

const createSmoothMessage = (params: {
  onTextUpdate: (delta: string, text: string) => void;
  startSpeed?: number;
}) => {
  const { startSpeed = START_ANIMATION_SPEED } = params;

  let buffer = '';
  const outputQueue: string[] = [];
  let isAnimationActive = false;
  let animationFrameId: number | null = null;

  const stopAnimation = () => {
    isAnimationActive = false;
    if (animationFrameId !== null) {
      cancelAnimationFrame(animationFrameId);
      animationFrameId = null;
    }
  };

  const startAnimation = (speed = startSpeed) =>
    new Promise<void>((resolve) => {
      if (isAnimationActive) {
        resolve();
        return;
      }

      isAnimationActive = true;

      const updateText = () => {
        if (!isAnimationActive) {
          cancelAnimationFrame(animationFrameId as number);
          animationFrameId = null;
          resolve();
          return;
        }

        if (outputQueue.length > 0) {
          const charsToAdd = outputQueue.splice(0, speed).join('');
          buffer += charsToAdd;
          params.onTextUpdate(charsToAdd, buffer);
        } else {
          isAnimationActive = false;
          animationFrameId = null;
          resolve();
          return;
        }
        animationFrameId = requestAnimationFrame(updateText);
      };

      animationFrameId = requestAnimationFrame(updateText);
    });

  const pushToQueue = (text: string) => {
    outputQueue.push(...text.split(''));
  };

  const reset = () => {
    buffer = '';
    outputQueue.length = 0;
    stopAnimation();
  };

  return {
    isAnimationActive,
    isTokenRemain: () => outputQueue.length > 0,
    pushToQueue,
    startAnimation,
    stopAnimation,
    reset,
  };
};

/*
 Note: Adding smooth animation is way slower than the original implementation and
  websockets will still send the rest of text chunks when stop generating which will
  cause weird behavior in the UI.
 */

export const useWebSocketMessageSmooth = ({
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

  const isStreamingFinished = useRef(false);

  const smoothMessageRef = useRef(
    createSmoothMessage({
      onTextUpdate: (_, text) => {
        if (isStreamingFinished.current) return;
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
              lastMessage.content = text;
            }
          }),
        );
      },
    }),
  );

  useEffect(() => {
    if (!enabled || !auth) return;
    if (lastMessage?.data) {
      try {
        const parseData: WsMessage = JSON.parse(lastMessage.data);
        if (parseData.inbox !== inboxId) return;
        if (
          parseData.message_type === 'ShinkaiMessage' &&
          parseData.message &&
          JSON.parse(parseData.message)?.external_metadata.sender ===
            auth.shinkai_identity &&
          JSON.parse(parseData.message)?.body.unencrypted.internal_metadata
            .sender_subidentity === auth.profile
        ) {
          queryClient.setQueryData(
            queryKey,
            produce((draft: ChatConversationInfiniteData) => {
              const newMessages = [generateOptimisticAssistantMessage()];
              if (draft.pages.length > 0) {
                draft.pages[draft.pages.length - 1] =
                  draft.pages[draft.pages.length - 1].concat(newMessages);
              } else {
                draft.pages.push(newMessages);
              }
            }),
          );
          return;
        }

        if (
          parseData.message_type === 'ShinkaiMessage' &&
          parseData.message &&
          (JSON.parse(parseData.message)?.external_metadata.sender !==
            auth.shinkai_identity ||
            JSON.parse(parseData.message)?.body.unencrypted.internal_metadata
              .sender_subidentity !== auth.profile)
        ) {
          void queryClient.invalidateQueries({ queryKey: queryKey });
          return;
        }

        if (parseData.message_type !== 'Stream') return;
        isStreamingFinished.current = false;

        if (parseData.metadata?.is_done === true) {
          smoothMessageRef.current.stopAnimation();
          if (smoothMessageRef.current.isTokenRemain()) {
            void smoothMessageRef.current.startAnimation(END_ANIMATION_SPEED);
          }
          void queryClient.invalidateQueries({ queryKey: queryKey });
          isStreamingFinished.current = true;
          smoothMessageRef.current?.reset();
        }

        smoothMessageRef.current.pushToQueue(parseData.message);

        if (!smoothMessageRef.current.isAnimationActive)
          void smoothMessageRef.current.startAnimation();
      } catch (error) {
        console.error('Failed to parse ws message', error);
      }
    }
  }, [enabled, inboxId, lastMessage?.data, queryClient, queryKey]);

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
        if (parseData.message_type !== 'Stream') return;
        isStreamSupported.current = true;

        queryClient.setQueryData(
          queryKey,
          produce((draft: ChatConversationInfiniteData | undefined) => {
            if (!draft?.pages?.[0]) return;
            const lastMessage: FormattedMessage | undefined = draft.pages.at(-1)?.at(-1);
            if (
              lastMessage &&
              lastMessage.messageId === OPTIMISTIC_ASSISTANT_MESSAGE_ID &&
              lastMessage.role === 'assistant' &&
              lastMessage.status?.type === 'running'
            ) {
              if (parseData.metadata?.is_reasoning) {
                if (!lastMessage.reasoning) {
                  lastMessage.reasoning = {
                    text: '',
                    status: { type: 'running' },
                  };
                }
                lastMessage.reasoning.text += parseData.message;
                lastMessage.reasoning.status = { type: 'running' };
              } else {
                if (lastMessage.reasoning) {
                  lastMessage.reasoning.status = { type: 'complete', reason: 'unknown' };
                }
                lastMessage.content += parseData.message;
              }
            }
          }),
        );

        if (parseData.metadata?.is_done === true) {
          void queryClient.invalidateQueries({ queryKey: queryKey });
          return;
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
  const isToolReceived = useRef(false);

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
          parseData.message_type === 'ShinkaiMessage' &&
          isToolReceived.current
        ) {
          isToolReceived.current = false;
          const paginationKey = [
            FunctionKeyV2.GET_CHAT_CONVERSATION_PAGINATION,
            { inboxId: inboxId as string },
          ];
          void queryClient.invalidateQueries({ queryKey: paginationKey });
          return;
        }
        if (
          parseData.message_type === 'Widget' &&
          parseData?.widget?.ToolRequest
        ) {
          isToolReceived.current = true;
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

type ContentPartState = {
  type: 'text';
  text: string;
  part: {
    type: 'text';
    text: string;
  };
  status: {
    type: 'complete' | 'running';
  };
};

export const ContentPartContext = createContext({});
const COMPLETE_STATUS = {
  type: 'complete' as const,
};

const RUNNING_STATUS = {
  type: 'running' as const,
};

export const TextContentPartProvider = ({
  isRunning,
  text,
  children,
}: {
  text: string;
  isRunning?: boolean | undefined;
  children: React.ReactNode;
}) => {
  const [store] = useState(() => {
    return create<ContentPartState>(() => ({
      status: isRunning ? RUNNING_STATUS : COMPLETE_STATUS,
      part: { type: 'text', text },
      type: 'text',
      text: '',
    }));
  });

  useEffect(() => {
    const state = store.getState() as ContentPartState & {
      type: 'text';
    };

    const textUpdated = state.text !== text;
    const targetStatus = isRunning ? RUNNING_STATUS : COMPLETE_STATUS;
    const statusUpdated = state.status !== targetStatus;

    if (!textUpdated && !statusUpdated) return;

    store.setState(
      {
        type: 'text',
        text,
        part: { type: 'text', text },
        status: targetStatus,
      } satisfies ContentPartState,
      true,
    );
  }, [store, isRunning, text]);

  return (
    <ContentPartContext.Provider value={store}>
      {children}
    </ContentPartContext.Provider>
  );
};
