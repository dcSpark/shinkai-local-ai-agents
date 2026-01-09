import { zodResolver } from '@hookform/resolvers/zod';
import { PlusCircledIcon } from '@radix-ui/react-icons';
import validator from '@rjsf/validator-ajv8';
import { useTranslation } from '@shinkai_network/shinkai-i18n';
import {
  extractJobIdFromInbox,
  isJobInbox,
} from '@shinkai_network/shinkai-message-ts/utils/inbox_name_handler';
import {
  type ChatMessageFormSchema,
  chatMessageFormSchema,
} from '@shinkai_network/shinkai-node-state/forms/chat/chat-message';
import { DEFAULT_CHAT_CONFIG } from '@shinkai_network/shinkai-node-state/v2/constants';
import { useSendMessageToJob } from '@shinkai_network/shinkai-node-state/v2/mutations/sendMessageToJob/useSendMessageToJob';
import { useStopGeneratingLLM } from '@shinkai_network/shinkai-node-state/v2/mutations/stopGeneratingLLM/useStopGeneratingLLM';
import { useGetAgents } from '@shinkai_network/shinkai-node-state/v2/queries/getAgents/useGetAgents';
import { useGetChatConfig } from '@shinkai_network/shinkai-node-state/v2/queries/getChatConfig/useGetChatConfig';
import { useGetJobFolderName } from '@shinkai_network/shinkai-node-state/v2/queries/getJobFolderName/useGetJobFolderName';
import { useGetNodeStorageLocation } from '@shinkai_network/shinkai-node-state/v2/queries/getNodeStorageLocation/useGetNodeStorageLocation';
import { useGetProviderFromJob } from '@shinkai_network/shinkai-node-state/v2/queries/getProviderFromJob/useGetProviderFromJob';
import { useGetTools } from '@shinkai_network/shinkai-node-state/v2/queries/getToolsList/useGetToolsList';
import { useGetSearchTools } from '@shinkai_network/shinkai-node-state/v2/queries/getToolsSearch/useGetToolsSearch';
import {
  Button,
  ChatInput,
  ChatInputArea,
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandShortcut,
  JsonForm,
  Popover,
  PopoverAnchor,
  PopoverContent,
  PopoverTrigger,
  ToggleGroup,
  ToggleGroupItem,
  Tooltip,
  TooltipContent,
  TooltipPortal,
  TooltipTrigger,
} from '@shinkai_network/shinkai-ui';
import {
  fileIconMap,
  FileTypeIcon,
  PlusIcon,
  SendIcon,
  ToolsIcon,
} from '@shinkai_network/shinkai-ui/assets';
import { formatText, getFileExt } from '@shinkai_network/shinkai-ui/helpers';
import { useDebounce } from '@shinkai_network/shinkai-ui/hooks';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import { invoke } from '@tauri-apps/api/core';
import equal from 'fast-deep-equal';
import { partial } from 'filesize';
import { motion } from 'framer-motion';
import {
  CornerDownRight,
  EllipsisIcon,
  Loader2,
  Paperclip,
  X,
  XIcon,
} from 'lucide-react';
import { memo, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useDropzone } from 'react-dropzone';
import { useForm } from 'react-hook-form';
import { useHotkeys } from 'react-hotkeys-hook';
import { Link, useLocation } from 'react-router';
import { toast } from 'sonner';

import { useAnalytics } from '../../lib/posthog-provider';
import { useAuth } from '../../store/auth';
import { getThinkingConfig } from '../../utils/thinking-config';
import { usePromptSelectionStore } from '../prompt/context/prompt-selection-context';
import { AiUpdateSelectionActionBar } from './chat-action-bar/ai-update-selection-action-bar';
import {
  chatConfigFormSchema,
  type ChatConfigFormSchemaType,
  UpdateChatConfigActionBar,
} from './chat-action-bar/chat-config-action-bar';
import { FileSelectionActionBar } from './chat-action-bar/file-selection-action-bar';
import { OpenChatFolderActionBar } from './chat-action-bar/open-chat-folder-action-bar';
import PromptSelectionActionBar from './chat-action-bar/prompt-selection-action-bar';
import { UpdateThinkingSwitchActionBar } from './chat-action-bar/thinking-switch-action-bar';
import { UpdateToolsSwitchActionBar } from './chat-action-bar/tools-switch-action-bar';
import { UpdateVectorFsActionBar } from './chat-action-bar/vector-fs-action-bar';
import { useChatStore } from './context/chat-context';
import { useSetJobScope } from './context/set-job-scope-context';
import { useIsStreaming } from './context/streaming-context';

