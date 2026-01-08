import { useTranslation } from '@shinkai_network/shinkai-i18n';
import {
  buildInboxIdFromJobId,
  extractJobIdFromInbox,
} from '@shinkai_network/shinkai-message-ts/utils';
import {
  FunctionKeyV2,
  generateOptimisticAssistantMessage,
} from '@shinkai_network/shinkai-node-state/v2/constants';
import { useForkJobMessages } from '@shinkai_network/shinkai-node-state/v2/mutations/forkJobMessages/useForkJobMessages';
import { useRetryMessage } from '@shinkai_network/shinkai-node-state/v2/mutations/retryMessage/useRetryMessage';
import { useSendMessageToJob } from '@shinkai_network/shinkai-node-state/v2/mutations/sendMessageToJob/useSendMessageToJob';
import { useGetChatConfig } from '@shinkai_network/shinkai-node-state/v2/queries/getChatConfig/useGetChatConfig';
import { type ChatConversationInfiniteData } from '@shinkai_network/shinkai-node-state/v2/queries/getChatConversation/types';
import { useGetChatConversationWithPagination } from '@shinkai_network/shinkai-node-state/v2/queries/getChatConversation/useGetChatConversationWithPagination';
import { useGetProviderFromJob } from '@shinkai_network/shinkai-node-state/v2/queries/getProviderFromJob/useGetProviderFromJob';
import { useQueryClient } from '@tanstack/react-query';
import { useEffect, useMemo } from 'react';
import { useNavigate, useParams } from 'react-router';
import { toast } from 'sonner';

import { MessageList } from '../../components/chat/components/message-list';
import { useChatStore } from '../../components/chat/context/chat-context';
import ConversationChatFooter from '../../components/chat/conversation-footer';
import ConversationHeader from '../../components/chat/conversation-header';
import MessageExtra from '../../components/chat/message-extra';
import {
  useWebSocketMessage,
  useWebSocketTools,
} from '../../components/chat/websocket-message';
import { useAnalytics } from '../../lib/posthog-provider';
import { useAuth } from '../../store/auth';
import { useSettings } from '../../store/settings';
import EmptyMessage from './empty-message';

export const useChatConversationWithOptimisticUpdates = ({
  inboxId,
  forceRefetchInterval,
}: {
  inboxId: string;
  forceRefetchInterval?: boolean;
}) => {
  const queryClient = useQueryClient();
  const auth = useAuth((state) => state.auth);

  const { data: chatConfig } = useGetChatConfig(
    {
      nodeAddress: auth?.node_address ?? '',
      token: auth?.api_v2_key ?? '',
      jobId: inboxId ? extractJobIdFromInbox(inboxId) : '',
    },
    {
      enabled: !!inboxId,
    },
  );

  const { data: provider } = useGetProviderFromJob({
    jobId: inboxId ? extractJobIdFromInbox(inboxId) : '',
    token: auth?.api_v2_key ?? '',
    nodeAddress: auth?.node_address ?? '',
  });

  const {
    data,
    fetchPreviousPage,
    hasPreviousPage,
    isPending: isChatConversationLoading,
    isFetchingPreviousPage,
    isSuccess: isChatConversationSuccess,
    isError,
    error: chatConversationError,
  } = useGetChatConversationWithPagination({
    token: auth?.api_v2_key ?? '',
    nodeAddress: auth?.node_address ?? '',
    inboxId: inboxId as string,
    shinkaiIdentity: auth?.shinkai_identity ?? '',
    profile: auth?.profile ?? '',
    refetchIntervalEnabled: chatConfig?.stream === false,
    enabled: !!inboxId,
  });

  useEffect(() => {
    if (
      isChatConversationSuccess &&
      data.content.length > 0 &&
      data.content.length % 2 === 1 &&
      data.content.at(-1)?.role === 'user'
    ) {
      const queryKey = [
        FunctionKeyV2.GET_CHAT_CONVERSATION_PAGINATION,
        { inboxId },
      ];

      queryClient.setQueryData(
        queryKey,
        (old: ChatConversationInfiniteData | undefined) => {
          if (!old?.pages?.length) return old;

          const pages = old.pages.slice();
          const lastPageIdx = pages.length - 1;
          const lastPage = [...pages[lastPageIdx]];

          const lastMsg = lastPage.at(-1);
          if (lastMsg?.role === 'assistant') return old;

          lastPage.push(generateOptimisticAssistantMessage(provider));
          pages[lastPageIdx] = lastPage;

          return { ...old, pages };
        },
      );
    }
  }, [
    data?.content.length,
    data?.content,
    inboxId,
    isChatConversationSuccess,
    provider,
    queryClient,
  ]);

  useEffect(() => {
    if (forceRefetchInterval) {
      setTimeout(() => {
        void queryClient.invalidateQueries({
          queryKey: [
            FunctionKeyV2.GET_CHAT_CONVERSATION_PAGINATION,
            { inboxId },
          ],
        });
      }, 6000);
    }
  }, [forceRefetchInterval, inboxId, queryClient]);

  return {
    data,
    fetchPreviousPage,
    hasPreviousPage,
    isChatConversationLoading,
    isFetchingPreviousPage,
    isChatConversationSuccess,
    isError,
    chatConversationError,
    provider,
  };
};

