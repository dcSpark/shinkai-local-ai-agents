import { OPTIMISTIC_ASSISTANT_MESSAGE_ID } from '@shinkai_network/shinkai-node-state/v2/constants';
import {
  type AssistantMessage,
  type FormattedMessage,
} from '@shinkai_network/shinkai-node-state/v2/queries/getChatConversation/types';
import { memo, useMemo } from 'react';
import { useParams } from 'react-router';

import { useStreamingContent } from '../context/streaming-context';
import { Message } from './message';

type StreamingMessageProps = {
  message: FormattedMessage;
  messageId: string;
  inboxId: string;
  handleRetryMessage?: () => void;
  handleForkMessage?: () => void;
  disabledRetry?: boolean;
  disabledEdit?: boolean;
  handleEditMessage?: (message: string) => void;
  hidePythonExecution?: boolean;
  minimalistMode?: boolean;
  isLastMessage?: boolean;
};

/**
 * Wrapper component that provides streaming content for the optimistic assistant message.
 * This allows only the currently streaming message to re-render during streaming,
 * rather than the entire message list.
 */
export const StreamingMessage = memo(function StreamingMessage({
  message,
  messageId,
  inboxId: inboxIdProp,
  handleRetryMessage,
  handleForkMessage,
  disabledRetry,
  disabledEdit,
  handleEditMessage,
  hidePythonExecution,
  minimalistMode,
  isLastMessage,
}: StreamingMessageProps) {
  // Try to get inboxId from prop first, fallback to useParams for backwards compatibility
  const { inboxId: encodedInboxId = '' } = useParams();
  const inboxIdFromParams = encodedInboxId
    ? decodeURIComponent(encodedInboxId)
    : '';
  const inboxId = inboxIdProp || inboxIdFromParams;

  // Only subscribe to streaming content for the optimistic message
  const isOptimisticMessage =
    messageId === OPTIMISTIC_ASSISTANT_MESSAGE_ID &&
    message.role === 'assistant';

  const streamingContent = useStreamingContent(
    isOptimisticMessage ? inboxId : '',
  );

  // Merge streaming content with the message for the optimistic assistant message
  const mergedMessage = useMemo(() => {
    if (!isOptimisticMessage) {
      return message;
    }

    const assistantMsg = message as AssistantMessage;

    // Check if we have streaming content (either active or just ended)
    // When streaming ends, the store keeps content with isStreaming=false
    // This bridges the gap until React Query updates with final data
    const hasStreamingContent =
      streamingContent &&
      (streamingContent.content ||
        streamingContent.reasoning?.text ||
        streamingContent.toolCalls.length > 0);

    if (hasStreamingContent) {
      return {
        ...assistantMsg,
        content: streamingContent.content || assistantMsg.content,
        reasoning: streamingContent.reasoning ?? assistantMsg.reasoning,
        toolCalls:
          streamingContent.toolCalls.length > 0
            ? streamingContent.toolCalls
            : assistantMsg.toolCalls,
      } as FormattedMessage;
    }

    // No streaming content available - use the message prop directly
    // This happens either before streaming starts or after React Query
    // has been updated with the final data
    return message;
  }, [isOptimisticMessage, message, streamingContent]);

  return (
    <Message
      disabledEdit={disabledEdit}
      disabledRetry={disabledRetry}
      handleEditMessage={handleEditMessage}
      handleForkMessage={handleForkMessage}
      handleRetryMessage={handleRetryMessage}
      hidePythonExecution={hidePythonExecution}
      isLastMessage={isLastMessage}
      key={`${messageId}::streaming`}
      message={mergedMessage}
      messageId={messageId}
      minimalistMode={minimalistMode}
    />
  );
});