export const actionButtonClassnames = cn(
  'text-text-secondary hover:bg-bg-quaternary inline-flex h-[32px] w-[32px] shrink-0 cursor-pointer items-center justify-center gap-1.5 truncate rounded-full p-[8px] px-3 text-left text-[13px] font-normal hover:text-white disabled:cursor-not-allowed disabled:opacity-50',
);

export type ChatConversationLocationState = {
  files: File[];
  agentName: string;
  selectedVRFiles: string[];
  selectedVRFolders: string[];
  llmProviderId: string;
  selectedTool: {
    key: string;
    name: string;
    description: string;
    args: Record<string, any>;
  };
};

export const useSelectedFilesChat = ({ inboxId }: { inboxId?: string }) => {
  const selectedFileKeysRef = useSetJobScope(
    (state) => state.selectedFileKeysRef,
  );
  const selectedFolderKeysRef = useSetJobScope(
    (state) => state.selectedFolderKeysRef,
  );
  const onSelectedKeysChange = useSetJobScope(
    (state) => state.onSelectedKeysChange,
  );

  const location = useLocation();
  const locationState = location.state as ChatConversationLocationState;

  useEffect(() => {
    if (
      !inboxId &&
      (locationState?.selectedVRFiles?.length > 0 ||
        locationState?.selectedVRFolders?.length > 0)
    ) {
      const selectedVRFilesPathMap = locationState?.selectedVRFiles?.reduce(
        (acc, path) => {
          selectedFileKeysRef.set(path, path);
          acc[path] = {
            checked: true,
          };
          return acc;
        },
        {} as Record<string, { checked: boolean }>,
      );

      const selectedVRFoldersPathMap = locationState?.selectedVRFolders?.reduce(
        (acc, path) => {
          selectedFolderKeysRef.set(path, path);
          acc[path] = {
            checked: true,
          };
          return acc;
        },
        {} as Record<string, { checked: boolean }>,
      );

      onSelectedKeysChange({
        ...selectedVRFilesPathMap,
        ...selectedVRFoldersPathMap,
      });
    }
  }, [
    inboxId,
    locationState?.selectedVRFiles,
    locationState?.selectedVRFolders,
    selectedFileKeysRef,
    selectedFolderKeysRef,
    onSelectedKeysChange,
  ]);
  return {
    selectedFileKeysRef,
    selectedFolderKeysRef,
    clearSelectedFiles: () => {
      onSelectedKeysChange(null);
      selectedFileKeysRef.clear();
      selectedFolderKeysRef.clear();
    },
  };
};

export const SUGGESTED_TOOLS_COUNT = 3;

