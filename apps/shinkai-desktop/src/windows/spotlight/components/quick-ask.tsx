import { zodResolver } from '@hookform/resolvers/zod';
import { useTranslation } from '@shinkai_network/shinkai-i18n';
import { type ShinkaiToolHeader } from '@shinkai_network/shinkai-message-ts/api/tools/types';
import {
  buildInboxIdFromJobId,
  extractJobIdFromInbox,
} from '@shinkai_network/shinkai-message-ts/utils/inbox_name_handler';
import {
  type CreateJobFormSchema,
  createJobFormSchema,
} from '@shinkai_network/shinkai-node-state/forms/chat/create-job';
import { DEFAULT_CHAT_CONFIG } from '@shinkai_network/shinkai-node-state/v2/constants';
import { useCreateJob } from '@shinkai_network/shinkai-node-state/v2/mutations/createJob/useCreateJob';
import { useRetryMessage } from '@shinkai_network/shinkai-node-state/v2/mutations/retryMessage/useRetryMessage';
import { useSendMessageToJob } from '@shinkai_network/shinkai-node-state/v2/mutations/sendMessageToJob/useSendMessageToJob';
import { useGetAgents } from '@shinkai_network/shinkai-node-state/v2/queries/getAgents/useGetAgents';
import { useGetLLMProviders } from '@shinkai_network/shinkai-node-state/v2/queries/getLLMProviders/useGetLLMProviders';
import { useGetTools } from '@shinkai_network/shinkai-node-state/v2/queries/getToolsList/useGetToolsList';
import {
  Button,
  ChatInputArea,
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  Popover,
  PopoverAnchor,
  PopoverContent,
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@shinkai_network/shinkai-ui';
import { SendIcon, ToolsIcon } from '@shinkai_network/shinkai-ui/assets';
import { formatText } from '@shinkai_network/shinkai-ui/helpers';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

import { motion } from 'framer-motion';
import { ExternalLinkIcon, PlusIcon } from 'lucide-react';
import { memo, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useDropzone } from 'react-dropzone';
import { useForm } from 'react-hook-form';
import { useHotkeys } from 'react-hotkeys-hook';
import { toast } from 'sonner';

import {
  chatConfigFormSchema,
  UpdateChatConfigActionBar,
  type ChatConfigFormSchemaType,
} from '../../../components/chat/chat-action-bar/chat-config-action-bar';
import { FileSelectionActionBar } from '../../../components/chat/chat-action-bar/file-selection-action-bar';
import { ThinkingSwitchActionBar } from '../../../components/chat/chat-action-bar/thinking-switch-action-bar';
import { ToolsSwitchActionBar } from '../../../components/chat/chat-action-bar/tools-switch-action-bar';
import {
  DropFileActive,
  FileList,
} from '../../../components/chat/conversation-footer';
import {
  useWebSocketMessage,
  useWebSocketTools,
} from '../../../components/chat/websocket-message';
import { useChatConversationWithOptimisticUpdates } from '../../../pages/chat/chat-conversation';
import { useAuth } from '../../../store/auth';
import { useSettings } from '../../../store/settings';
import { getThinkingConfig } from '../../../utils/thinking-config';
import { useQuickAskStore } from '../context/quick-ask';
import { AIModelSelector, AiUpdateSelection } from './ai-update-selection';
import { MessageList } from './message-list';

export const hideSpotlightWindow = async () => {
  return invoke('hide_spotlight_window_app');
};

export const useDefaultSpotlightAiByDefault = () => {
  const auth = useAuth((state) => state.auth);
  const defaultSpotlightAiId = useSettings(
    (state) => state.defaultSpotlightAiId,
  );
  const setDefaultSpotlightAiId = useSettings(
    (state) => state.setDefaultSpotlightAiId,
  );
  const { llmProviders, isSuccess } = useGetLLMProviders({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
  });

  useEffect(() => {
    if (isSuccess && llmProviders?.length && !defaultSpotlightAiId) {
      setDefaultSpotlightAiId(llmProviders[0].id);
    }
  }, [llmProviders, isSuccess, setDefaultSpotlightAiId, defaultSpotlightAiId]);

  return {
    defaultSpotlightAiId,
    setDefaultSpotlightAiId,
  };
};

function QuickAsk() {
  const auth = useAuth((state) => state.auth);
  const { t } = useTranslation();

  const inboxId = useQuickAskStore((state) => state.inboxId);
  const setInboxId = useQuickAskStore((state) => state.setInboxId);

  const isLoadingResponse = useQuickAskStore(
    (state) => state.isLoadingResponse,
  );

  const { defaultSpotlightAiId, setDefaultSpotlightAiId } =
    useDefaultSpotlightAiByDefault();

  const handleOpenInApp = async () => {
    // await hideSpotlightWindow();
    await invoke('open_main_window_with_path_app', {
      path: inboxId ? `/inboxes/${encodeURIComponent(inboxId)}` : '/home',
    });
  };

  const chatInputRef = useRef<HTMLTextAreaElement>(null);

  const { llmProviders } = useGetLLMProviders({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
  });

  const [isCommandOpen, setIsCommandOpen] = useState(false);
  const { data: toolsList, isSuccess: isToolsListSuccess } = useGetTools(
    {
      nodeAddress: auth?.node_address ?? '',
      token: auth?.api_v2_key ?? '',
    },
    {
      select: (data: ShinkaiToolHeader[]) =>
        data.filter((tool) => tool.enabled),
    },
  );

  const chatForm = useForm<CreateJobFormSchema>({
    resolver: zodResolver(createJobFormSchema),
    defaultValues: {
      message: '',
      files: [],
    },
  });

  const chatConfigForm = useForm<ChatConfigFormSchemaType>({
    resolver: zodResolver(chatConfigFormSchema),
    defaultValues: {
      stream: DEFAULT_CHAT_CONFIG.stream,
      customPrompt: '',
      temperature: DEFAULT_CHAT_CONFIG.temperature,
      topP: DEFAULT_CHAT_CONFIG.top_p,
      topK: DEFAULT_CHAT_CONFIG.top_k,
      useTools: DEFAULT_CHAT_CONFIG.use_tools,
      thinking: DEFAULT_CHAT_CONFIG.thinking,
      reasoningEffort: DEFAULT_CHAT_CONFIG.reasoning_effort,
      webSearchEnabled: DEFAULT_CHAT_CONFIG.web_search_enabled,
    },
  });

  const createNewChat = useCallback(() => {
    setInboxId(null);
    chatForm.reset({
      message: '',
      agent: defaultSpotlightAiId,
      files: [],
    });
    chatConfigForm.reset({
      stream: DEFAULT_CHAT_CONFIG.stream,
      customPrompt: '',
      temperature: DEFAULT_CHAT_CONFIG.temperature,
      topP: DEFAULT_CHAT_CONFIG.top_p,
      topK: DEFAULT_CHAT_CONFIG.top_k,
      useTools: DEFAULT_CHAT_CONFIG.use_tools,
      thinking: DEFAULT_CHAT_CONFIG.thinking,
      reasoningEffort: DEFAULT_CHAT_CONFIG.reasoning_effort,
      webSearchEnabled: DEFAULT_CHAT_CONFIG.web_search_enabled,
    });
    chatInputRef.current?.focus();
  }, [chatConfigForm, chatForm, defaultSpotlightAiId, setInboxId]);

  useHotkeys(
    ['esc'],
    () => {
      void hideSpotlightWindow();
    },
    {
      preventDefault: true,
      enableOnFormTags: true,
    },
  );
  useHotkeys(
    ['mod+shift+o', 'ctrl+shift+o'],
    () => {
      void handleOpenInApp();
    },
    {
      preventDefault: true,
      enableOnFormTags: true,
    },
  );

  useHotkeys(
    ['mod+shift+n', 'ctrl+shift+n'],
    () => {
      void createNewChat();
    },
    {
      enableOnContentEditable: true,
      preventDefault: true,
      enableOnFormTags: true,
    },
  );

  const selectedTool = chatForm.watch('tool');
  const currentMessage = chatForm.watch('message');
  const currentFiles = chatForm.watch('files');
  const currentAI = chatForm.watch('agent');

  const onDrop = useCallback(
    (acceptedFiles: File[]) => {
      const previousFiles = chatForm.getValues('files') ?? [];
      const newFiles = [...previousFiles, ...acceptedFiles];
      chatForm.setValue('files', newFiles, { shouldValidate: true });
      chatInputRef.current?.focus();
      invoke('show_spotlight_window_app').catch(console.error);
    },
    [chatForm],
  );

  const {
    getRootProps: getRootFileProps,
    getInputProps: getInputFileProps,
    isDragActive,
    open: openFilePicker,
  } = useDropzone({
    noClick: true,
    noKeyboard: true,
    multiple: true,
    onDrop,
  });

  useEffect(() => {
    const unlisten = listen('tauri://focus', () => {
      chatForm.setFocus('message');
    });

    return () => {
      void unlisten.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    chatInputRef.current?.focus();
  }, []);

  const { mutateAsync: createJob } = useCreateJob({
    onSuccess: (data) => {
      const inboxId = buildInboxIdFromJobId(data.jobId);
      setInboxId(inboxId);
      chatForm.reset({
        message: '',
        agent: defaultSpotlightAiId,
        files: [],
      });
      chatConfigForm.reset({
        stream: DEFAULT_CHAT_CONFIG.stream,
        customPrompt: '',
        temperature: DEFAULT_CHAT_CONFIG.temperature,
        topP: DEFAULT_CHAT_CONFIG.top_p,
        topK: DEFAULT_CHAT_CONFIG.top_k,
        useTools: DEFAULT_CHAT_CONFIG.use_tools,
        thinking: DEFAULT_CHAT_CONFIG.thinking,
        reasoningEffort: DEFAULT_CHAT_CONFIG.reasoning_effort,
        webSearchEnabled: DEFAULT_CHAT_CONFIG.web_search_enabled,
      });
    },
    onError: (error) => {
      toast.error('Failed to send message', {
        description: error?.response?.data?.message ?? error.message,
      });
    },
  });

  const toolListRef = useRef<HTMLDivElement>(null);

  const scrollUpWhenSearchingTools = useCallback(() => {
    requestAnimationFrame(() => {
      toolListRef.current?.scrollTo({ top: 0 });
    });
  }, []);

  const { data: agents } = useGetAgents({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
  });

  const { mutateAsync: sendMessageToJob } = useSendMessageToJob({});

  const selectedAgent = agents?.find((agent) => agent.agent_id === currentAI);

  useEffect(() => {
    chatForm.setValue('agent', defaultSpotlightAiId);
  }, [chatForm, defaultSpotlightAiId]);

  const onSubmit = async (data: CreateJobFormSchema) => {
    if (!auth) return;

    if (!inboxId) {
      await createJob({
        nodeAddress: auth.node_address,
        token: auth.api_v2_key,
        llmProvider: data.agent,
        content: data.message,
        files: currentFiles,
        isHidden: false,
        toolKey: data.tool?.key,
        chatConfig: {
          stream: chatConfigForm.getValues('stream'),
          custom_prompt: chatConfigForm.getValues('customPrompt') ?? '',
          temperature: chatConfigForm.getValues('temperature'),
          top_p: chatConfigForm.getValues('topP'),
          top_k: chatConfigForm.getValues('topK'),
          use_tools: chatConfigForm.getValues('useTools'),
          thinking: chatConfigForm.getValues('thinking'),
          reasoning_effort: chatConfigForm.getValues('reasoningEffort'),
          web_search_enabled: chatConfigForm.getValues('webSearchEnabled'),
        },
      });
      return;
    }

    const jobId = inboxId ? extractJobIdFromInbox(inboxId) : '';

    await sendMessageToJob({
      nodeAddress: auth.node_address,
      token: auth.api_v2_key,
      jobId,
      message: data.message,
      parent: '',
      files: currentFiles,
    });

    chatForm.reset({
      message: '',
      agent: defaultSpotlightAiId,
      files: [],
    });
    chatConfigForm.reset({
      stream: DEFAULT_CHAT_CONFIG.stream,
      customPrompt: '',
      temperature: DEFAULT_CHAT_CONFIG.temperature,
      topP: DEFAULT_CHAT_CONFIG.top_p,
      topK: DEFAULT_CHAT_CONFIG.top_k,
      useTools: DEFAULT_CHAT_CONFIG.use_tools,
      thinking: DEFAULT_CHAT_CONFIG.thinking,
      reasoningEffort: DEFAULT_CHAT_CONFIG.reasoning_effort,
      webSearchEnabled: DEFAULT_CHAT_CONFIG.web_search_enabled,
    });
  };

  const thinkingConfig = useMemo(() => {
    const selectedProviderModel = llmProviders?.find(
      (p: { id: string; model: string }) => p.id === (currentAI ?? ''),
    )?.model;
    const modelName = selectedProviderModel ?? currentAI;
    return getThinkingConfig(modelName);
  }, [currentAI, llmProviders]);

  return (
    <div className="relative flex size-full flex-col gap-2">
      <div
        className="absolute top-0 z-50 h-4 w-full"
        data-tauri-drag-region={true}
      />
      <div className="font-lg flex h-[50px] items-center justify-between px-3 py-1.5">
        <span className="pl-1 text-base">{inboxId ? null : 'New Chat'}</span>
        <div className="flex items-center gap-2">
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant="tertiary"
                size="icon"
                onClick={createNewChat}
                type="button"
              >
                <PlusIcon className="size-4" />
              </Button>
            </TooltipTrigger>
            <TooltipContent className="flex flex-col items-center gap-1 text-xs">
              New Chat <br />
              <span className="inline-flex gap-1">
                {['⇧', '⌘', 'N'].map((key, i) => (
                  <kbd
                    key={i}
                    className="border-divider bg-bg-dark rounded border px-2 py-1 font-mono text-xs text-white shadow-sm"
                  >
                    {key}
                  </kbd>
                ))}
              </span>
            </TooltipContent>
          </Tooltip>
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant="tertiary"
                size="icon"
                onClick={handleOpenInApp}
                type="button"
              >
                <ExternalLinkIcon className="size-4" />
              </Button>
            </TooltipTrigger>
            <TooltipContent className="flex flex-col items-center gap-1 text-xs">
              Open in Main Window <br />
              <span className="inline-flex gap-1">
                {['⇧', '⌘', 'O'].map((key, i) => (
                  <kbd
                    key={i}
                    className="border-divider bg-bg-dark rounded border px-2 py-1 font-mono text-xs text-white shadow-sm"
                  >
                    {key}
                  </kbd>
                ))}
              </span>
            </TooltipContent>
          </Tooltip>
        </div>
      </div>
      <QuickAskBody />
      <div
        {...getRootFileProps({
          className: 'relative shrink-0 px-3 pb-3 mx-auto w-full',
        })}
      >
        <div className="relative z-[1]">
          <Popover onOpenChange={setIsCommandOpen} open={isCommandOpen}>
            <PopoverAnchor>
              <ChatInputArea
                autoFocus
                ref={chatInputRef}
                bottomAddons={
                  <div className="flex items-center justify-between gap-4 px-3 pb-2">
                    <div className="flex items-center gap-2.5">
                      {inboxId ? (
                        <AiUpdateSelection inboxId={inboxId ?? ''} />
                      ) : (
                        <AIModelSelector
                          onValueChange={(value) => {
                            chatForm.setValue('agent', value);
                            setDefaultSpotlightAiId(value);
                          }}
                          value={chatForm.watch('agent')}
                        />
                      )}

                      {!selectedTool && (
                        <FileSelectionActionBar
                          disabled={!!selectedTool}
                          inputProps={{
                            ...chatForm.register('files'),
                            ...getInputFileProps(),
                          }}
                          onClick={openFilePicker}
                        />
                      )}

                      {!selectedTool && !selectedAgent && !inboxId && (
                        <ToolsSwitchActionBar
                          checked={chatConfigForm.watch('useTools')}
                          onClick={() => {
                            chatConfigForm.setValue(
                              'useTools',
                              !chatConfigForm.watch('useTools'),
                            );
                          }}
                        />
                      )}

                      {!selectedTool &&
                        !selectedAgent &&
                        !inboxId &&
                        thinkingConfig.supportsThinking && (
                          <ThinkingSwitchActionBar
                            checked={
                              thinkingConfig.forceEnabled ||
                              chatConfigForm.watch('thinking')
                            }
                            disabled={thinkingConfig.forceEnabled}
                            forceEnabled={thinkingConfig.forceEnabled}
                            onClick={() => {
                              if (!thinkingConfig.forceEnabled) {
                                chatConfigForm.setValue(
                                  'thinking',
                                  !chatConfigForm.watch('thinking'),
                                );
                              }
                            }}
                          />
                        )}
                      {!selectedTool && !selectedAgent && inboxId && (
                        <UpdateChatConfigActionBar />
                      )}
                    </div>

                    <div className="flex items-center gap-2">
                      <Button
                        className={cn('size-[36px] p-2')}
                        disabled={isLoadingResponse}
                        isLoading={isLoadingResponse}
                        onClick={chatForm.handleSubmit(onSubmit)}
                        size="icon"
                      >
                        <SendIcon className="h-full w-full" />
                        <span className="sr-only">{t('chat.sendMessage')}</span>
                      </Button>
                    </div>
                  </div>
                }
                className="rounded-xl"
                disabled={isLoadingResponse}
                onChange={(value) => {
                  chatForm.setValue('message', value);
                }}
                onKeyDown={(e) => {
                  if (
                    e.key === '/' &&
                    !e.shiftKey &&
                    !e.ctrlKey &&
                    !e.metaKey &&
                    !currentMessage.trim()
                  ) {
                    e.preventDefault();
                    setIsCommandOpen(true);
                  }
                }}
                onPaste={(event) => {
                  const items = event.clipboardData?.items;
                  if (items) {
                    for (let i = 0; i < items.length; i++) {
                      if (items[i].type.indexOf('image') !== -1) {
                        const file = items[i].getAsFile();
                        if (file) {
                          onDrop([file]);
                          event.preventDefault();
                        }
                      }
                    }
                  }
                }}
                onSubmit={chatForm.handleSubmit(onSubmit)}
                textareaClassName={cn('max-h-[60vh] min-h-[60px] p-3 text-sm')}
                topAddons={
                  <>
                    {isDragActive && <DropFileActive />}
                    {!isDragActive &&
                      currentFiles &&
                      currentFiles.length > 0 && (
                        <FileList
                          currentFiles={currentFiles}
                          isPending={isLoadingResponse}
                          onRemoveFile={(index) => {
                            const newFiles = [...currentFiles];
                            newFiles.splice(index, 1);
                            chatForm.setValue('files', newFiles, {
                              shouldValidate: true,
                            });
                          }}
                        />
                      )}
                  </>
                }
                value={chatForm.watch('message')}
              />
            </PopoverAnchor>
            <PopoverContent
              align="start"
              className="w-[500px] p-0"
              side="top"
              sideOffset={-200}
            >
              <Command>
                <CommandInput
                  onValueChange={scrollUpWhenSearchingTools}
                  placeholder="Search tools..."
                />
                <CommandList ref={toolListRef}>
                  <CommandEmpty>No tools found.</CommandEmpty>
                  <CommandGroup heading="Your Active Tools">
                    {isToolsListSuccess &&
                      toolsList?.map((tool) => (
                        <CommandItem
                          key={tool.tool_router_key}
                          onSelect={() => {
                            chatForm.setValue('tool', {
                              key: tool.tool_router_key,
                              name: tool.name,
                              description: tool.description,
                              args: Object.keys(
                                tool.input_args.properties ?? {},
                              ),
                            });
                            chatConfigForm.setValue('useTools', true);
                            chatForm.setValue('message', 'Tool Used');
                            setIsCommandOpen(false);
                            const input = document.querySelector('input');
                            if (input) {
                              input.focus();
                            }
                          }}
                        >
                          <ToolsIcon className="mr-2 h-4 w-4" />
                          <div className="flex flex-col gap-0.5 text-xs">
                            <span className="line-clamp-1 text-white">
                              {formatText(tool.name)}
                            </span>
                            <span className="text-text-secondary line-clamp-2 text-xs">
                              {tool.description}
                            </span>
                          </div>
                        </CommandItem>
                      ))}
                  </CommandGroup>
                </CommandList>
              </Command>
            </PopoverContent>
          </Popover>
        </div>
      </div>
    </div>
  );
}

export default QuickAsk;

const QuickAskBody = () => {
  const inboxId = useQuickAskStore((state) => state.inboxId);

  const decodedInboxId = decodeURIComponent(inboxId ?? '');
  if (!decodedInboxId) {
    return (
      <div className="flex h-full w-full flex-1 items-center justify-center px-2.5 py-6">
        <motion.div
          animate={{ opacity: 1 }}
          className="flex max-w-lg flex-col items-center gap-2 text-center"
          exit={{ opacity: 0 }}
          initial={{ opacity: 0 }}
          transition={{ duration: 0.3 }}
        >
          <h1 className="font-clash text-2xl font-semibold text-white">
            How can I help you today?
          </h1>
        </motion.div>
      </div>
    );
  }
  return <QuickAskBodyWithResponse inboxId={decodedInboxId} />;
};

const QuickAskBodyWithResponseBase = ({ inboxId }: { inboxId: string }) => {
  const { t } = useTranslation();
  const auth = useAuth((state) => state.auth);
  const setLoadingResponse = useQuickAskStore(
    (state) => state.setLoadingResponse,
  );
  const setMessageResponse = useQuickAskStore(
    (state) => state.setMessageResponse,
  );

  const { mutateAsync: sendMessageToJob } = useSendMessageToJob({});
  const { mutateAsync: retryMessage } = useRetryMessage();

  const {
    data,
    fetchPreviousPage,
    hasPreviousPage,
    isChatConversationLoading,
    isFetchingPreviousPage,
    isChatConversationSuccess,
  } = useChatConversationWithOptimisticUpdates({
    inboxId,
  });

  useWebSocketMessage({
    enabled: !!inboxId,
    inboxId: inboxId,
  });

  useWebSocketTools({ inboxId: inboxId ?? '', enabled: !!inboxId });

  const editAndRegenerateMessage = async (
    content: string,
    parentHash: string,
  ) => {
    if (!auth) return;
    const decodedInboxId = decodeURIComponent(inboxId ?? '');
    const jobId = extractJobIdFromInbox(decodedInboxId);

    await sendMessageToJob({
      nodeAddress: auth.node_address,
      token: auth.api_v2_key,
      jobId,
      message: content,
      parent: parentHash,
    });
  };

  const regenerateMessage = async (messageId: string) => {
    if (!auth) return;
    const decodedInboxId = decodeURIComponent(inboxId ?? '');

    await retryMessage({
      nodeAddress: auth.node_address,
      token: auth.api_v2_key,
      inboxId: decodedInboxId,
      messageId: messageId,
    });
  };

  useEffect(() => {
    const lastMessage = data?.pages?.at(-1)?.at(-1);
    if (
      lastMessage?.role === 'assistant' &&
      lastMessage?.status.type === 'running'
    ) {
      setLoadingResponse(true);
    } else if (
      lastMessage?.role === 'assistant' &&
      lastMessage?.status.type === 'complete'
    ) {
      setLoadingResponse(false);
      setMessageResponse(lastMessage?.content ?? '');
    }
  }, [data, setLoadingResponse, setMessageResponse]);

  return (
    <MessageList
      containerClassName="px-4 py-2"
      editAndRegenerateMessage={editAndRegenerateMessage}
      fetchPreviousPage={fetchPreviousPage}
      hasPreviousPage={hasPreviousPage}
      isFetchingPreviousPage={isFetchingPreviousPage}
      isLoading={isChatConversationLoading}
      isSuccess={isChatConversationSuccess}
      noMoreMessageLabel={t('chat.allMessagesLoaded')}
      paginatedMessages={data}
      regenerateMessage={regenerateMessage}
    />
  );
};

const QuickAskBodyWithResponse = memo(
  QuickAskBodyWithResponseBase,
  (prevProps, nextProps) => prevProps.inboxId === nextProps.inboxId,
);
