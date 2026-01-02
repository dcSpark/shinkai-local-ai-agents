import { type ChatConversationInfiniteData } from '@shinkai_network/shinkai-node-state/v2/queries/getChatConversation/types';
import { Button, Skeleton } from '@shinkai_network/shinkai-ui';
import {
  getRelativeDateLabel,
  groupMessagesByDate,
} from '@shinkai_network/shinkai-ui/helpers';
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

const SCROLL_THRESHOLD = 50;

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
  }) => {
    const chatContainerRef = useRef<HTMLDivElement>(null);
    const storedScrollHeightRef = useRef<number>(0);
    const prevMessageCountRef = useRef<number>(0);
    const hasInitiallyScrolledRef = useRef(false);
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

    const storeScrollHeight = useCallback(() => {
      const container = chatContainerRef.current;
      if (container) {
        storedScrollHeightRef.current = container.scrollHeight;
      }
    }, []);

    const fetchPreviousMessages = useCallback(async () => {
      storeScrollHeight();
      setAutoScroll(false);
      await fetchPreviousPage();
    }, [fetchPreviousPage, setAutoScroll, storeScrollHeight]);

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

    useLayoutEffect(() => {
      if (
        !isFetchingPreviousPage &&
        inView &&
        storedScrollHeightRef.current > 0
      ) {
        const container = chatContainerRef.current;
        if (!container) return;

        const currentHeight = container.scrollHeight;
        const previousHeight = storedScrollHeightRef.current;
        const heightDiff = currentHeight - previousHeight;

        if (heightDiff > 0 && !autoScroll) {
          container.scrollTop = container.scrollTop + heightDiff;
        }

        storedScrollHeightRef.current = 0; // Reset after use
      }
    }, [paginatedMessages, isFetchingPreviousPage, inView, autoScroll]);

    useEffect(() => {
      const prevCount = prevMessageCountRef.current;
      const currentCount = messageList.length;
      prevMessageCountRef.current = currentCount;

      // Detect new user message (count increased and last message is from user)
      if (currentCount > prevCount && prevCount > 0) {
        const newMessage = messageList[currentCount - 1];
        if (newMessage?.role === 'user') {
          scrollDomToBottom();
        }
      }
    }, [messageList, scrollDomToBottom]);

    useEffect(() => {
      if (isSuccess && !hasInitiallyScrolledRef.current) {
        hasInitiallyScrolledRef.current = true;
        requestAnimationFrame(() => {
          scrollDomToBottom();
        });
      }
    }, [isSuccess, scrollDomToBottom]);

    return (
      <div className="relative flex-1">
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
                        {!minimalistMode && (
                          <div
                            className={cn(
                              'bg-bg-tertiary relative z-10 m-auto my-2 flex h-[26px] w-fit min-w-[100px] items-center justify-center rounded-xl px-2.5 capitalize',
                              'sticky top-5',
                            )}
                          >
                            <span className="text-text-tertiary text-sm font-medium">
                              {getRelativeDateLabel(
                                new Date(messages[0].createdAt || ''),
                              )}
                            </span>
                          </div>
                        )}
                        <div className="flex flex-col">
                          {messages.map((message, messageIndex) => {
                            const previousMessage = messages[messageIndex - 1];

                            const disabledRetryAndEditValue =
                              disabledRetryAndEdit ?? messageIndex === 0;

                            const handleRetryMessage = () => {
                              regenerateMessage?.(message?.messageId ?? '');
                            };

                            const handleForkMessage = () => {
                              forkMessage?.(message?.messageId ?? '');
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
                                handleForkMessage={handleForkMessage}
                                handleRetryMessage={handleRetryMessage}
                                hidePythonExecution={hidePythonExecution}
                                key={`${message.messageId}::${messageIndex}`}
                                message={message}
                                messageId={message.messageId}
                                minimalistMode={minimalistMode}
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