const ChatConversation = () => {
  const { captureAnalyticEvent } = useAnalytics();
  const { t } = useTranslation();
  const navigate = useNavigate();

  const queryClient = useQueryClient();

  const { inboxId: encodedInboxId = '' } = useParams();
  const inboxId = decodeURIComponent(encodedInboxId);

  useWebSocketMessage({ inboxId, enabled: !!inboxId });
  useWebSocketTools({ inboxId, enabled: !!inboxId });

  const setSelectedArtifact = useChatStore(
    (state) => state.setSelectedArtifact,
  );

  const auth = useAuth((state) => state.auth);
  const optInExperimental = useSettings((state) => state.optInExperimental);

  const {
    data,
    fetchPreviousPage,
    hasPreviousPage,
    isChatConversationLoading,
    isFetchingPreviousPage,
    isChatConversationSuccess,
    isError,
    chatConversationError,
    provider,
  } = useChatConversationWithOptimisticUpdates({
    inboxId,
  });

  const isLoadingMessage = useMemo(() => {
    const lastMessage = data?.pages?.at(-1)?.at(-1);
    return (
      !!inboxId &&
      lastMessage &&
      lastMessage.role === 'assistant' &&
      lastMessage.status.type === 'running'
    );
  }, [data?.pages, inboxId]);

  useEffect(() => {
    if (isError) {
      toast.error('Failed loading chat conversation', {
        description:
          chatConversationError?.response?.data?.message ??
          chatConversationError?.message,
      });
    }
  }, [chatConversationError, isError]);

  useEffect(() => {
    const lastMessage = data?.pages?.at(-1)?.at(-1);
    if (
      optInExperimental &&
      lastMessage?.role === 'assistant' &&
      lastMessage.artifacts?.length > 0
    ) {
      setSelectedArtifact(lastMessage.artifacts?.at(-1) ?? null);
    }
  }, [data?.pages, optInExperimental]);

  const { mutateAsync: retryMessage } = useRetryMessage();

  const { mutateAsync: forkMessage } = useForkJobMessages({
    onSuccess: async (response) => {
      await navigate(
        `/inboxes/${encodeURIComponent(
          buildInboxIdFromJobId(response.job_id),
        )}`,
      );
    },
    onError: (error) => {
      toast.error('Failed to fork message', {
        description: error.message,
      });
    },
  });

  const { mutateAsync: sendMessageToJob } = useSendMessageToJob({
    onSuccess: () => {
      captureAnalyticEvent('Edit and Regenerate Message', undefined);
      void queryClient.invalidateQueries({
        queryKey: [FunctionKeyV2.GET_CHAT_CONVERSATION_PAGINATION, { inboxId }],
      });
    },
  });

  const regenerateMessage = async (messageId: string) => {
    if (!auth) return;
    const decodedInboxId = decodeURIComponent(inboxId);

    await retryMessage({
      nodeAddress: auth.node_address,
      token: auth.api_v2_key,
      inboxId: decodedInboxId,
      messageId: messageId,
    });
  };

  const handleForkMessage = async (messageId: string) => {
    if (!auth) return;
    const decodedInboxId = decodeURIComponent(inboxId);
    const jobId = extractJobIdFromInbox(decodedInboxId);

    await forkMessage({
      nodeAddress: auth.node_address,
      token: auth.api_v2_key,
      jobId: jobId,
      messageId: messageId,
    });
  };

  const editAndRegenerateMessage = async (
    content: string,
    parentHash: string,
  ) => {
    if (!auth) return;
    const decodedInboxId = decodeURIComponent(inboxId);
    const jobId = extractJobIdFromInbox(decodedInboxId);

    await sendMessageToJob({
      nodeAddress: auth.node_address,
      token: auth.api_v2_key,
      jobId,
      message: content,
      parent: parentHash,
      provider,
    });
  };

  return (
    <div className="flex max-h-screen flex-1 flex-col overflow-hidden pt-2">
      <ConversationHeader />
      {inboxId ? (
        <MessageList
          containerClassName="px-5"
          editAndRegenerateMessage={editAndRegenerateMessage}
          fetchPreviousPage={fetchPreviousPage}
          forkMessage={handleForkMessage}
          hasPreviousPage={hasPreviousPage}
          isFetchingPreviousPage={isFetchingPreviousPage}
          isLoading={isChatConversationLoading}
          isSuccess={isChatConversationSuccess}
          messageExtra={<MessageExtra />}
          noMoreMessageLabel={t('chat.allMessagesLoaded')}
          paginatedMessages={data}
          regenerateMessage={regenerateMessage}
        />
      ) : (
        <EmptyMessage />
      )}
      <ConversationChatFooter
        inboxId={inboxId}
        isLoadingMessage={!!isLoadingMessage}
      />
    </div>
  );
};

export default ChatConversation;
