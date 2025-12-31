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
  useMemo,
} from 'react';
import { useInView } from 'react-intersection-observer';

import { useMessages } from '../use-messages';
import { Message } from './message';

type ChatStatus = 'submitted' | 'streaming' | 'ready' | 'error';

function getMessageStatus(messageList: unknown[]): ChatStatus {
  const lastMessage = messageList.at(-1);
  if (!lastMessage) return 'ready';

  // Check if last message is from assistant
  if ((lastMessage as { role?: string }).role !== 'assistant') {
    return 'ready';
  }

  const assistantMessage = lastMessage as AssistantMessage;
  const isRunning = assistantMessage.status?.type === 'running';
  const isComplete = assistantMessage.status?.type === 'complete';
  const hasContent = Boolean(assistantMessage.content);

  if (isRunning && !hasContent) {
    return 'submitted';
  }

  if (isRunning && hasContent) {
    return 'streaming';
  }
  if (isComplete) {
    return 'ready';
  }

  return 'ready';
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
    lastMessageContent?: ReactNode;
    disabledRetryAndEdit?: boolean;
    messageExtra?: ReactNode;
    hidePythonExecution?: boolean;
    minimalistMode?: boolean;
  }) => {
    const { ref: loadMoreRef, inView } = useInView();
    const messageList = useMemo(
      () => paginatedMessages?.pages.flat() ?? [],
      [paginatedMessages],
    );

    const lastMessageStatus = useMemo(
      () => getMessageStatus(messageList),
      [messageList],
    );

    const {
      containerRef,
      isAtBottom,
      setAutoScroll,
      scrollToBottom,
      scrollDomToBottom,
      storeScrollHeight,
      preserveScrollPosition,
      hasSentMessage,
    } = useMessages({ status: lastMessageStatus });

    // Fetch previous messages when scrolled to top
    const fetchPreviousMessages = useCallback(async () => {
      storeScrollHeight(); // Store height before fetching
      setAutoScroll(false); // Disable auto-scroll during pagination
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

    // Preserve scroll position after fetching previous messages
    useLayoutEffect(() => {
      if (!isFetchingPreviousPage && inView) {
        preserveScrollPosition();
      }
    }, [isFetchingPreviousPage, inView, preserveScrollPosition]);

    // // Scroll to bottom when chat is first loaded
    // useEffect(() => {
    //   if (isSuccess && messageList.length > 0) {
    //     scrollDomToBottom();
    //   }
    //   // eslint-disable-next-line react-hooks/exhaustive-deps
    // }, [isSuccess]);

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
