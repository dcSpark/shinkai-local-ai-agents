import { OPTIMISTIC_ASSISTANT_MESSAGE_ID } from '@shinkai_network/shinkai-node-state/v2/constants';
import { type ChatConversationInfiniteData } from '@shinkai_network/shinkai-node-state/v2/queries/getChatConversation/types';
import { Button, Skeleton } from '@shinkai_network/shinkai-ui';
import { getRelativeDateLabel } from '@shinkai_network/shinkai-ui/helpers';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import {
  type FetchPreviousPageOptions,
  type InfiniteQueryObserverResult,
} from '@tanstack/react-query';
import { ArrowDownIcon } from 'lucide-react';
import React, {
  Fragment,
  memo,
  type RefObject,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import { useInView } from 'react-intersection-observer';

import { Message } from './message';
import { StreamingMessage } from './streaming-message';

const SCROLL_THRESHOLD = 50;

// Skeleton components for better loading UI
const MessageSkeleton = ({
  isUser,
  index,
  lines = 3,
}: {
  isUser: boolean;
  index: number;
  lines?: number;
}) => {
  const animationDelay = `${index * 100}ms`;

  if (isUser) {
    return (
      <div
        className="container flex justify-end px-3.5 pt-4 pb-4"
        style={{ animationDelay }}
      >
        <div className="flex max-w-[70%] flex-col items-end gap-2">
          <Skeleton
            className="h-10 w-48 rounded-lg sm:w-64"
            style={{ animationDelay }}
          />
        </div>
      </div>
    );
  }

  return (
    <div className="container px-3.5 pb-10" style={{ animationDelay }}>
      <div className="flex flex-col gap-2">
        {/* Avatar and name */}
        <div className="mt-2 flex items-center gap-2">
          <Skeleton
            className="size-5 rounded-full"
            style={{ animationDelay }}
          />
          <Skeleton
            className="h-4 w-24 rounded-md"
            style={{ animationDelay }}
          />
        </div>
        {/* Content bubble */}
        <div className="bg-transparent pt-1">
          <div className="flex flex-col gap-2">
            {[...Array(lines)].map((_, lineIndex) => (
              <Skeleton
                key={lineIndex}
                className={cn(
                  'h-4 rounded-md',
                  lineIndex === 0 && 'w-[90%]',
                  lineIndex === 1 && 'w-[75%]',
                  lineIndex === 2 && 'w-[60%]',
                  lineIndex > 2 && 'w-[45%]',
                )}
                style={{ animationDelay: `${index * 100 + lineIndex * 50}ms` }}
              />
            ))}
          </div>
        </div>
      </div>
    </div>
  );
};

const LoadingMoreSkeleton = () => (
  <div className="flex flex-col gap-2 px-3.5 py-4">
    <div className="flex items-center justify-center gap-2">
      <div className="bg-brand/20 size-2 animate-bounce rounded-full [animation-delay:0ms]" />
      <div className="bg-brand/20 size-2 animate-bounce rounded-full [animation-delay:150ms]" />
      <div className="bg-brand/20 size-2 animate-bounce rounded-full [animation-delay:300ms]" />
    </div>
  </div>
);

function useScrollToBottom(scrollRef: RefObject<HTMLDivElement | null>) {
  const [isAtBottom, setIsAtBottom] = useState(true);
  const [autoScroll, setAutoScroll] = useState(true);
  const autoScrollRef = useRef(autoScroll);
  autoScrollRef.current = autoScroll;

  const checkIsAtBottom = useCallback(() => {
    const container = scrollRef.current;
    if (!container) return true;
    const { scrollTop, scrollHeight, clientHeight } = container;
    return scrollTop + clientHeight >= scrollHeight - SCROLL_THRESHOLD;
  }, [scrollRef]);

  const scrollDomToBottom = useCallback(() => {
    const container = scrollRef.current;
    if (!container) return;
    setAutoScroll(true);
    setIsAtBottom(true);
    container.scrollTo({ top: container.scrollHeight, behavior: 'instant' });
  }, [scrollRef]);

  const scrollToBottom = useCallback(() => {
    const container = scrollRef.current;
    if (!container) return;
    setAutoScroll(true);
    setIsAtBottom(true);
    container.scrollTo({ top: container.scrollHeight, behavior: 'smooth' });
  }, [scrollRef]);

  useEffect(() => {
    const container = scrollRef.current;
    if (!container) return;

    const handleScroll = () => {
      const nearBottom = checkIsAtBottom();
      setIsAtBottom(nearBottom);
      setAutoScroll(nearBottom);
    };

    container.addEventListener('scroll', handleScroll, { passive: true });
    return () => container.removeEventListener('scroll', handleScroll);
  }, [scrollRef, checkIsAtBottom]);

  useEffect(() => {
    const container = scrollRef.current;
    if (!container) return;

    const handleContentChange = () => {
      if (autoScrollRef.current) {
        container.scrollTo({
          top: container.scrollHeight,
          behavior: 'instant',
        });
      }
    };

    const observer = new MutationObserver(handleContentChange);
    observer.observe(container, {
      childList: true,
      subtree: true,
      characterData: true,
    });

    return () => observer.disconnect();
  }, [scrollRef]);

  return {
    isAtBottom,
    autoScroll,
    setAutoScroll,
    scrollDomToBottom,
    scrollToBottom,
  };
}

export const MessageList = memo(
  ({
    noMoreMessageLabel,
    paginatedMessages,
    isSuccess,
    isLoading,
    isFetchingPreviousPage,
    hasPreviousPage,
    fetchPreviousPage,
    containerClassName,
    lastMessageContent,
    editAndRegenerateMessage,
    regenerateMessage,
    forkMessage,
    disabledRetryAndEdit,
    messageExtra,
    hidePythonExecution,
    minimalistMode,
    inboxId,
  }: {
    noMoreMessageLabel: string;
    isSuccess: boolean;
    isLoading: boolean;
    isFetchingPreviousPage: boolean;
    hasPreviousPage: boolean;
    paginatedMessages: ChatConversationInfiniteData | undefined;
    fetchPreviousPage: (
      options?: FetchPreviousPageOptions | undefined,
    ) => Promise<
      InfiniteQueryObserverResult<ChatConversationInfiniteData, Error>
    >;
    regenerateMessage?: (messageId: string) => void;
    forkMessage?: (messageId: string) => void;
    editAndRegenerateMessage?: (content: string, messageHash: string) => void;
    containerClassName?: string;
    lastMessageContent?: React.ReactNode;
    disabledRetryAndEdit?: boolean;
    messageExtra?: React.ReactNode;
    hidePythonExecution?: boolean;
    minimalistMode?: boolean;
    inboxId?: string;
  }) => {
    const chatContainerRef = useRef<HTMLDivElement>(null);
    // Store both scrollTop and scrollHeight for accurate restoration after prepending
    const scrollRestoreRef = useRef<{ top: number; height: number } | null>(
      null,
    );
    const hasInitiallyScrolledRef = useRef(false);
    const prevLastMessageIdRef = useRef<string | null>(null);
    const { ref, inView } = useInView();
    const messageList = useMemo(
      () => paginatedMessages?.pages.flat() ?? [],
      [paginatedMessages?.pages],
    );

    const {
      isAtBottom,
      autoScroll,
      setAutoScroll,
      scrollDomToBottom,
      scrollToBottom,
    } = useScrollToBottom(chatContainerRef);

    // Reset scroll state when switching to a different chat
    const pagesKey = paginatedMessages?.pages?.[0]?.[0]?.messageId;
    useEffect(() => {
      hasInitiallyScrolledRef.current = false;
      scrollRestoreRef.current = null;
      prevLastMessageIdRef.current = null;
    }, [pagesKey]);

    const fetchPreviousMessages = useCallback(async () => {
      const container = chatContainerRef.current;
      if (!container) return;

      // Store current scroll position before fetching
      scrollRestoreRef.current = {
        top: container.scrollTop,
        height: container.scrollHeight,
      };
      setAutoScroll(false);
      await fetchPreviousPage();
    }, [fetchPreviousPage, setAutoScroll]);

    // Trigger fetch when load more sentinel is in view
    useEffect(() => {
      if (hasPreviousPage && inView && !isFetchingPreviousPage) {
        void fetchPreviousMessages();
      }
    }, [
      hasPreviousPage,
      inView,
      isFetchingPreviousPage,
      fetchPreviousMessages,
    ]);

    // Restore scroll position after prepending older messages
    useLayoutEffect(() => {
      const container = chatContainerRef.current;
      const restore = scrollRestoreRef.current;

      // Only restore if we have stored position and fetch completed
      if (!container || !restore || isFetchingPreviousPage) return;

      const newHeight = container.scrollHeight;
      const heightDiff = newHeight - restore.height;

      if (heightDiff > 0) {
        container.scrollTop = restore.top + heightDiff;
      }

      scrollRestoreRef.current = null;
    }, [messageList.length, isFetchingPreviousPage]);

    // Detect new messages appended at the bottom and scroll appropriately
    useEffect(() => {
      const lastMessage = messageList.at(-1);
      if (!lastMessage) return;

      const prevLastId = prevLastMessageIdRef.current;
      prevLastMessageIdRef.current = lastMessage.messageId;

      // Skip if this is the initial load or same message
      if (!prevLastId || prevLastId === lastMessage.messageId) return;

      // Detect if this is a local send (user message or optimistic assistant)
      const isLocalSend =
        lastMessage.role === 'user' ||
        lastMessage.messageId === OPTIMISTIC_ASSISTANT_MESSAGE_ID;

      if (isLocalSend) {
        // Force scroll on send, even if user scrolled up
        scrollDomToBottom();
      } else if (autoScroll) {
        // For other messages (e.g., assistant response), only scroll if already at bottom
        scrollDomToBottom();
      }
    }, [messageList, autoScroll, scrollDomToBottom]);

    // Initial scroll to bottom when chat loads
    useEffect(() => {
      if (isSuccess && !hasInitiallyScrolledRef.current) {
        hasInitiallyScrolledRef.current = true;
        requestAnimationFrame(() => {
          scrollDomToBottom();
        });
      }
    }, [isSuccess, scrollDomToBottom]);

    return (
      <div className="relative h-full flex-1">
        <div
          className={cn(
            'scroll size-full overflow-y-auto overscroll-none will-change-scroll',
            containerClassName,
          )}
          ref={chatContainerRef}
          style={{ contain: 'strict' }}
        >
          {isSuccess &&
            !isFetchingPreviousPage &&
            !hasPreviousPage &&
            (paginatedMessages?.pages ?? [])?.length > 1 && (
              <div className="text-text-secondary py-2 text-center text-xs">
                {noMoreMessageLabel}
              </div>
            )}
          <div className="">
            {isLoading && (
              <div className="flex flex-col">
                {/* Simulate a conversation pattern: user, assistant, user, assistant... */}
                {[...Array(6).keys()].map((index) => (
                  <MessageSkeleton
                    key={`skeleton-${index}`}
                    index={index}
                    isUser={index % 2 === 0}
                    lines={index % 2 === 0 ? 1 : [2, 4, 3][index % 3]}
                  />
                ))}
              </div>
            )}
            {(hasPreviousPage || isFetchingPreviousPage) && (
              <div ref={ref}>
                <LoadingMoreSkeleton />
              </div>
            )}
            {isSuccess && messageList?.length > 0 && (
              <Fragment>
                <div className="flex flex-col">
                  {messageList.map((message, index) => {
                    // Get previous message from the flat list (correct across date boundaries)
                    const previousMessage = messageList[index - 1];

                    // Determine if we need a date separator
                    const currentDateKey = message.createdAt
                      ? new Date(message.createdAt).toDateString()
                      : '';
                    const previousDateKey = previousMessage?.createdAt
                      ? new Date(previousMessage.createdAt).toDateString()
                      : '';
                    const showDateSeparator =
                      !minimalistMode && currentDateKey !== previousDateKey;

                    // Disable edit/retry for the first message in the entire list
                    const disabledRetryAndEditValue =
                      disabledRetryAndEdit ?? index === 0;

                    const handleRetryMessage = () => {
                      regenerateMessage?.(message?.messageId ?? '');
                    };

                    const handleForkMessage = () => {
                      forkMessage?.(message?.messageId ?? '');
                    };

                    const handleEditMessage = (content: string) => {
                      editAndRegenerateMessage?.(
                        content,
                        previousMessage?.messageId ?? '',
                      );
                    };

                    // Use StreamingMessage for the optimistic assistant message
                    // to enable efficient streaming updates without re-rendering the whole list
                    const isOptimisticMessage =
                      message.messageId === OPTIMISTIC_ASSISTANT_MESSAGE_ID &&
                      message.role === 'assistant';

                    const MessageComponent = isOptimisticMessage
                      ? StreamingMessage
                      : Message;

                    const isLastMessage = index === messageList.length - 1;

                    return (
                      <Fragment key={message.messageId}>
                        {showDateSeparator && (
                          <div className="bg-bg-tertiary relative z-10 m-auto my-2 flex h-[26px] w-fit min-w-[100px] items-center justify-center rounded-xl px-2.5 capitalize">
                            <span className="text-text-tertiary text-sm font-medium">
                              {getRelativeDateLabel(
                                new Date(message.createdAt || ''),
                              )}
                            </span>
                          </div>
                        )}
                        <MessageComponent
                          disabledEdit={disabledRetryAndEditValue}
                          handleEditMessage={handleEditMessage}
                          handleForkMessage={handleForkMessage}
                          handleRetryMessage={handleRetryMessage}
                          hidePythonExecution={hidePythonExecution}
                          inboxId={inboxId || ''}
                          isLastMessage={isLastMessage}
                          message={message}
                          messageId={message.messageId}
                          minimalistMode={minimalistMode}
                        />
                      </Fragment>
                    );
                  })}
                </div>
                {messageExtra}
                {lastMessageContent}
              </Fragment>
            )}
          </div>
        </div>
        {!isAtBottom && (
          <Button
            aria-label="Scroll to bottom"
            className="absolute bottom-4 left-1/2 z-10 -translate-x-1/2 rounded-full border p-2 shadow-lg transition-colors"
            onClick={scrollToBottom}
            size="icon"
            type="button"
            variant="outline"
          >
            <ArrowDownIcon className="size-4" />
          </Button>
        )}
      </div>
    );
  },
);
MessageList.displayName = 'MessageList';
