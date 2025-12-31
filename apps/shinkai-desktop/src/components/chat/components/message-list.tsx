import {
  type AssistantMessage,
  type ChatConversationInfiniteData,
} from '@shinkai_network/shinkai-node-state/v2/queries/getChatConversation/types';
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
import {
  Fragment,
  memo,
  type ReactNode,
  useCallback,
  useEffect,
  useLayoutEffect,
} from 'react';
import { useInView } from 'react-intersection-observer';

import { useMessages } from '../use-messages';
import { Message } from './message';

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
    lastMessageContent?: ReactNode;
    disabledRetryAndEdit?: boolean;
    messageExtra?: ReactNode;
    hidePythonExecution?: boolean;
    minimalistMode?: boolean;
  }) => {
    const { ref: loadMoreRef, inView } = useInView();
    const messageList = paginatedMessages?.pages.flat() ?? [];

    const lastMessageStatus =
      (messageList ?? []).at(-1)?.role === 'assistant' &&
      ((messageList ?? []).at(-1) as AssistantMessage)?.status?.type ===
        'running' &&
      (messageList?.at(-1) as AssistantMessage)?.content === ''
        ? 'submitted'
        : (messageList?.at(-1) as AssistantMessage)?.status?.type === 'running'
          ? 'ready'
          : 'streaming';

    const {
      containerRef,
      isAtBottom,
      autoScroll,
      setAutoScroll,
      scrollToBottom,
      scrollDomToBottom,
      preserveScrollPosition,
      hasSentMessage,
    } = useMessages({ status: lastMessageStatus });

    // Fetch previous messages when scrolled to top
    const fetchPreviousMessages = useCallback(async () => {
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

    // Preserve scroll position after fetching previous messages
    useLayoutEffect(() => {
      if (!isFetchingPreviousPage && inView) {
        preserveScrollPosition();
      }
    }, [
      paginatedMessages,
      isFetchingPreviousPage,
      inView,
      preserveScrollPosition,
    ]);

    // Scroll to bottom when new user message is sent (odd number = user message)
    useEffect(() => {
      if (messageList?.length % 2 === 1 && autoScroll) {
        scrollDomToBottom();
      }
    }, [messageList?.length, autoScroll, scrollDomToBottom]);

    // Scroll to bottom when chat is first loaded
    useEffect(() => {
      if (isSuccess) {
        scrollDomToBottom();
      }
    }, [isSuccess, scrollDomToBottom]);

    return (
      <div
        className={cn(
          'scroll size-full overflow-y-auto overscroll-none will-change-scroll',
          'flex-1 overflow-y-auto',
          containerClassName,
        )}
        ref={containerRef}
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
            <div className="flex flex-col space-y-3" ref={loadMoreRef}>
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
                              requiresScrollPadding={
                                hasSentMessage &&
                                messageIndex === messages.length - 1
                              }
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
        {!isAtBottom && (
          <Button
            aria-label="Scroll to bottom"
            className="absolute bottom-40 left-1/2 z-10 -translate-x-1/2 rounded-full border p-2 shadow-lg transition-colors"
            onClick={() => scrollToBottom('smooth')}
            size="icon"
            type="button"
            variant="default"
          >
            <ArrowDownIcon className="size-4" />
          </Button>
        )}
      </div>
    );
  },
);
MessageList.displayName = 'MessageList';
