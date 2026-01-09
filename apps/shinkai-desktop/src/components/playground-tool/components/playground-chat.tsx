import { useTranslation } from '@shinkai_network/shinkai-i18n';
import { type CodeLanguage } from '@shinkai_network/shinkai-message-ts/api/tools/types';
import { type ChatConversationInfiniteData } from '@shinkai_network/shinkai-node-state/v2/queries/getChatConversation/types';
import { useGetLLMProviders } from '@shinkai_network/shinkai-node-state/v2/queries/getLLMProviders/useGetLLMProviders';
import {
  Button,
  ChatInputArea,
  FormControl,
  FormField,
  FormItem,
  FormLabel,
} from '@shinkai_network/shinkai-ui';
import { SendIcon } from '@shinkai_network/shinkai-ui/assets';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import {
  type InfiniteQueryObserverResult,
  type FetchPreviousPageOptions,
} from '@tanstack/react-query';
import { memo, useMemo, useEffect } from 'react';
import { useFormContext } from 'react-hook-form';

import { useAuth } from '../../../store/auth';
import { getThinkingConfig } from '../../../utils/thinking-config';
import { ThinkingSwitchActionBar } from '../../chat/chat-action-bar/thinking-switch-action-bar';
import { MessageList } from '../../chat/components/message-list';
import { usePlaygroundStore } from '../context/playground-context';
import { type CreateToolCodeFormSchema } from '../hooks/use-tool-code';
import { AIModelSelectorTools } from './ai-update-selection-tool';
import { LanguageToolSelector } from './language-tool-selector';
import { ToolsSelection } from './tools-selection';

const PlaygroundChatBase = ({
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  toolName,

  chatInboxId,
  handleCreateToolCode,
  fetchPreviousPage,
  hasPreviousPage,
  isFetchingPreviousPage,
  isChatConversationLoading,
  isChatConversationSuccess,
  chatConversationData,
}: {
  toolName: string;
  chatInboxId: string;
  handleCreateToolCode: (data: CreateToolCodeFormSchema) => void;
  fetchPreviousPage: (
    options?: FetchPreviousPageOptions | undefined,
  ) => Promise<
    InfiniteQueryObserverResult<ChatConversationInfiniteData, Error>
  >;
  hasPreviousPage: boolean;
  isFetchingPreviousPage: boolean;
  isChatConversationLoading: boolean;
  isChatConversationSuccess: boolean;
  chatConversationData: ChatConversationInfiniteData | undefined;
}) => {
  const { t } = useTranslation();
  const auth = useAuth((state) => state.auth);

  const toolCodeStatus = usePlaygroundStore((state) => state.toolCodeStatus);
  const isToolCodeGenerationPending = toolCodeStatus === 'pending';

  const toolMetadataStatus = usePlaygroundStore(
    (state) => state.toolMetadataStatus,
  );
  const isMetadataGenerationPending = toolMetadataStatus === 'pending';

  const form = useFormContext<CreateToolCodeFormSchema>();

  const { data: llmProviders } = useGetLLMProviders({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
  });

  const currentAI = form.watch('llmProviderId');

  const thinkingConfig = useMemo(() => {
    if (!currentAI || !llmProviders) {
      return {
        supportsThinking: false,
        forceEnabled: false,
        reasoningLevel: false,
      };
    }

    const selectedProvider = llmProviders.find(
      (provider) => provider.id === currentAI,
    );
    const modelName = selectedProvider?.model;
    return getThinkingConfig(modelName);
  }, [currentAI, llmProviders]);

  // Auto-enable thinking if force enabled for the model
  useEffect(() => {
    if (thinkingConfig.forceEnabled) {
      form.setValue('thinking', true);
    }
  }, [thinkingConfig.forceEnabled, form]);

  return (
    <>
      <div className={cn('flex flex-1 flex-col overflow-y-auto px-2')}>
        <MessageList
          containerClassName="px-3 pt-2"
          disabledRetryAndEdit={true}
          fetchPreviousPage={fetchPreviousPage}
          hasPreviousPage={hasPreviousPage}
          hidePythonExecution={true}
          inboxId={chatInboxId}
          isFetchingPreviousPage={isFetchingPreviousPage}
          isLoading={isChatConversationLoading}
          isSuccess={isChatConversationSuccess}
          minimalistMode
          noMoreMessageLabel={t('chat.allMessagesLoaded')}
          paginatedMessages={chatConversationData}
        />
      </div>

      <form
        className="shrink-0 space-y-2 px-3 pt-2"
        onSubmit={form.handleSubmit(handleCreateToolCode)}
      >
        <div className="flex shrink-0 items-center gap-1">
          <FormField
            control={form.control}
            name="message"
            render={({ field }) => (
              <FormItem className="flex-1 space-y-0">
                <FormLabel className="sr-only">
                  {t('chat.enterMessage')}
                </FormLabel>
                <FormControl>
                  <div className="space-y-1.5">
                    <div className="flex items-center gap-2">
                      <AIModelSelectorTools
                        onValueChange={(value) => {
                          form.setValue('llmProviderId', value);
                        }}
                        value={form.watch('llmProviderId')}
                      />
                      <LanguageToolSelector
                        onValueChange={(value) => {
                          form.setValue('language', value as CodeLanguage);
                        }}
                        value={form.watch('language')}
                      />
                      <ToolsSelection
                        onChange={(value) => {
                          form.setValue('tools', value);
                        }}
                        value={form.watch('tools')}
                      />
                      {thinkingConfig.supportsThinking && (
                        <ThinkingSwitchActionBar
                          checked={
                            thinkingConfig.forceEnabled ||
                            !!form.watch('thinking')
                          }
                          disabled={thinkingConfig.forceEnabled}
                          onClick={() => {
                            if (!thinkingConfig.forceEnabled) {
                              form.setValue(
                                'thinking',
                                !form.watch('thinking'),
                              );
                            }
                          }}
                          forceEnabled={thinkingConfig.forceEnabled}
                        />
                      )}
                    </div>
                    <ChatInputArea
                      autoFocus
                      bottomAddons={
                        <div className="flex items-end gap-3 self-end p-2">
                          <span className="text-text-secondary pb-1 text-xs font-light">
                            <span className="font-medium">Enter</span> to send
                          </span>
                          <Button
                            className={cn('size-[36px] p-2')}
                            disabled={
                              isToolCodeGenerationPending ||
                              isMetadataGenerationPending ||
                              !form.watch('message')
                            }
                            onClick={form.handleSubmit(handleCreateToolCode)}
                            size="icon"
                          >
                            <SendIcon className="h-full w-full" />
                            <span className="sr-only">
                              {t('chat.sendMessage')}
                            </span>
                          </Button>
                        </div>
                      }
                      disabled={
                        isToolCodeGenerationPending ||
                        isMetadataGenerationPending
                      }
                      onChange={field.onChange}
                      onSubmit={form.handleSubmit(handleCreateToolCode)}
                      placeholder="Send message..."
                      value={field.value}
                    />
                  </div>
                </FormControl>
              </FormItem>
            )}
          />
        </div>
      </form>
    </>
  );
};

export const PlaygroundChat = memo(PlaygroundChatBase);