function ConversationChatFooter({
  inboxId,
  isLoadingMessage,
}: {
  inboxId: string;
  isLoadingMessage: boolean;
}) {
  const { t } = useTranslation();

  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const toolListRef = useRef<HTMLDivElement>(null);

  const isStreaming = useIsStreaming(inboxId);
  const wasStreamingRef = useRef(false);

  useEffect(() => {
    if (wasStreamingRef.current && !isStreaming) {
      requestAnimationFrame(() => {
        textareaRef.current?.focus();
      });
    }
    wasStreamingRef.current = isStreaming;
  }, [isStreaming]);

  const scrollUpWhenSearchingTools = useCallback(() => {
    requestAnimationFrame(() => {
      toolListRef.current?.scrollTo({ top: 0 });
    });
  }, []);

  const auth = useAuth((state) => state.auth);
  const { captureAnalyticEvent } = useAnalytics();

  const [toolFormData, setToolFormData] = useState<Record<string, any> | null>(
    null,
  );

  const promptSelected = usePromptSelectionStore(
    (state) => state.promptSelected,
  );
  const setPromptSelected = usePromptSelectionStore(
    (state) => state.setPromptSelected,
  );

  const quotedText = useChatStore((state) => state.quotedText);
  const setQuotedText = useChatStore((state) => state.setQuotedText);

  const chatForm = useForm<ChatMessageFormSchema>({
    resolver: zodResolver(chatMessageFormSchema),
    defaultValues: {
      message: '',
      files: [],
      agent: '',
    },
  });

  useEffect(() => {
    chatForm.reset();
  }, [chatForm, inboxId]);

  const selectedTool = chatForm.watch('tool');
  const currentMessage = chatForm.watch('message');
  const currentFiles = chatForm.watch('files');

  const { data: chatConfig } = useGetChatConfig(
    {
      nodeAddress: auth?.node_address ?? '',
      token: auth?.api_v2_key ?? '',
      jobId: inboxId ? extractJobIdFromInbox(inboxId) : '',
    },
    { enabled: !!inboxId },
  );

  const chatConfigForm = useForm<ChatConfigFormSchemaType>({
    resolver: zodResolver(chatConfigFormSchema),
    defaultValues: {
      stream: chatConfig?.stream ?? DEFAULT_CHAT_CONFIG.stream,
      customPrompt: chatConfig?.custom_prompt ?? '',
      temperature: chatConfig?.temperature ?? DEFAULT_CHAT_CONFIG.temperature,
      topP: chatConfig?.top_p ?? DEFAULT_CHAT_CONFIG.top_p,
      topK: chatConfig?.top_k ?? DEFAULT_CHAT_CONFIG.top_k,
      useTools: chatConfig?.use_tools ?? DEFAULT_CHAT_CONFIG.use_tools,
      thinking: chatConfig?.thinking ?? DEFAULT_CHAT_CONFIG.thinking,
      reasoningEffort:
        chatConfig?.reasoning_effort ?? DEFAULT_CHAT_CONFIG.reasoning_effort,
      webSearchEnabled:
        chatConfig?.web_search_enabled ??
        DEFAULT_CHAT_CONFIG.web_search_enabled,
    },
  });

  const [isCommandOpen, setIsCommandOpen] = useState(false);
  const { data: toolsList, isSuccess: isToolsListSuccess } = useGetTools(
    {
      nodeAddress: auth?.node_address ?? '',
      token: auth?.api_v2_key ?? '',
    },
    {
      select: (data) => data.filter((tool) => tool.enabled),
    },
  );

  useEffect(() => {
    if (chatConfig) {
      chatConfigForm.reset({
        stream: chatConfig.stream,
        customPrompt: chatConfig.custom_prompt ?? '',
        temperature: chatConfig.temperature,
        topP: chatConfig.top_p,
        topK: chatConfig.top_k,
        useTools: chatConfig.use_tools,
        thinking: chatConfig.thinking,
        reasoningEffort: chatConfig.reasoning_effort,
        webSearchEnabled: chatConfig.web_search_enabled,
      });
    }
  }, [chatConfig, chatConfigForm]);

  const { data: provider } = useGetProviderFromJob({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
    jobId: inboxId ? extractJobIdFromInbox(inboxId) : '',
  });

  const isAgentInbox = provider?.provider_type === 'Agent';

  const debounceMessage = useDebounce(currentMessage, 500);

  const { data: searchToolList, isSuccess: isSearchToolListSuccess } =
    useGetSearchTools(
      {
        nodeAddress: auth?.node_address ?? '',
        token: auth?.api_v2_key ?? '',
        search: debounceMessage,
      },
      {
        enabled: !!debounceMessage && !!currentMessage && !selectedTool,
        select: (data) =>
          data.slice(0, SUGGESTED_TOOLS_COUNT).filter((item) => item.enabled),
      },
    );

  const toolRawInput = useChatStore((state) => state.toolRawInput);
  const chatToolView = useChatStore((state) => state.chatToolView);

  // for suggested tools
  useHotkeys(
    ['mod+1', 'ctrl+1', 'mod+2', 'ctrl+2', 'mod+3', 'ctrl+3'],
    (event) => {
      if (!searchToolList?.length) return;

      const toolNumber = parseInt(event.key);
      if (toolNumber < 1 || toolNumber > SUGGESTED_TOOLS_COUNT) return;
      const selectedTool = searchToolList[toolNumber - 1];
      chatForm.setValue('tool', {
        key: selectedTool.tool_router_key,
        name: selectedTool.name,
        description: selectedTool.description,
        args: selectedTool.input_args,
      });
    },
    {
      enabled: !!searchToolList?.length && !!currentMessage && !selectedTool,
      enableOnFormTags: true,
    },
  );

  const onDrop = useCallback(
    (acceptedFiles: File[]) => {
      const previousFiles = chatForm.getValues('files') ?? [];
      const newFiles = [...previousFiles, ...acceptedFiles];
      chatForm.setValue('files', newFiles, { shouldValidate: true });
      textareaRef.current?.focus();
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
  const { data: jobChatFolderName } = useGetJobFolderName(
    {
      nodeAddress: auth?.node_address ?? '',
      token: auth?.api_v2_key ?? '',
      jobId: inboxId ? extractJobIdFromInbox(inboxId) : '',
    },
    {
      enabled: !!inboxId,
    },
  );
  const { data: nodeStorageLocation } = useGetNodeStorageLocation({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
  });

  const { data: agents } = useGetAgents({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
  });

  const { mutateAsync: sendMessageToJob } = useSendMessageToJob({
    onSuccess: (_, variables) => {
      if (variables.files && variables.files.length > 0) {
        captureAnalyticEvent('AI Chat with Files', {
          filesCount: variables.files.length,
        });
      }
      const agentSelected = agents?.find(
        (agent) => agent.agent_id === provider?.agent.id,
      );
      if (agentSelected && agentSelected.edited === false) {
        captureAnalyticEvent('Chat with Pre-built Agents', {
          agentName: agentSelected.name,
        });
      }
      if (agentSelected && agentSelected.edited === true) {
        captureAnalyticEvent('Chat with Custom Agents', undefined);
      }
      if (!agentSelected) {
        captureAnalyticEvent('Chat with AI Model', undefined);
      }
    },
    onError: (error) => {
      toast.error('Failed to send message', {
        description: error.message,
      });
    },
  });

  const onSubmit = async (data: ChatMessageFormSchema) => {
    if (!auth || data.message.trim() === '') return;

    if (isJobInbox(inboxId)) {
      // Format the tool parameters including additionalPrompt
      const formattedToolMessage = Object.keys(toolFormData ?? {})
        .map((key) => {
          return `${key}: ${toolFormData?.[key]}`;
        })
        .join('\n');

      let content = data.message;

      // Prepend quoted text if present
      if (quotedText) {
        content = `> ${quotedText}\n\n${data.message}`;
      }

      if (selectedTool) {
        content = `${selectedTool.name} \n ${formattedToolMessage}`;
      }
      if (toolRawInput && chatToolView === 'raw') {
        content = `${toolRawInput}`;
      }

      const jobId = extractJobIdFromInbox(inboxId);
      await sendMessageToJob({
        token: auth.api_v2_key,
        nodeAddress: auth.node_address,
        jobId: jobId,
        message: content,
        parent: '', // Note: we should set the parent if we want to retry or branch out
        files: currentFiles,
        toolKey: selectedTool?.key,
        provider: provider, // Pass provider for optimistic assistant message
      });
    }
    chatForm.reset();
    setToolFormData(null);
    setQuotedText(null); // Clear quoted text after sending
    setPromptSelected(undefined); // Clear prompt selection after sending
  };

  useEffect(() => {
    if (promptSelected) {
      chatForm.setValue('message', promptSelected.prompt);
    }
  }, [chatForm, promptSelected]);

  useEffect(() => {
    if (!textareaRef.current) return;
    textareaRef.current.scrollTop = textareaRef.current.scrollHeight;
    textareaRef.current.focus();
  }, [currentMessage]);

  const thinkingConfig = useMemo(() => {
    const modelName = provider?.agent?.model ?? provider?.agent?.id;
    return getThinkingConfig(modelName);
  }, [provider?.agent?.id, provider?.agent?.model]);

  return (
    <div className="container p-3 pb-1">
      <div
        {...getRootFileProps({
          className: 'relative shrink-0 pb-[40px]',
        })}
      >
        <div className="relative z-[1]">
          <Popover onOpenChange={setIsCommandOpen} open={isCommandOpen}>
            <PopoverAnchor>
              <ChatInputArea
                {...(selectedTool && {
                  alternateElement: (
                    <SelectedToolChat
                      args={selectedTool.args}
                      description={selectedTool.description}
                      name={formatText(selectedTool.name)}
                      onSubmit={() => {
                        void chatForm.handleSubmit(onSubmit)();
                      }}
                      onToolFormChange={setToolFormData}
                      remove={() => {
                        chatForm.setValue('tool', undefined);
                        chatForm.setValue('message', '');
                        setToolFormData(null);
                      }}
                      toolFormData={toolFormData}
                    />
                  ),
                })}
                autoFocus
                bottomAddons={
                  <div className="flex items-center justify-between gap-4 px-3 pb-2">
                    <div className="flex items-center gap-2.5">
                      <Popover>
                        <Tooltip>
                          <TooltipTrigger asChild>
                            <PopoverTrigger asChild>
                              <Button
                                className="h-8 w-8"
                                size="icon"
                                variant="tertiary"
                              >
                                <PlusIcon className="h-4 w-4" />
                                <span className="sr-only">Add</span>
                              </Button>
                            </PopoverTrigger>
                          </TooltipTrigger>
                          <TooltipPortal>
                            <TooltipContent align="center" side="top">
                              Add
                            </TooltipContent>
                          </TooltipPortal>
                        </Tooltip>
                        <PopoverContent align="start" className="w-48 p-2">
                          <div className="flex flex-col items-start gap-1">
                            {!selectedTool && (
                              <FileSelectionActionBar
                                inputProps={{
                                  ...chatForm.register('files'),
                                  ...getInputFileProps(),
                                }}
                                onClick={openFilePicker}
                                showLabel
                              />
                            )}

                            {!selectedTool && (
                              <PromptSelectionActionBar showLabel />
                            )}
                            {!selectedTool &&
                              jobChatFolderName &&
                              nodeStorageLocation && (
                                <OpenChatFolderActionBar
                                  showLabel
                                  onClick={async () => {
                                    try {
                                      await invoke(
                                        'shinkai_node_open_chat_folder',
                                        {
                                          storageLocation: nodeStorageLocation,
                                          chatFolderName:
                                            jobChatFolderName.folder_name,
                                        },
                                      );
                                    } catch {
                                      toast.warning(
                                        t('chat.failedToOpenChatFolder'),
                                      );
                                    }
                                  }}
                                />
                              )}
                          </div>
                        </PopoverContent>
                      </Popover>
                      <AiUpdateSelectionActionBar inboxId={inboxId} />

                      {isAgentInbox || selectedTool ? null : (
                        <UpdateToolsSwitchActionBar inboxId={inboxId} />
                      )}

                      {!isAgentInbox &&
                      !selectedTool &&
                      thinkingConfig.supportsThinking ? (
                        <UpdateThinkingSwitchActionBar
                          forceEnabled={thinkingConfig.forceEnabled}
                          inboxId={inboxId}
                        />
                      ) : null}

                      {isAgentInbox || selectedTool ? null : (
                        <UpdateVectorFsActionBar />
                      )}
                    </div>
                    <div className="flex items-center gap-2">
                      {isAgentInbox || selectedTool ? null : (
                        <UpdateChatConfigActionBar inboxId={inboxId} />
                      )}

                      {selectedTool ? (
                        <Button
                          className={cn('size-[36px] p-2')}
                          disabled={isLoadingMessage}
                          form="tools-form"
                          isLoading={isLoadingMessage}
                          size="icon"
                        >
                          <SendIcon className="h-full w-full" />
                          <span className="sr-only">
                            {t('chat.sendMessage')}
                          </span>
                        </Button>
                      ) : isLoadingMessage ? (
                        <StopGeneratingButtonInPlace inboxId={inboxId} />
                      ) : (
                        <Button
                          className={cn('size-[36px] p-2')}
                          disabled={!currentMessage}
                          onClick={chatForm.handleSubmit(onSubmit)}
                          size="icon"
                        >
                          <SendIcon className="h-full w-full" />
                          <span className="sr-only">
                            {t('chat.sendMessage')}
                          </span>
                        </Button>
                      )}
                    </div>
                    {/* <WebSearchActionBar
                                      checked={chatConfigForm.watch('useTools')}
                                      onClick={() => {
                                        chatConfigForm.setValue(
                                          'useTools',
                                          !chatConfigForm.watch('useTools'),
                                        );
                                      }}
                                    /> */}
                  </div>
                }
                disabled={isLoadingMessage}
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
                  if (
                    (e.ctrlKey || e.metaKey) &&
                    e.key === 'z' &&
                    promptSelected?.prompt === chatForm.watch('message')
                  ) {
                    chatForm.setValue('message', '');
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
                ref={textareaRef}
                textareaClassName="p-4 text-sm"
                topAddons={
                  <>
                    {quotedText && (
                      <div className="bg-bg-quaternary mx-3 mt-3 flex items-start gap-2 rounded-lg p-3">
                        <CornerDownRight className="text-text-secondary mt-0.5 h-4 w-4 shrink-0" />
                        <div className="flex-1">
                          <p className="text-text-secondary line-clamp-3 text-xs">
                            "{quotedText}"
                          </p>
                        </div>
                        <Button
                          className="h-6 w-6 shrink-0"
                          onClick={() => setQuotedText(null)}
                          size="icon"
                          variant="tertiary"
                        >
                          <X className="h-3 w-3" />
                        </Button>
                      </div>
                    )}
                    {isDragActive && <DropFileActive />}
                    {!isDragActive &&
                      currentFiles &&
                      currentFiles.length > 0 && (
                        <FileList
                          currentFiles={currentFiles}
                          isPending={isLoadingMessage}
                          onRemoveFile={(index) => {
                            const newFiles = [...currentFiles];
                            newFiles.splice(index, 1);
                            chatForm.setValue('files', newFiles, {
                              shouldValidate: true,
                            });
                          }}
                        />
                      )}
                    {/* <VectorFsActionBarPreview /> */}
                  </>
                }
                value={chatForm.watch('message')}
              />
            </PopoverAnchor>
            <PopoverContent
              align="start"
              className="w-[500px] p-0"
              side="top"
              sideOffset={10}
            >
              <Command>
                <CommandInput
                  onValueChange={scrollUpWhenSearchingTools}
                  placeholder={t('tools.searchPlaceholder')}
                />
                <CommandList ref={toolListRef}>
                  <CommandEmpty>{t('tools.commandEmpty')}</CommandEmpty>
                  <CommandGroup heading={t('tools.commandActiveHeading')}>
                    {isToolsListSuccess &&
                      toolsList?.map((tool) => (
                        <CommandItem
                          key={tool.tool_router_key}
                          onSelect={() => {
                            setIsCommandOpen(false);
                            chatForm.setValue('tool', {
                              key: tool.tool_router_key,
                              name: tool.name,
                              description: tool.description,
                              args: tool.input_args,
                            });
                            chatConfigForm.setValue('useTools', true);
                            chatForm.setValue('message', 'Tool Used');
                          }}
                        >
                          <ToolsIcon className="mr-2 h-4 w-4" />
                          <div className="flex flex-col">
                            <span className="line-clamp-1 text-white">
                              {formatText(tool.name)}
                            </span>
                            <span className="text-text-secondary line-clamp-3 text-xs whitespace-pre-wrap">
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
        <motion.div
          animate={{ opacity: 1 }}
          className={cn(
            'bg-bg-tertiary shadow-text-default absolute inset-x-2 bottom-1.5 z-0 flex h-[40px] justify-between gap-2 rounded-b-xl px-2 pt-2.5 pb-1',
          )}
          exit={{ opacity: 0 }}
          initial={{ opacity: 0 }}
          transition={{ duration: 0.2 }}
        >
          {!!debounceMessage &&
            !selectedTool &&
            isSearchToolListSuccess &&
            searchToolList?.length > 0 && (
              <div className="flex items-center gap-2 px-2">
                <span className="text-text-secondary pr-1 text-xs font-light">
                  Suggested Tools
                </span>
                {searchToolList?.map((tool, idx) => (
                  <Tooltip key={tool.tool_router_key}>
                    <TooltipTrigger asChild>
                      <motion.button
                        animate={{ opacity: 1, x: 0 }}
                        className={cn(
                          'hover:bg-bg-quaternary flex items-center gap-2 rounded-lg px-2 py-1 text-xs text-white transition-colors',
                        )}
                        exit={{ opacity: 0, x: -10 }}
                        initial={{ opacity: 0, x: -10 }}
                        key={tool.tool_router_key}
                        onClick={() => {
                          chatForm.setValue('tool', {
                            key: tool.tool_router_key,
                            name: tool.name,
                            description: tool.description,
                            args: tool.input_args,
                          });
                          chatConfigForm.setValue('useTools', true);
                          chatForm.setValue('message', 'Tool Used');
                        }}
                        type="button"
                      >
                        <ToolsIcon className="h-3 w-3" />
                        {formatText(tool.name)}
                      </motion.button>
                    </TooltipTrigger>
                    <TooltipPortal>
                      <TooltipContent
                        align="start"
                        className="max-w-[500px]"
                        side="top"
                      >
                        <span className="whitespace-pre-wrap">
                          {tool.description}
                        </span>

                        <br />
                        <div className="text-text-secondary flex items-center justify-end gap-2 text-xs">
                          <CommandShortcut>⌘ + {idx + 1}</CommandShortcut>
                        </div>
                      </TooltipContent>
                    </TooltipPortal>
                  </Tooltip>
                ))}
                <Link
                  className={cn(
                    'hover:bg-bg-quaternary flex items-center gap-2 rounded-lg px-2 py-1 text-xs text-white transition-colors',
                  )}
                  to="/tools"
                >
                  <EllipsisIcon className="size-4" />
                  All Tools
                </Link>
              </div>
            )}
          {(!debounceMessage || selectedTool) && (
            <div className="flex w-full items-center justify-between gap-2 px-2">
              <span className="text-text-secondary text-xs font-light">
                <span className="font-medium">Shift + Enter</span> for a new
                line
              </span>
              <span className="text-text-secondary text-xs font-light">
                <span className="font-medium">Enter</span> to send
              </span>
            </div>
          )}
        </motion.div>
      </div>
    </div>
  );
}

export default memo(
  ConversationChatFooter,
  (prev, next) =>
    prev.inboxId === next.inboxId &&
    prev.isLoadingMessage === next.isLoadingMessage,
);

function StopGeneratingButtonInPlaceBase({ inboxId }: { inboxId: string }) {
  const auth = useAuth((state) => state.auth);
  const { mutateAsync: stopGenerating } = useStopGeneratingLLM();

  const onStopGenerating = async () => {
    if (!inboxId) return;
    const decodedInboxId = decodeURIComponent(inboxId);
    const jobId = extractJobIdFromInbox(decodedInboxId);
    await stopGenerating({
      nodeAddress: auth?.node_address ?? '',
      token: auth?.api_v2_key ?? '',
      jobId: jobId,
    });
  };

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          className={cn('bg-bg-quaternary size-[36px] fill-white p-2')}
          onClick={onStopGenerating}
          size="icon"
          variant="tertiary"
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            viewBox="0 0 24 24"
            width="24"
            height="24"
            fill="var(--color-text-white)"
          >
            <path
              d="M4 12C4 8.72077 4 7.08116 4.81382 5.91891C5.1149 5.48891 5.48891 5.1149 5.91891 4.81382C7.08116 4 8.72077 4 12 4C15.2792 4 16.9188 4 18.0811 4.81382C18.5111 5.1149 18.8851 5.48891 19.1862 5.91891C20 7.08116 20 8.72077 20 12C20 15.2792 20 16.9188 19.1862 18.0811C18.8851 18.5111 18.5111 18.8851 18.0811 19.1862C16.9188 20 15.2792 20 12 20C8.72077 20 7.08116 20 5.91891 19.1862C5.48891 18.8851 5.1149 18.5111 4.81382 18.0811C4 16.9188 4 15.2792 4 12Z"
              stroke="var(--color-text-white)"
              strokeWidth="1.5"
            />
          </svg>
        </Button>
      </TooltipTrigger>
      <TooltipPortal>
        <TooltipContent align="center" side="top">
          Stop generating response
        </TooltipContent>
      </TooltipPortal>
    </Tooltip>
  );
}

export const StopGeneratingButtonInPlace = memo(
  StopGeneratingButtonInPlaceBase,
  (prevProps, nextProps) => prevProps.inboxId === nextProps.inboxId,
);

type FileListProps = {
  currentFiles: File[];
  onRemoveFile: (index: number) => void;
  isPending?: boolean;
};

// TODO: unify with file-preview.tsx
const FileListBase = ({
  currentFiles,
  onRemoveFile,
  isPending,
}: FileListProps) => {
  const size = partial({ standard: 'jedec' });

  return (
    <div className="no-scrollbar bg-bg-quaternary scroll border-divider h-16 overflow-hidden border-b">
      <div className="flex items-center gap-3 overflow-x-auto p-2.5">
        {currentFiles.map((file, index) => (
          <div
            className="border-divider relative flex h-10 w-[180px] shrink-0 items-center gap-1.5 rounded-lg border px-1 py-1.5 pr-2.5"
            key={index}
          >
            <div className="flex w-6 shrink-0 items-center justify-center">
              {isPending ? (
                <Loader2 className="text-text-secondary h-5 w-5 animate-spin" />
              ) : getFileExt(file.name) &&
                fileIconMap[getFileExt(file.name)] ? (
                <FileTypeIcon
                  className="text-text-secondary h-[18px] w-[18px] shrink-0"
                  type={getFileExt(file.name)}
                />
              ) : (
                <Paperclip className="text-text-secondary h-4 w-4 shrink-0" />
              )}
            </div>

            <div className="text-left text-xs">
              <span className="line-clamp-1 break-all">{file.name}</span>
              <span className="text-text-tertiary line-clamp-1 break-all">
                {size(file.size)}
              </span>
            </div>
            <button
              className={cn(
                'bg-bg-tertiary hover:bg-bg-quaternary text-text-secondary border-divider absolute -top-2 -right-2 h-5 w-5 cursor-pointer rounded-full border p-1 transition-colors hover:text-white',
              )}
              onClick={(event) => {
                event.stopPropagation();
                onRemoveFile(index);
              }}
            >
              <X className="h-full w-full" />
            </button>
          </div>
        ))}
      </div>
    </div>
  );
};
export const FileList = memo(FileListBase, (prevProps, nextProps) => {
  if (!equal(prevProps.currentFiles, nextProps.currentFiles)) return false;
  if (prevProps.isPending !== nextProps.isPending) return false;
  return true;
});

export const DropFileActive = () => (
  <motion.div
    animate={{ opacity: 1, y: 0 }}
    className="bg-bg-dark z-10 flex h-16 w-full items-center justify-center p-2.5"
    initial={{ opacity: 0, y: 10 }}
    transition={{ duration: 0.2 }}
  >
    <div className="w-full rounded-lg border border-dashed border-gray-100">
      <div className="flex w-full items-center justify-center px-4 py-3.5">
        <PlusCircledIcon className="h-4 w-4 text-white" />
        <span className="ml-2 text-xs font-medium text-white">
          Drop file here to add to your conversation
        </span>
      </div>
    </div>
  </motion.div>
);
export type ToolView = 'form' | 'raw';

export const SelectedToolChat = ({
  name,
  description,
  args,
  remove,
  toolFormData,
  onToolFormChange,
  onSubmit,
}: {
  name: string;
  description: string;
  args: any;
  remove: () => void;
  toolFormData: Record<string, any> | null;
  onToolFormChange: (formData: Record<string, any> | null) => void;
  onSubmit: () => void;
}) => {
  const chatToolView = useChatStore((state) => state.chatToolView);
  const setChatToolView = useChatStore((state) => state.setChatToolView);
  const toolRawInput = useChatStore((state) => state.toolRawInput);
  const setToolRawInput = useChatStore((state) => state.setToolRawInput);

  const toolInputRef = useRef<HTMLTextAreaElement>(null);

  // Create an enhanced schema that includes the additional request field
  const enhancedSchema = {
    ...args,
    properties: {
      ...(args.properties || {}),
      additionalRequest: {
        type: 'string',
        title: 'Additional Request',
        description:
          'Optional text that will be added to the prompt sent to the AI',
      },
    },
  };

  // Create a custom UI schema to properly order and format fields
  const uiSchema = {
    ...Object.keys(args.properties || {}).reduce(
      (acc: Record<string, any>, key) => {
        acc[key] = { 'ui:order': 1 }; // Put regular parameters first
        return acc;
      },
      {},
    ),
    additionalRequest: {
      'ui:widget': 'textarea',
      'ui:options': {
        rows: 3,
      },
      'ui:order': 2, // Put additional request last
      'ui:placeholder': 'Add additional instructions or context for the AI',
    },
    'ui:submitButtonOptions': { norender: true },
  };

  useEffect(() => {
    if (chatToolView === 'raw') {
      toolInputRef.current?.focus();
      toolInputRef.current?.setSelectionRange(
        toolInputRef.current?.value.length,
        toolInputRef.current?.value.length,
      );
    }
  }, [chatToolView]);

  return (
    <motion.div
      animate={{ opacity: 1 }}
      className="bg-bg-dark mb-1 max-h-[50vh] w-full max-w-full overflow-auto rounded-lg p-4 px-5 text-left [&_textarea::placeholder]:text-[rgb(176,176,176)]"
      exit={{ opacity: 0 }}
      initial={{ opacity: 0 }}
      transition={{ duration: 0.2, ease: 'easeInOut' }}
    >
      <div className="text-text-secondary flex flex-1 flex-col gap-2 text-sm">
        <Tooltip>
          <TooltipTrigger asChild>
            <div className="flex items-center gap-2">
              <ToolsIcon className="size-3.5" />
              <span className="line-clamp-1 text-left font-medium text-white">
                {name}
              </span>
            </div>
          </TooltipTrigger>
          <TooltipPortal>
            <TooltipContent
              align="start"
              alignOffset={-10}
              className="max-w-[400px]"
              side="top"
              sideOffset={10}
            >
              <span className="text-xs text-white">{description}</span>
            </TooltipContent>
          </TooltipPortal>
        </Tooltip>
        {chatToolView === 'form' && (
          <JsonForm
            className="py-1"
            formData={toolFormData}
            id="tools-form"
            noHtml5Validate={true}
            onChange={(e) => onToolFormChange(e.formData)}
            onSubmit={onSubmit}
            schema={enhancedSchema}
            uiSchema={uiSchema}
            validator={validator}
          />
        )}
        {chatToolView === 'raw' && (
          <div className="flex items-center gap-2">
            <ChatInput
              className="h-full w-full text-white"
              onChange={(e) => setToolRawInput(e.target.value)}
              onSend={() => {
                onSubmit();
                setToolRawInput('');
                setTimeout(() => {
                  setChatToolView('form');
                }, 1000);
              }}
              placeholder="Enter your prompt..."
              ref={toolInputRef}
              value={toolRawInput}
            />
          </div>
        )}
      </div>
      <div className="text-text-secondary absolute top-3 right-3 flex items-center gap-6 hover:text-white">
        <ToggleGroup
          className="bg-background inline-flex gap-0 -space-x-px rounded-lg shadow-xs shadow-black/5 rtl:space-x-reverse"
          onValueChange={(value) => {
            if (value) {
              let text = '';
              if (Object.keys(toolFormData || {}).length === 0) {
                text = `${Object.keys(enhancedSchema.properties || {})
                  .map((key) => `${key}:  [insert value]`)
                  .join('\n')}`;
              } else {
                text = `${Object.entries(toolFormData || {})
                  .map(([key, value]) => `${key}: ${value}`)
                  .join('\n')}`;
              }
              setToolRawInput(text);
              setChatToolView(value as ToolView);
            }
          }}
          type="single"
          value={chatToolView}
          variant="outline"
        >
          <ToggleGroupItem
            className="rounded-none shadow-none first:rounded-s-lg last:rounded-e-lg focus-visible:z-10"
            size="sm"
            value="form"
          >
            Form
          </ToggleGroupItem>

          <ToggleGroupItem
            className="rounded-none shadow-none first:rounded-s-lg last:rounded-e-lg focus-visible:z-10"
            size="sm"
            value="raw"
          >
            Raw Input
          </ToggleGroupItem>
        </ToggleGroup>
        <button
          className="text-text-secondary hover:text-white"
          onClick={() => {
            remove();
            setToolRawInput('');
            setChatToolView('form');
          }}
          type="button"
        >
          <XIcon className="h-4 w-4" />
        </button>
      </div>
    </motion.div>
  );
};
