import { type ChatConversationInfiniteData } from '@shinkai_network/shinkai-node-state/v2/queries/getChatConversation/types';
import { Skeleton } from '@shinkai_network/shinkai-ui';
import { groupMessagesByDate } from '@shinkai_network/shinkai-ui/helpers';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import {
  type FetchPreviousPageOptions,
  type InfiniteQueryObserverResult,
} from '@tanstack/react-query';
import React, {
  Fragment,
  memo,
  type RefObject,
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from 'react';
import { useInView } from 'react-intersection-observer';

import { Message } from './message';

function useScrollToBottom(
  scrollRef: RefObject<HTMLDivElement | null>,
  detach = false,
) {
  const [autoScroll, setAutoScroll] = useState(true);
  function scrollDomToBottom() {
    const scrollContainer = scrollRef.current;
    if (scrollContainer) {
      requestAnimationFrame(() => {
        setAutoScroll(true);
        scrollContainer.scrollTo(0, scrollContainer.scrollHeight);
      });
    }
  }

  useEffect(() => {
    if (autoScroll && !detach) {
      scrollDomToBottom();
    }
  });

  return {
    scrollRef,
    autoScroll,
    setAutoScroll,
    scrollDomToBottom,
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
    disabledRetryAndEdit,
    messageExtra,
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
    editAndRegenerateMessage?: (content: string, messageHash: string) => void;
    containerClassName?: string;
    lastMessageContent?: React.ReactNode;
    disabledRetryAndEdit?: boolean;
    messageExtra?: React.ReactNode;
  }) => {
    const chatContainerRef = useRef<HTMLDivElement>(null);
    const previousChatHeightRef = useRef<number>(0);
    const { ref, inView } = useInView();
    const messageList = paginatedMessages?.pages.flat() ?? [];

    const { autoScroll, setAutoScroll, scrollDomToBottom } =
      useScrollToBottom(chatContainerRef);

    const fetchPreviousMessages = useCallback(async () => {
      setAutoScroll(false);
      await fetchPreviousPage();
      // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [fetchPreviousPage]);

    useEffect(() => {
      if (hasPreviousPage && inView) {
        void fetchPreviousMessages();
      }
      // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [hasPreviousPage, inView]);

    // adjust the scroll position of a chat container after new messages are fetched
    useLayoutEffect(() => {
      if (!isFetchingPreviousPage && inView) {
        const chatContainerElement = chatContainerRef.current;
        if (!chatContainerElement) return;
        const currentHeight = chatContainerElement.scrollHeight;
        const previousHeight = previousChatHeightRef.current;

        if (!autoScroll) {
          chatContainerElement.scrollTop =
            currentHeight - previousHeight + chatContainerElement.scrollTop;
        } else {
          scrollDomToBottom();
        }

        chatContainerElement.scrollTop = currentHeight - previousHeight;
      }
      // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [paginatedMessages, isFetchingPreviousPage, inView]);

    useEffect(() => {
      const chatContainerElement = chatContainerRef.current;
      if (!chatContainerElement) return;
      const handleScroll = async () => {
        const currentHeight = chatContainerElement.scrollHeight;
        const currentScrollTop = chatContainerElement.scrollTop;
        previousChatHeightRef.current = currentHeight;
        const scrollThreshold = 20;
        const isNearBottom =
          currentScrollTop + chatContainerElement.clientHeight >=
          currentHeight - scrollThreshold;

        setAutoScroll(isNearBottom);

        if (inView && hasPreviousPage && !isFetchingPreviousPage) {
          previousChatHeightRef.current = currentHeight - currentScrollTop;
        }
      };

      chatContainerElement.addEventListener('scroll', handleScroll, {
        passive: true,
      });
      return () => {
        chatContainerElement.removeEventListener('scroll', handleScroll);
      };
      // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [
      fetchPreviousMessages,
      hasPreviousPage,
      inView,
      isFetchingPreviousPage,
      paginatedMessages?.pages?.length,
    ]);

    useEffect(() => {
      if (messageList?.length % 2 === 1) {
        scrollDomToBottom();
      }
      // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [messageList?.length]);

    useEffect(() => {
      if (isSuccess) {
        scrollDomToBottom();
      }
      // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [isSuccess]);

    return (
      <div
        className={cn(
          'scroll size-full overflow-y-auto overscroll-none will-change-scroll',
          'flex-1 overflow-y-auto',
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
            <div className="container flex flex-col space-y-8">
              {[...Array(10).keys()].map((index) => (
                <div
                  className={cn(
                    'flex w-[85%] gap-2',
                    index % 2 !== 0
                      ? 'mr-auto ml-0 flex-col'
                      : 'mr-0 ml-auto w-[300px] items-start',
                  )}
                  key={`skeleton-${index}`}
                >
                  {index % 2 !== 0 ? (
                    <div className="flex items-center justify-start gap-2">
                      <Skeleton
                        className="size-6 shrink-0 rounded-full"
                        key={`avatar-${index}`}
                      />
                      <Skeleton
                        className="h-6 w-[100px] shrink-0 rounded-md"
                        key={`name-${index}`}
                      />
                    </div>
                  ) : null}
                  <Skeleton
                    className={cn(
                      'w-full rounded-lg px-2.5 py-3',
                      index % 2 !== 0
                        ? 'bg-bg-secondary h-32 rounded-bl-none'
                        : 'h-10',
                    )}
                  />
                </div>
              ))}
            </div>
          )}
          {(hasPreviousPage || isFetchingPreviousPage) && (
            <div className="flex flex-col space-y-3" ref={ref}>
              {[...Array(4).keys()].map((index) => (
                <div
                  className={cn(
                    'flex w-[85%] gap-2',
                    index % 2 === 0
                      ? 'mr-auto ml-0 flex-col'
                      : 'mr-0 ml-auto w-[300px] items-start',
                  )}
                  key={`skeleton-prev-${index}`}
                >
                  {index % 2 !== 0 ? (
                    <Skeleton
                      className="bg-bg-quaternary size-6 shrink-0 rounded-full"
                      key={`prev-avatar-${index}`}
                    />
                  ) : null}
                  <Skeleton
                    className={cn(
                      'w-full rounded-lg px-2.5 py-3',
                      index % 2 !== 0
                        ? 'bg-bg-secondary h-32 rounded-bl-none'
                        : 'h-10',
                    )}
                  />
                </div>
              ))}
            </div>
          )}
          {isSuccess && messageList?.length > 0 && (
            <Fragment>
              {Object.entries(groupMessagesByDate(messageList)).map(
                ([date, messages]) => {
                  return (
                    <div key={date}>
                      <div className="flex flex-col">
                        {messages.map((message, messageIndex) => {
                          const previousMessage = messages[messageIndex - 1];

                          const disabledRetryAndEditValue =
                            disabledRetryAndEdit ?? messageIndex === 0;

                          const handleRetryMessage = () => {
                            regenerateMessage?.(message?.messageId ?? '');
                          };

                          const handleEditMessage = (message: string) => {
                            editAndRegenerateMessage?.(
                              message,
                              previousMessage?.messageId ?? '',
                            );
                          };

                          return (
                            <Message
                              disabledEdit={disabledRetryAndEditValue}
                              handleEditMessage={handleEditMessage}
                              handleRetryMessage={handleRetryMessage}
                              key={`${message.messageId}::${messageIndex}`}
                              message={message}
                              messageId={message.messageId}
                            />
                          );
                        })}
                      </div>
                    </div>
                  );
                },
              )}
              {messageExtra}
              {lastMessageContent}
            </Fragment>
          )}
        </div>
      </div>
    );
  },
);
MessageList.displayName = 'MessageList';
