import { isJobInbox } from '@shinkai_network/shinkai-message-ts/utils';
import { useInfiniteQuery } from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import {
  type ChatConversationInfiniteData,
  type GetChatConversationInput,
  type GetChatConversationOutput,
} from './types';
import { getChatConversation } from '.';

export const CONVERSATION_PAGINATION_LIMIT = 20;
export const CONVERSATION_PAGINATION_REFETCH = 5000;

export const useGetChatConversationWithPagination = (
  input: GetChatConversationInput,
) => {
  const response = useInfiniteQuery<
    GetChatConversationOutput,
    APIError,
    ChatConversationInfiniteData,
    [string, { inboxId: string }],
    { lastKey: string | null }
  >({
    queryKey: [
      FunctionKeyV2.GET_CHAT_CONVERSATION_PAGINATION,
      {
        inboxId: input.inboxId,
      },
    ],
    queryFn: ({ pageParam }) =>
      getChatConversation({
        ...input,
        lastKey: pageParam?.lastKey ?? undefined,
        count: CONVERSATION_PAGINATION_LIMIT,
      }),
    getPreviousPageParam: (firstPage, pages) => {
      if (firstPage?.length < CONVERSATION_PAGINATION_LIMIT) return;
      const firstMessage = pages?.[0]?.[0];
      if (!firstMessage) return;
      return { lastKey: firstMessage.messageId };
    },
    refetchInterval: ({ state }) => {
      if (!input?.refetchIntervalEnabled) return 0;
      const lastMessage = state.data?.pages?.at(-1)?.at(-1);
      if (!lastMessage) return 0;
      if (
        isJobInbox(input.inboxId) &&
        lastMessage.role === 'assistant' &&
        lastMessage.status.type === 'running' &&
        lastMessage.content === ''
      )
        return CONVERSATION_PAGINATION_REFETCH;

      return 0;
    },
    initialPageParam: { lastKey: null },
    getNextPageParam: () => {
      return { lastKey: null };
    },
    select: (data) => {
      const allMessages = data.pages.flat();
      return {
        ...data,
        content: allMessages,
      };
    },
    enabled: input.enabled,
    refetchOnWindowFocus: false,
    // Cache data for 30 seconds so switching back to visited chats is instant
    // This prevents showing stale placeholder data from a different chat
    staleTime: 30_000,
  });
  return response;
};
