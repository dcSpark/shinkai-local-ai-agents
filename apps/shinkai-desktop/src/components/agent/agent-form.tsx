import { zodResolver } from '@hookform/resolvers/zod';
import { DialogClose } from '@radix-ui/react-dialog';
import { PlusIcon } from '@radix-ui/react-icons';
import * as SelectPrimitive from '@radix-ui/react-select'; // <-- Import SelectPrimitive
import { useTranslation } from '@shinkai_network/shinkai-i18n';
import {
  type ShinkaiTool,
  type ShinkaiToolHeader,
  type ShinkaiToolType,
} from '@shinkai_network/shinkai-message-ts/api/tools/types';
import {
  buildInboxIdFromJobId,
  extractJobIdFromInbox,
} from '@shinkai_network/shinkai-message-ts/utils/inbox_name_handler';
import {
  type UploadVRFilesFormSchema,
  uploadVRFilesFormSchema,
} from '@shinkai_network/shinkai-node-state/forms/vector-fs/folder';
import { transformDataToTreeNodes } from '@shinkai_network/shinkai-node-state/lib/utils/files';
import {
  DEFAULT_CHAT_CONFIG,
  FunctionKeyV2,
} from '@shinkai_network/shinkai-node-state/v2/constants';
import { useCreateAgent } from '@shinkai_network/shinkai-node-state/v2/mutations/createAgent/useCreateAgent';
import { useCreateJob } from '@shinkai_network/shinkai-node-state/v2/mutations/createJob/useCreateJob';
import { useCreateRecurringTask } from '@shinkai_network/shinkai-node-state/v2/mutations/createRecurringTask/useCreateRecurringTask';
import { useRemoveRecurringTask } from '@shinkai_network/shinkai-node-state/v2/mutations/removeRecurringTask/useRemoveRecurringTask';
import { useRetryMessage } from '@shinkai_network/shinkai-node-state/v2/mutations/retryMessage/useRetryMessage';
import { useSendMessageToJob } from '@shinkai_network/shinkai-node-state/v2/mutations/sendMessageToJob/useSendMessageToJob';
import { useUpdateAgent } from '@shinkai_network/shinkai-node-state/v2/mutations/updateAgent/useUpdateAgent';
import { useUploadVRFiles } from '@shinkai_network/shinkai-node-state/v2/mutations/uploadVRFiles/useUploadVRFiles';
import { useGetAgent } from '@shinkai_network/shinkai-node-state/v2/queries/getAgent/useGetAgent';
import { useGetListDirectoryContents } from '@shinkai_network/shinkai-node-state/v2/queries/getDirectoryContents/useGetListDirectoryContents';
import { useGetAgentInboxes } from '@shinkai_network/shinkai-node-state/v2/queries/getInboxes/useGetAgentInboxes';
import { useGetLLMProviders } from '@shinkai_network/shinkai-node-state/v2/queries/getLLMProviders/useGetLLMProviders';
import { useGetSearchDirectoryContents } from '@shinkai_network/shinkai-node-state/v2/queries/getSearchDirectoryContents/useGetSearchDirectoryContents';
import { useGetTool } from '@shinkai_network/shinkai-node-state/v2/queries/getTool/useGetTool';
import { useGetTools } from '@shinkai_network/shinkai-node-state/v2/queries/getToolsList/useGetToolsList';
import { useGetSearchTools } from '@shinkai_network/shinkai-node-state/v2/queries/getToolsSearch/useGetToolsSearch';
import {
  Badge,
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbSeparator,
  Button,
  buttonVariants,
  ChatInputArea,
  Checkbox,
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  FileUploader,
  Form,
  FormControl,
  FormDescription,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
  HoverCard,
  HoverCardContent,
  HoverCardTrigger,
  Label,
  RadioGroup,
  RadioGroupItem,
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
  SearchInput,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Skeleton,
  Slider,
  Switch,
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
  Textarea,
  TextField,
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@shinkai_network/shinkai-ui';
import {
  DirectoryTypeIcon,
  FileTypeIcon,
  ScheduledTasksIcon,
  SendIcon,
} from '@shinkai_network/shinkai-ui/assets';
import {
  formatDateToLocaleStringWithTime,
  formatText,
  getFileExt,
} from '@shinkai_network/shinkai-ui/helpers';
import { useDebounce } from '@shinkai_network/shinkai-ui/hooks';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import { useQueryClient } from '@tanstack/react-query';
import cronstrue from 'cronstrue';
import {
  AlertCircle,
  AlertTriangleIcon,
  BoltIcon,
  ChevronDownIcon,
  ChevronRight,
  HistoryIcon,
  LucideArrowLeft,
  MessageSquare,
  Trash2,
  XIcon,
} from 'lucide-react';
import { Tree, type TreeCheckboxSelectionKeys } from 'primereact/tree';
import { type TreeNode } from 'primereact/treenode';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { useForm } from 'react-hook-form';
import {
  Link,
  type To,
  useNavigate,
  useParams,
  useSearchParams,
} from 'react-router';
import { toast } from 'sonner';
import { merge } from 'ts-deepmerge';
import { z } from 'zod';

import { useSetJobScope } from '../../components/chat/context/set-job-scope-context';
import { useURLQueryParams } from '../../hooks/use-url-query-params';
import { treeOptions } from '../../lib/constants';
import { useAnalytics } from '../../lib/posthog-provider';
import { useChatConversationWithOptimisticUpdates } from '../../pages/chat/chat-conversation';
import { useAuth } from '../../store/auth';
import { useSettings } from '../../store/settings';
import { getThinkingConfig } from '../../utils/thinking-config';
import { getToolRequiresConfigurations } from '../../utils/tools-configurations';
import { AIModelSelector } from '../chat/chat-action-bar/ai-update-selection-action-bar';
import { MessageList } from '../chat/components/message-list';
import { ChatProvider } from '../chat/context/chat-context';
import { ToolsProvider } from '../chat/context/tools-context';
import {
  useWebSocketMessage,
  useWebSocketTools,
} from '../chat/websocket-message';
import ToolDetailsCard from '../tools/components/tool-details-card';
import { TooConfigOverrideForm } from './tool-config-override-form';

const agentFormSchema = z.object({
  name: z.string(),
  llmProviderId: z.string(),
  uiDescription: z.string(),
  storage_path: z.string(),
  knowledge: z.array(z.string()),
  tools: z.array(z.string()),
  tools_config_override: z.record(z.record(z.any())).optional(),
  debugMode: z.boolean(),
  config: z
    .object({
      custom_prompt: z.string(),
      custom_system_prompt: z.string(),
      temperature: z.number(),
      top_k: z.number(),
      top_p: z.number(),
      use_tools: z.boolean(),
      stream: z.boolean(),
      thinking: z.boolean(),
      reasoning_effort: z.enum(['low', 'medium', 'high']).optional(),
      web_search_enabled: z.boolean().optional(),
      other_model_params: z.record(z.string()),
    })
    .nullable(),
  cronExpression: z.string().optional(),
  aiPrompt: z.string().optional(),
});

type AgentFormValues = z.infer<typeof agentFormSchema>;

const TAB_VALUES = [
  'persona',
  'workflow',
  'knowledge',
  'tools',
  'schedule',
] as const;

type TabValue = (typeof TAB_VALUES)[number];

const isTabValue = (value: string | null): value is TabValue =>
  typeof value === 'string' &&
  TAB_VALUES.some((tabValue) => tabValue === value);

const getPreviousTab = (tab: TabValue): TabValue | null => {
  const index = TAB_VALUES.indexOf(tab);

  if (index <= 0) {
    return null;
  }

  return TAB_VALUES[index - 1];
};

const getNextTab = (tab: TabValue): TabValue | null => {
  const index = TAB_VALUES.indexOf(tab);

  if (index === -1 || index >= TAB_VALUES.length - 1) {
    return null;
  }

  return TAB_VALUES[index + 1];
};

interface AgentFormProps {
  mode: 'add' | 'edit';
}

const TabNavigation = () => {
  const { t } = useTranslation();
  return (
    <TabsList className="border-divider flex h-auto justify-center gap-4 rounded-full bg-transparent px-0.5 py-1">
      <TabsTrigger
        className="data-[state=active]:bg-bg-secondary text-text-secondary border-divider data-[state=active]:text-text-default h-full gap-2 rounded-full border bg-transparent px-4 py-2 text-xs font-medium"
        value="persona"
      >
        <span>{t('agents.create.persona')}</span>
      </TabsTrigger>
      <TabsTrigger
        className="data-[state=active]:bg-bg-secondary text-text-secondary border-divider data-[state=active]:text-text-default h-full gap-2 rounded-full border bg-transparent px-4 py-2 text-xs font-medium"
        value="workflow"
      >
        <span>Workflow</span>
        <Badge className="bg-bg-quaternary border-divider text-text-secondary inline-flex items-center justify-center rounded-full border-none p-0 text-center text-[10px]">
          NEW
        </Badge>
      </TabsTrigger>
      <TabsTrigger
        className="data-[state=active]:bg-bg-secondary text-text-secondary border-divider data-[state=active]:text-text-default h-full gap-2 rounded-full border bg-transparent px-4 py-2 text-xs font-medium"
        value="knowledge"
      >
        <span>{t('agents.create.knowledge')}</span>
      </TabsTrigger>
      <TabsTrigger
        className="data-[state=active]:bg-bg-secondary text-text-secondary border-divider data-[state=active]:text-text-default h-full gap-2 rounded-full border bg-transparent px-4 py-2 text-xs font-medium"
        value="tools"
      >
        <span>{t('agents.create.tools')}</span>
      </TabsTrigger>
      <TabsTrigger
        className="data-[state=active]:bg-bg-secondary text-text-secondary border-divider data-[state=active]:text-text-default h-full gap-2 rounded-full border bg-transparent px-4 py-2 text-xs font-medium"
        value="schedule"
      >
        <span>{t('agents.create.schedule')}</span>
      </TabsTrigger>
    </TabsList>
  );
};

function AgentSideChat({
  agentId,
  onClose,
}: {
  agentId: string;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const auth = useAuth((state) => state.auth);
  const [chatInboxId, setChatInboxId] = useState<string | null>(null);
  const [message, setMessage] = useState('');

  const { data: sideAgentData } = useGetAgent({
    agentId,
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
  });

  const hasTools = useMemo(() => {
    return !!sideAgentData?.tools && sideAgentData.tools.length > 0;
  }, [sideAgentData]);

  useWebSocketMessage({
    inboxId: chatInboxId ?? '',
    enabled: !!chatInboxId,
  });
  useWebSocketTools({ inboxId: chatInboxId ?? '', enabled: !!chatInboxId });

  const { mutateAsync: createJob } = useCreateJob({
    onSuccess: (data) => {
      const newInboxId = buildInboxIdFromJobId(data.jobId);
      setChatInboxId(newInboxId);
    },
  });

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
    inboxId: chatInboxId ?? '',
  });

  const isLoadingMessage = useMemo(() => {
    const lastMessage = data?.pages?.at(-1)?.at(-1);
    return (
      !!chatInboxId &&
      lastMessage?.role === 'assistant' &&
      lastMessage?.status.type === 'running'
    );
  }, [data?.pages, chatInboxId]);

  const handleSendMessage = async () => {
    if (!auth || !message.trim()) return;

    if (!chatInboxId) {
      await createJob({
        nodeAddress: auth.node_address,
        token: auth.api_v2_key,
        llmProvider: agentId,
        content: message,
        isHidden: true,
        chatConfig: {
          stream: DEFAULT_CHAT_CONFIG.stream,
          custom_prompt: '',
          temperature: DEFAULT_CHAT_CONFIG.temperature,
          top_p: DEFAULT_CHAT_CONFIG.top_p,
          top_k: DEFAULT_CHAT_CONFIG.top_k,
          use_tools: hasTools,
          thinking: DEFAULT_CHAT_CONFIG.thinking,
          reasoning_effort: DEFAULT_CHAT_CONFIG.reasoning_effort,
          web_search_enabled: DEFAULT_CHAT_CONFIG.web_search_enabled,
        },
      });
    } else {
      await sendMessageToJob({
        nodeAddress: auth.node_address,
        token: auth.api_v2_key,
        jobId: extractJobIdFromInbox(chatInboxId),
        message: message,
        parent: '',
      });
    }
    setMessage('');
  };

  const editAndRegenerateMessage = async (
    content: string,
    parentHash: string,
  ) => {
    if (!auth) return;
    const decodedInboxId = decodeURIComponent(chatInboxId ?? '');
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
    const decodedInboxId = decodeURIComponent(chatInboxId ?? '');

    await retryMessage({
      nodeAddress: auth.node_address,
      token: auth.api_v2_key,
      inboxId: decodedInboxId,
      messageId: messageId,
    });
  };

  const { data: agentInboxes } = useGetAgentInboxes({
    agentId,
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
    showHidden: true,
  });

  return (
    <ChatProvider>
      <div className="bg-bg-tertiary h-full shadow-lg">
        <div className="flex h-full flex-col">
          {/* Header Area */}
          <div className="flex items-center justify-between p-4">
            {/* Title */}
            <h2 className="truncate text-base font-medium">
              {chatInboxId === null
                ? t('agents.form.newChat')
                : agentInboxes?.find((inbox) => inbox.inbox_id === chatInboxId)
                    ?.custom_name ||
                  chatInboxId ||
                  t('agents.form.newChat')}
            </h2>

            {/* Buttons Group */}
            <div className="flex items-center gap-2">
              {/* New Chat Button*/}
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button
                    className="text-text-secondary p-2"
                    onClick={() => setChatInboxId(null)}
                    size="auto"
                    variant="tertiary"
                  >
                    <PlusIcon className="size-4" />
                  </Button>
                </TooltipTrigger>
                <TooltipContent>
                  <p>{t('agents.form.newChat')}</p>
                </TooltipContent>
              </Tooltip>

              <Select
                onValueChange={(value) => {
                  if (value) setChatInboxId(value);
                }}
                value={chatInboxId === null ? undefined : chatInboxId}
              >
                <Tooltip>
                  <TooltipTrigger asChild>
                    <SelectPrimitive.Trigger asChild>
                      <Button
                        className="text-text-secondary p-2"
                        disabled={!agentInboxes || agentInboxes.length === 0}
                        size="auto"
                        variant="tertiary"
                      >
                        <HistoryIcon className="size-4" />
                      </Button>
                    </SelectPrimitive.Trigger>
                  </TooltipTrigger>
                  <TooltipContent>
                    <p>{t('agents.form.chatHistory')}</p>
                  </TooltipContent>
                </Tooltip>
                <SelectContent className="w-[300px]">
                  {agentInboxes?.map((inbox) => (
                    <SelectItem key={inbox.inbox_id} value={inbox.inbox_id}>
                      <div className="flex w-full flex-col">
                        <span className="text-text-default max-w-[255px] truncate text-sm">
                          {inbox.custom_name || inbox.inbox_id}
                        </span>
                        <span className="text-text-secondary truncate text-xs">
                          {formatDateToLocaleStringWithTime(
                            new Date(inbox.datetime_created),
                          )}
                        </span>
                      </div>
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>

              <Tooltip>
                <TooltipTrigger asChild>
                  <Button
                    className="text-text-tertiary p-2"
                    onClick={onClose}
                    size="auto"
                    variant="tertiary"
                  >
                    <XIcon className="size-4" />
                  </Button>
                </TooltipTrigger>
                <TooltipContent>
                  <p>{t('agents.form.closeChat')}</p>
                </TooltipContent>
              </Tooltip>
            </div>
          </div>

          <div className="flex-1 overflow-y-auto p-4">
            {!chatInboxId ? (
              <div className="flex h-full flex-col items-center justify-center gap-2 text-center">
                <span aria-hidden className="text-4xl">
                  🤖
                </span>
                <h2 className="text-base font-medium">
                  {t('agents.form.emptyChatTitle')}
                </h2>
                <p className="text-text-secondary">
                  {t('agents.form.emptyChatDescription')}
                </p>
              </div>
            ) : (
              <MessageList
                containerClassName="px-2"
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
            )}
          </div>

          <div className="p-4">
            <ChatInputArea
              autoFocus
              bottomAddons={
                <div className="relative z-50 flex items-end gap-3 self-end p-2">
                  <span className="text-text-tertiary pb-1 font-light">
                    <span className="font-medium">
                      {t('agents.form.enter')}
                    </span>{' '}
                    {t('agents.form.enterToSend')}
                  </span>

                  <Button
                    className={cn('size-[36px] p-2')}
                    disabled={isLoadingMessage || !message.trim()}
                    isLoading={isLoadingMessage}
                    onClick={handleSendMessage}
                    size="icon"
                    type="submit"
                  >
                    <SendIcon className="h-full w-full" />
                    <span className="sr-only">{t('chat.sendMessage')}</span>
                  </Button>
                </div>
              }
              disabled={isLoadingMessage}
              onChange={(value) => setMessage(value)}
              onSubmit={handleSendMessage}
              placeholder={t('agents.form.messagePlaceholder')}
              value={message}
            />
          </div>
        </div>
      </div>
    </ChatProvider>
  );
}

function AgentForm({ mode }: AgentFormProps) {
  const { agentId } = useParams();
  const { captureAnalyticEvent } = useAnalytics();

  const defaultAgentId = useSettings((state) => state.defaultAgentId);
  const auth = useAuth((state) => state.auth);
  const navigate = useNavigate();
  const { t } = useTranslation();
  const [searchParams] = useSearchParams();
  const defaultTabParam = searchParams.get('defaultTab');
  const defaultTabValue = isTabValue(defaultTabParam)
    ? defaultTabParam
    : 'persona';

  const [currentTab, setCurrentTab] = useState<TabValue>(defaultTabValue);
  const isWorkflowTab = currentTab === 'workflow';

  const [isSideChatOpen, setIsSideChatOpen] = useState(false);
  const [selectedToolConfig, setSelectedToolConfig] = useState<null | string>(
    null,
  );

  const [scheduleType, setScheduleType] = useState<'normal' | 'scheduled'>(
    'normal',
  );

  const [searchQueryKnowledge, setSearchQueryKnowledge] = useState('');
  const debouncedSearchQueryKnowledge = useDebounce(searchQueryKnowledge, 600);
  const isSearchQueryKnowledgeSynced =
    searchQueryKnowledge === debouncedSearchQueryKnowledge;

  const { data: searchKnowledgeList, isPending: isSearchKnowledgeListPending } =
    useGetSearchDirectoryContents(
      {
        nodeAddress: auth?.node_address ?? '',
        token: auth?.api_v2_key ?? '',
        name: debouncedSearchQueryKnowledge,
      },
      {
        enabled: !!debouncedSearchQueryKnowledge,
      },
    );

  const [searchQuery, setSearchQuery] = useState('');
  const debouncedSearchQuery = useDebounce(searchQuery, 600);
  const isSearchQuerySynced = searchQuery === debouncedSearchQuery;

  const { data: searchToolList, isLoading: isSearchToolListPending } =
    useGetSearchTools(
      {
        nodeAddress: auth?.node_address ?? '',
        token: auth?.api_v2_key ?? '',
        search: debouncedSearchQuery,
      },
      { enabled: isSearchQuerySynced && !!searchQuery },
    );

  const { data: fileInfoArray } = useGetListDirectoryContents({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
    path: '/',
    depth: 6,
  });

  const queryParams = useURLQueryParams();
  const isOpenQueryActive = queryParams.get('openChat') === 'true';

  const nodes = useMemo(() => {
    return transformDataToTreeNodes(fileInfoArray ?? [], undefined, []);
  }, [fileInfoArray]);

  const [isAddFilesDialogOpen, setIsAddFilesDialogOpen] = useState(false);

  const selectedKeys = useSetJobScope((state) => state.selectedKeys);
  const selectedFileKeysRef = useSetJobScope(
    (state) => state.selectedFileKeysRef,
  );
  const selectedFolderKeysRef = useSetJobScope(
    (state) => state.selectedFolderKeysRef,
  );
  const onSelectedKeysChange = useSetJobScope(
    (state) => state.onSelectedKeysChange,
  );

  const { data: agent } = useGetAgent({
    agentId: agentId ?? '',
    token: auth?.api_v2_key ?? '',
    nodeAddress: auth?.node_address ?? '',
  });

  const { data: llmProviders } = useGetLLMProviders({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
  });

  const form = useForm<AgentFormValues>({
    resolver: zodResolver(agentFormSchema),
    defaultValues: {
      name: '',
      uiDescription: '',
      storage_path: '',
      knowledge: [],
      tools: [],
      tools_config_override: {},
      debugMode: false,
      config: {
        stream: DEFAULT_CHAT_CONFIG.stream,
        temperature: DEFAULT_CHAT_CONFIG.temperature,
        top_p: DEFAULT_CHAT_CONFIG.top_p,
        top_k: DEFAULT_CHAT_CONFIG.top_k,
        custom_prompt: '',
        custom_system_prompt: '',
        other_model_params: {},
        use_tools: true,
        thinking: DEFAULT_CHAT_CONFIG.thinking,
        reasoning_effort: DEFAULT_CHAT_CONFIG.reasoning_effort,
        web_search_enabled: DEFAULT_CHAT_CONFIG.web_search_enabled,
      },
      llmProviderId: defaultAgentId,

      cronExpression: '',
      aiPrompt: '',
    },
  });

  // Thinking support detection
  const currentLLMProviderId = form.watch('llmProviderId');
  const thinkingConfig = useMemo(() => {
    if (!currentLLMProviderId || !llmProviders) {
      return {
        supportsThinking: false,
        forceEnabled: false,
        reasoningLevel: false,
      };
    }

    // Find the provider that matches the selected ID
    const selectedProvider = llmProviders.find(
      (provider) => provider.id === currentLLMProviderId,
    );
    if (!selectedProvider) {
      return {
        supportsThinking: false,
        forceEnabled: false,
        reasoningLevel: false,
      };
    }

    // Use the provider's model for thinking detection
    return getThinkingConfig(selectedProvider.model);
  }, [currentLLMProviderId, llmProviders]);

  // Check if thinking is enabled (either forced or manually enabled)
  const isThinkingEnabled =
    thinkingConfig.forceEnabled || form.watch('config.thinking');
  const shouldDisableSliders =
    thinkingConfig.supportsThinking && isThinkingEnabled;

  const handleResetAdvancedOptions = useCallback(() => {
    const existingConfig =
      form.getValues('config') ??
      ({
        custom_prompt: '',
        custom_system_prompt: '',
        other_model_params: {},
        stream: DEFAULT_CHAT_CONFIG.stream,
        temperature: DEFAULT_CHAT_CONFIG.temperature,
        top_p: DEFAULT_CHAT_CONFIG.top_p,
        top_k: DEFAULT_CHAT_CONFIG.top_k,
        use_tools: DEFAULT_CHAT_CONFIG.use_tools,
        thinking: thinkingConfig.forceEnabled
          ? true
          : DEFAULT_CHAT_CONFIG.thinking,
        reasoning_effort: DEFAULT_CHAT_CONFIG.reasoning_effort,
        web_search_enabled: DEFAULT_CHAT_CONFIG.web_search_enabled,
      } as NonNullable<AgentFormValues['config']>);

    const resetConfig: NonNullable<AgentFormValues['config']> = {
      ...existingConfig,
      stream: DEFAULT_CHAT_CONFIG.stream,
      use_tools: DEFAULT_CHAT_CONFIG.use_tools,
      web_search_enabled: DEFAULT_CHAT_CONFIG.web_search_enabled,
      thinking: thinkingConfig.forceEnabled
        ? true
        : DEFAULT_CHAT_CONFIG.thinking,
      reasoning_effort: DEFAULT_CHAT_CONFIG.reasoning_effort,
      temperature: DEFAULT_CHAT_CONFIG.temperature,
      top_p: DEFAULT_CHAT_CONFIG.top_p,
      top_k: DEFAULT_CHAT_CONFIG.top_k,
    };

    form.setValue('config', resetConfig, {
      shouldDirty: true,
      shouldTouch: true,
    });
  }, [form, thinkingConfig.forceEnabled]);

  // Effect to handle initial agent data and schedule type
  useEffect(() => {
    if (mode === 'edit' && agent) {
      form.setValue('name', agent.name);
      form.setValue('uiDescription', agent.ui_description);
      form.setValue('storage_path', agent.storage_path);
      form.setValue('knowledge', agent.knowledge);
      form.setValue('tools', agent.tools);
      form.setValue('tools_config_override', agent.tools_config_override ?? {});
      form.setValue('debugMode', agent.debug_mode);
      form.setValue('config', {
        custom_prompt: agent.config?.custom_prompt ?? '',
        custom_system_prompt: agent.config?.custom_system_prompt ?? '',
        temperature:
          agent.config?.temperature ?? DEFAULT_CHAT_CONFIG.temperature,
        top_k: agent.config?.top_k ?? DEFAULT_CHAT_CONFIG.top_k,
        top_p: agent.config?.top_p ?? DEFAULT_CHAT_CONFIG.top_p,
        use_tools: agent.config?.use_tools ?? true,
        thinking: agent.config?.thinking ?? DEFAULT_CHAT_CONFIG.thinking,
        reasoning_effort:
          agent.config?.reasoning_effort ??
          DEFAULT_CHAT_CONFIG.reasoning_effort,
        web_search_enabled:
          agent.config?.web_search_enabled ??
          DEFAULT_CHAT_CONFIG.web_search_enabled,
        stream: agent.config?.stream ?? DEFAULT_CHAT_CONFIG.stream,
        other_model_params: agent.config?.other_model_params ?? {},
      });
      form.setValue('llmProviderId', agent.llm_provider_id);

      // Set schedule type and cron expression based on existing tasks
      if (agent.cron_tasks && agent.cron_tasks.length > 0) {
        setScheduleType('scheduled');
        // Assuming only one cron task per agent for this form
        form.setValue('cronExpression', agent.cron_tasks[0]?.cron ?? '');

        if (
          agent.cron_tasks[0]?.action &&
          'CreateJobWithConfigAndMessage' in agent.cron_tasks[0].action
        ) {
          form.setValue(
            'aiPrompt',
            agent.cron_tasks[0].action.CreateJobWithConfigAndMessage.message
              .content ?? '',
          );
        }
      } else {
        setScheduleType('normal');
        form.setValue('cronExpression', '');
        form.setValue('aiPrompt', '');
      }

      // Set selected files and folders
      if (
        agent.scope?.vector_fs_items?.length ||
        agent.scope?.vector_fs_folders?.length
      ) {
        const selectedVRFilesPathMap = agent.scope.vector_fs_items.reduce<
          Record<string, { checked: boolean }>
        >((acc: Record<string, { checked: boolean }>, filePath: string) => {
          acc[filePath] = {
            checked: true,
          };
          // Also populate the selectedFileKeysRef
          selectedFileKeysRef.set(String(filePath), filePath);
          return acc;
        }, {});

        const selectedVRFoldersPathMap = agent.scope.vector_fs_folders.reduce<
          Record<string, { checked: boolean }>
        >((acc: Record<string, { checked: boolean }>, folderPath: string) => {
          acc[folderPath] = {
            checked: true,
          };
          // Also populate the selectedFolderKeysRef
          selectedFolderKeysRef.set(String(folderPath), folderPath);
          return acc;
        }, {});

        onSelectedKeysChange({
          ...selectedVRFilesPathMap,
          ...selectedVRFoldersPathMap,
        });
      }
    }
  }, [
    agent,
    form,
    mode,
    onSelectedKeysChange,
    selectedFileKeysRef,
    selectedFolderKeysRef,
  ]);

  useEffect(() => {
    if (mode === 'edit' && isOpenQueryActive) {
      setIsSideChatOpen(true);
    }
  }, [mode, isOpenQueryActive]);

  // Auto-enable thinking if force enabled for the model
  useEffect(() => {
    if (thinkingConfig.forceEnabled) {
      form.setValue('config.thinking', true);
    }
  }, [thinkingConfig.forceEnabled, form]);

  const { data: toolsList } = useGetTools({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
  });

  const { mutateAsync: createAgent, isPending: isCreating } = useCreateAgent({
    onError: (error) => {
      toast.error('Failed to create agent', {
        description: error.response?.data?.message ?? error.message,
      });
    },
  });

  const { mutateAsync: updateAgent, isPending: isUpdating } = useUpdateAgent({
    onError: (error) => {
      toast.error('Failed to update agent core data', {
        description: error.response?.data?.message ?? error.message,
      });
    },
  });

  // Create a separate mutation for quick save without navigation
  const { mutateAsync: quickSaveAgentMutation, isPending: isQuickSavePending } =
    useUpdateAgent({
      onError: (error) => {
        toast.error('Failed to update agent', {
          description: error.response?.data?.message ?? error.message,
        });
      },
      onSuccess: () => {
        toast.success('Agent updated successfully');
      },
    });

  const quickSaveAgent = async () => {
    if (!agent || !auth) return;

    try {
      const values = form.getValues();
      // Calculate cron validity for quick save
      const currentCronExpressionValue = values.cronExpression;
      let isCronValidForQuickSave = false;
      if (scheduleType === 'scheduled' && currentCronExpressionValue) {
        try {
          const readable = cronstrue.toString(currentCronExpressionValue, {
            throwExceptionOnParseError: true,
          });
          isCronValidForQuickSave = !readable.toLowerCase().includes('error');
        } catch (e) {
          console.error('Error parsing cron expression', e);
          isCronValidForQuickSave = false;
        }
      } else if (scheduleType === 'normal') {
        isCronValidForQuickSave = true; // It's valid to save in normal mode
      }

      const agentData = {
        agent_id: agent.agent_id,
        full_identity_name: agent.full_identity_name,
        llm_provider_id: values.llmProviderId,
        ui_description: values.uiDescription,
        storage_path: values.storage_path,
        knowledge: values.knowledge,
        tools: values.tools,
        debug_mode: values.debugMode,
        config: values.config,
        tools_config_override: values.tools_config_override || {},
        name: values.name,
        scope: {
          vector_fs_items: Array.from(selectedFileKeysRef.values()),
          vector_fs_folders: Array.from(selectedFolderKeysRef.values()),
          vector_search_mode: 'FillUpTo25k',
        },
      };
      // Call the update mutation WITHOUT cronExpression
      await quickSaveAgentMutation({
        nodeAddress: auth.node_address,
        token: auth.api_v2_key,
        agent: agentData,
      });

      const existingTask = agent.cron_tasks?.[0];
      const desiredCron =
        scheduleType === 'scheduled' ? values.cronExpression : undefined;

      if (desiredCron && isCronValidForQuickSave) {
        if (!existingTask) {
          await createTask({
            nodeAddress: auth.node_address,
            token: auth.api_v2_key,
            name: agent.agent_id,
            description: values.uiDescription || agent.agent_id,
            llmProvider: agent.agent_id,
            cronExpression: desiredCron,
            chatConfig: {
              custom_prompt: values.config?.custom_prompt ?? '',
              temperature: values.config?.temperature,
              top_p: values.config?.top_p,
              top_k: values.config?.top_k,
              use_tools: values.tools.length > 0,
              thinking: values.config?.thinking ?? DEFAULT_CHAT_CONFIG.thinking,
              reasoning_effort:
                values.config?.reasoning_effort ??
                DEFAULT_CHAT_CONFIG.reasoning_effort,
              web_search_enabled:
                values.config?.web_search_enabled ??
                DEFAULT_CHAT_CONFIG.web_search_enabled,
              stream: true,
            },
            message: values.aiPrompt || values.uiDescription || 'Scheduled run',
            toolKey: '',
          });
        } else if (existingTask.cron !== desiredCron) {
          // Use correct parameters for removeTask
          await removeTask({
            nodeAddress: auth.node_address,
            token: auth.api_v2_key,
            recurringTaskId: existingTask.task_id.toString(),
          });
          await createTask({
            nodeAddress: auth.node_address,
            token: auth.api_v2_key,
            name: agent.agent_id,
            description: values.uiDescription || agent.agent_id,
            llmProvider: agent.agent_id,
            cronExpression: desiredCron,
            chatConfig: {
              custom_prompt: values.config?.custom_prompt ?? '',
              temperature: values.config?.temperature,
              top_p: values.config?.top_p,
              top_k: values.config?.top_k,
              use_tools: values.tools.length > 0,
              thinking: values.config?.thinking ?? DEFAULT_CHAT_CONFIG.thinking,
              reasoning_effort:
                values.config?.reasoning_effort ??
                DEFAULT_CHAT_CONFIG.reasoning_effort,
              web_search_enabled:
                values.config?.web_search_enabled ??
                DEFAULT_CHAT_CONFIG.web_search_enabled,
              stream: true,
            },
            message: values.aiPrompt || values.uiDescription || 'Scheduled run',
            toolKey: '',
          });
        }
      } else if (!desiredCron && existingTask) {
        // Use correct parameters for removeTask
        await removeTask({
          nodeAddress: auth.node_address,
          token: auth.api_v2_key,
          recurringTaskId: existingTask.task_id.toString(),
        });
      } else if (desiredCron && !isCronValidForQuickSave) {
        // toast.error("Invalid Cron Expression. Quick save did not update schedule.");
      }
      // --- End Cron Handling ---
    } catch (error: any) {
      console.error('Quick save error:', error);
      if (!error.response?.data?.message) {
        toast.error('Failed to quick save agent schedule', {
          description: error.message || 'An unexpected error occurred.',
        });
      }
    }
  };

  const queryClient = useQueryClient();

  const { mutateAsync: removeTask, isPending: isRemovingTask } =
    useRemoveRecurringTask({
      onSuccess: () => {
        void queryClient.invalidateQueries({
          queryKey: [
            FunctionKeyV2.GET_AGENT,
            {
              agentId: agent?.agent_id,
              nodeAddress: auth?.node_address ?? '',
            },
          ],
        });
        // Clear cron expression in form after successful deletion
        form.setValue('cronExpression', '');
        setScheduleType('normal');
        toast.success('Delete task successfully');
      },
      onError: (error) => {
        toast.error('Failed remove task', {
          description: error.response?.data?.message ?? error.message,
        });
      },
    });

  const onDeleteTask = async (taskId: string) => {
    await removeTask({
      nodeAddress: auth?.node_address ?? '',
      token: auth?.api_v2_key ?? '',
      recurringTaskId: taskId,
    });
  };

  const { mutateAsync: createTask, isPending: isCreatingTask } =
    useCreateRecurringTask({
      onSuccess: () => {
        void queryClient.invalidateQueries({
          queryKey: [
            FunctionKeyV2.GET_AGENT,
            {
              agentId:
                agentId ??
                form
                  .getValues('name')
                  .replace(/[^a-zA-Z0-9_]/g, '_')
                  .toLowerCase(),
              nodeAddress: auth?.node_address ?? '',
            },
          ],
        });
        toast.success('Scheduled task created successfully');
      },
      onError: (error) => {
        toast.error('Failed to create scheduled task', {
          description: error.response?.data?.message ?? error.message,
        });
      },
    });

  const submit = async (
    values: AgentFormValues,
    options?: { openChat?: boolean },
  ) => {
    if (!auth) {
      toast.error('Authentication details are missing.');
      return;
    }

    // Use memoized readableCronExpression for validation
    if (
      scheduleType === 'scheduled' &&
      (!values.cronExpression || !readableCronExpression)
    ) {
      toast.error('Invalid or empty Cron Expression. Cannot save.');
      setCurrentTab('schedule');
      return;
    }

    const agentIdToUse =
      mode === 'edit' && agent
        ? agent.agent_id
        : values.name.replace(/[^a-zA-Z0-9_]/g, '_').toLowerCase();
    const agentData = {
      agent_id: agentIdToUse,
      full_identity_name: `${auth.shinkai_identity}/main/agent/${agentIdToUse}`,
      llm_provider_id: values.llmProviderId,
      ui_description: values.uiDescription,
      storage_path: values.storage_path,
      knowledge: values.knowledge,
      tools: values.tools,
      tools_config_override: values.tools_config_override || {},
      debug_mode: values.debugMode,
      config: values.config,
      name: values.name,
      scope: {
        vector_fs_items: Array.from(selectedFileKeysRef.values()),
        vector_fs_folders: Array.from(selectedFolderKeysRef.values()),
        vector_search_mode: 'FillUpTo25k',
      },
    };
    try {
      if (mode === 'edit' && agent) {
        // Step 1: Update Agent Core Data
        await updateAgent({
          nodeAddress: auth.node_address,
          token: auth.api_v2_key,
          agent: merge(agentData, {
            agent_id: agent.agent_id,
            full_identity_name: agent.full_identity_name,
          }),
        });

        // Step 2: Handle Cron Task Logic (Create/Update/Delete)
        const existingTask = agent.cron_tasks?.[0];
        const desiredCron =
          scheduleType === 'scheduled' ? values.cronExpression : undefined;

        if (desiredCron) {
          // User wants a schedule
          if (!existingTask) {
            // No existing task -> Create
            await createTask({
              nodeAddress: auth.node_address,
              token: auth.api_v2_key,
              name: agent.agent_id,
              description: values.uiDescription || agent.agent_id,
              llmProvider: agent.agent_id,
              cronExpression: desiredCron,
              chatConfig: {
                custom_prompt: values.config?.custom_prompt ?? '',
                temperature: values.config?.temperature,
                top_p: values.config?.top_p,
                top_k: values.config?.top_k,
                use_tools: values.tools.length > 0,
                thinking:
                  values.config?.thinking ?? DEFAULT_CHAT_CONFIG.thinking,
                reasoning_effort:
                  values.config?.reasoning_effort ??
                  DEFAULT_CHAT_CONFIG.reasoning_effort,
                web_search_enabled:
                  values.config?.web_search_enabled ??
                  DEFAULT_CHAT_CONFIG.web_search_enabled,
                stream: true,
              },
              message:
                values.aiPrompt || values.uiDescription || 'Scheduled run',
              toolKey: '',
            });
          } else if (existingTask.cron !== desiredCron) {
            // Existing task, cron changed -> Remove + Create
            await removeTask({
              nodeAddress: auth.node_address,
              token: auth.api_v2_key,
              recurringTaskId: existingTask.task_id.toString(),
            });
            await createTask({
              nodeAddress: auth.node_address,
              token: auth.api_v2_key,
              name: agent.agent_id,
              description: values.uiDescription || agent.agent_id,
              llmProvider: agent.agent_id,
              cronExpression: desiredCron,
              chatConfig: {
                custom_prompt: values.config?.custom_prompt ?? '',
                temperature: values.config?.temperature,
                top_p: values.config?.top_p,
                top_k: values.config?.top_k,
                use_tools: values.tools.length > 0,
                thinking:
                  values.config?.thinking ?? DEFAULT_CHAT_CONFIG.thinking,
                reasoning_effort:
                  values.config?.reasoning_effort ??
                  DEFAULT_CHAT_CONFIG.reasoning_effort,
                web_search_enabled:
                  values.config?.web_search_enabled ??
                  DEFAULT_CHAT_CONFIG.web_search_enabled,
                stream: true,
              },
              message:
                values.aiPrompt || values.uiDescription || 'Scheduled run',
              toolKey: '',
            });
          }
          // If desiredCron and existingTask.cron are the same, do nothing to the task
        } else if (!desiredCron && existingTask) {
          // User wants 'normal' mode, but task exists -> Remove
          await removeTask({
            nodeAddress: auth.node_address,
            token: auth.api_v2_key,
            recurringTaskId: existingTask.task_id.toString(),
          });
        }

        // --- End Cron Handling ---

        // Step 3: Show success and navigate ONLY if all above steps succeeded
        toast.success('Agent updated successfully!');
        await navigate('/agents');
      } else {
        // --- Create New Agent ---
        const cronToPass =
          scheduleType === 'scheduled' ? values.cronExpression : undefined;
        await createAgent(
          {
            // createAgent handles cron internally
            nodeAddress: auth.node_address,
            token: auth.api_v2_key,
            agent: agentData,
            cronExpression: cronToPass,
          },
          {
            onSuccess: async () => {
              toast.success('Agent created successfully!');
              captureAnalyticEvent('Agent Created', undefined);

              if (options?.openChat) {
                await navigate(`/agents/edit/${agentIdToUse}?openChat=true`);
              } else {
                await navigate('/agents');
              }
            },
          },
        );
      }
    } catch (error: any) {
      // Catch errors from ANY await above
      console.error('Submit error:', error);
      // Avoid redundant toasts if mutation's onError already fired
      if (!error.response?.data?.message) {
        // Provide a generic error if the specific mutation didn't toast
        toast.error('Failed to save agent settings', {
          description: error.message || 'An unexpected error occurred.',
        });
      }
      // Do NOT navigate on error
    }
  };

  const isPending =
    mode === 'edit'
      ? isUpdating || isCreatingTask || isRemovingTask
      : isCreating || isCreatingTask;

  const currentCronExpression = form.watch('cronExpression');

  const readableCronExpression = useMemo(() => {
    if (!currentCronExpression) {
      return null;
    }
    try {
      const readableCron = cronstrue.toString(currentCronExpression, {
        throwExceptionOnParseError: true, // Ensure errors are caught
      });
      // Check for error strings just in case throwException doesn't catch everything
      if (readableCron.toLowerCase().includes('error')) {
        return null;
      }
      return readableCron;
    } catch (e) {
      console.error('Error parsing cron expression', e);
      return null; // Invalid cron expression
    }
  }, [currentCronExpression]);

  // --- Helper function to find a TreeNode by its key (path) in the tree ---
  const findNodeByKey = (
    key: string,
    searchNodes: TreeNode[],
  ): TreeNode | null => {
    for (const node of searchNodes) {
      if (String(node.key) === key) return node;
      if (node.children) {
        const found = findNodeByKey(key, node.children);
        if (found) return found;
      }
    }
    return null;
  };

  const toolsRequiringConfig = useMemo(() => {
    const getToolsRequiringConfig = (tools: ShinkaiToolHeader[]) => {
      return tools.filter((tool) =>
        getToolRequiresConfigurations(tool?.config ?? []),
      );
    };
    const agentTools = new Set(agent?.tools);
    const tools =
      toolsList?.filter((tool) => agentTools.has(tool.tool_router_key)) ?? [];
    return getToolsRequiringConfig(tools);
  }, [agent?.tools, toolsList]);

  const hasToolsRequiringConfigurations = useMemo(() => {
    return toolsRequiringConfig.length > 0;
  }, [toolsRequiringConfig]);

  return (
    <ResizablePanelGroup
      className="relative h-full min-h-0"
      direction="horizontal"
    >
      <ResizablePanel
        className="flex h-full min-h-0 flex-1"
        defaultSize={70}
        minSize={50}
      >
        <div
          className={cn(
            'flex h-full min-h-0 w-full flex-col',
            isWorkflowTab ? 'px-6' : 'container max-w-3xl',
          )}
        >
          <div className="flex items-center justify-between pt-10 pb-6">
            <div className="flex items-center gap-5">
              <Breadcrumb>
                <BreadcrumbList>
                  <BreadcrumbItem>
                    <BreadcrumbLink asChild>
                      <Link to="/agents">Agents</Link>
                    </BreadcrumbLink>
                  </BreadcrumbItem>
                  <BreadcrumbSeparator />
                  <BreadcrumbItem>
                    <span className="text-text-default">
                      {agent?.name || t('agents.form.createAgent')}
                    </span>
                  </BreadcrumbItem>
                </BreadcrumbList>
              </Breadcrumb>
              {/* <h1 className="font-clash text-2xl font-medium">
                {mode === 'edit'
                  ? t('agents.form.updateAgent')
                  : t('agents.form.createAgent')}
              </h1> */}
            </div>
            {mode === 'edit' && agent && (
              <div className="flex gap-2">
                <Button
                  className="flex items-center gap-2"
                  isLoading={isQuickSavePending}
                  onClick={quickSaveAgent}
                  size="sm"
                  variant="outline"
                >
                  <svg
                    className="h-4 w-4"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="2"
                    viewBox="0 0 24 24"
                    xmlns="http://www.w3.org/2000/svg"
                  >
                    <path
                      d="M8 7H5a2 2 0 00-2 2v9a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-3m-1 4l-3 3m0 0l-3-3m3 3V4"
                      strokeLinecap="round"
                      strokeLinejoin="round"
                    />
                  </svg>
                  {t('common.save')}
                </Button>
                <Button
                  className="flex items-center gap-2"
                  onClick={() => setIsSideChatOpen(!isSideChatOpen)}
                  size="sm"
                  variant="outline"
                >
                  <MessageSquare className="h-4 w-4" />
                  {isSideChatOpen
                    ? t('agents.form.closeChat')
                    : t('agents.form.openChat')}
                </Button>
              </div>
            )}
          </div>

          <Form {...form}>
            <form
              className="flex min-h-0 w-full flex-1 flex-col justify-between space-y-2"
              // Pass values directly, options are handled inside submit
              onSubmit={form.handleSubmit((values) => submit(values))}
            >
              <div className="mx-auto min-h-0 w-full flex-1 overflow-hidden">
                <div className="h-full min-h-0 space-y-6 overflow-hidden">
                  <Tabs
                    className="flex h-full min-h-0 flex-col gap-4"
                    defaultValue={defaultTabValue}
                    onValueChange={(value) => {
                      if (isTabValue(value)) {
                        setCurrentTab(value);
                      }
                    }}
                    value={currentTab}
                  >
                    <TabNavigation />

                    <TabsContent
                      className="min-h-0 flex-1 overflow-y-auto"
                      value="persona"
                    >
                      <div className="h-full min-h-0 space-y-8">
                        <FormField
                          control={form.control}
                          name="name"
                          render={({ field }) => (
                            <TextField
                              autoFocus
                              field={field}
                              helperMessage={
                                mode === 'edit'
                                  ? t('agents.create.agentNameHelperEdit', {
                                      agentName: agent?.agent_id,
                                    })
                                  : t('agents.create.agentNameHelperCreate', {
                                      agentName: form.watch('name'),
                                    })
                              }
                              label={t('agents.create.agentName')}
                            />
                          )}
                        />

                        <FormField
                          control={form.control}
                          name="uiDescription"
                          render={({ field }) => (
                            <FormItem>
                              <FormLabel>
                                {t('agents.create.description')}
                              </FormLabel>

                              <FormControl>
                                <Textarea
                                  className="!min-h-[100px] text-sm"
                                  onChange={field.onChange}
                                  placeholder={t(
                                    'agents.create.descriptionPlaceholder',
                                  )}
                                  spellCheck={false}
                                  value={field.value}
                                />
                              </FormControl>
                              <FormDescription>
                                {t('agents.create.descriptionHelper')}
                              </FormDescription>
                              <FormMessage />
                            </FormItem>
                          )}
                        />

                        <FormField
                          control={form.control}
                          name="config.custom_system_prompt"
                          render={({ field }) => (
                            <FormItem>
                              <FormLabel>
                                {t('agents.create.systemInstructions')}
                              </FormLabel>
                              <FormControl>
                                <Textarea
                                  className="!max-h-[auto] !min-h-[300px] text-sm"
                                  placeholder={t(
                                    'agents.create.systemInstructionsPlaceholder',
                                  )}
                                  resize="vertical"
                                  spellCheck={false}
                                  {...field}
                                />
                              </FormControl>
                              <FormDescription>
                                {t('agents.create.systemInstructionsHelper')}
                              </FormDescription>
                            </FormItem>
                          )}
                        />

                        <FormField
                          control={form.control}
                          name="llmProviderId"
                          render={({ field }) => (
                            <div className="space-y-2">
                              <p className="text-text-default text-sm">
                                {t('agents.create.llmProviderLabel')}
                              </p>
                              <span className="text-text-secondary text-xs">
                                {t('agents.create.llmProviderDescription')}
                              </span>
                              <AIModelSelector
                                className="bg-bg-secondary hover:bg-bg-quaternary !border-divider !h-auto w-full rounded-lg border py-2.5"
                                onValueChange={field.onChange}
                                value={field.value}
                              />
                            </div>
                          )}
                        />

                        <Collapsible>
                          <CollapsibleTrigger
                            className={cn(
                              'text-text-secondary hover:text-text-default flex items-center gap-1 text-sm',
                              '[&[data-state=open]>svg]:rotate-90',
                              '[&[data-state=open]>span.input]:block',
                              '[&[data-state=open]>span.content]:hidden',
                            )}
                          >
                            {t('agents.create.contextSettingsLabel')}
                            <ChevronRight className="ml-1 size-4" />
                          </CollapsibleTrigger>
                          <CollapsibleContent>
                            <div className="space-y-4 py-6">
                              <FormField
                                control={form.control}
                                name="config.custom_prompt"
                                render={({ field }) => (
                                  <FormItem>
                                    <FormLabel>
                                      {t('agents.create.messageContextLabel')}
                                    </FormLabel>
                                    <FormControl>
                                      <Textarea
                                        className="!min-h-[100px] text-sm"
                                        placeholder="e.g., {{user_message}}. And then say: Chao amigo!"
                                        spellCheck={false}
                                        {...field}
                                      />
                                    </FormControl>
                                    <FormDescription>
                                      {t(
                                        'agents.create.messageContextDescription',
                                      )}
                                    </FormDescription>
                                  </FormItem>
                                )}
                              />
                            </div>
                          </CollapsibleContent>
                        </Collapsible>

                        <Collapsible>
                          <CollapsibleTrigger
                            className={cn(
                              'text-text-secondary hover:text-text-default flex items-center gap-1 text-sm',
                              '[&[data-state=open]>svg]:rotate-90',
                              '[&[data-state=open]>span.input]:block',
                              '[&[data-state=open]>span.content]:hidden',
                            )}
                          >
                            {t('agents.create.advanceOptionsLabel')}
                            <ChevronRight className="ml-1 size-4" />
                          </CollapsibleTrigger>
                          <CollapsibleContent>
                            <div className="space-y-4 py-6 pr-2">
                              <FormField
                                control={form.control}
                                name="config.stream"
                                render={({ field }) => (
                                  <FormItem className="flex w-full flex-col gap-3">
                                    <div className="flex justify-between gap-3">
                                      <div className="space-y-1 leading-none">
                                        <FormLabel className="text-text-default static space-y-1.5 text-sm">
                                          {t('agents.create.enableStream')}
                                        </FormLabel>
                                        <p className="text-text-secondary text-xs">
                                          {t(
                                            'agents.create.enableStreamDescription',
                                          )}
                                        </p>
                                      </div>
                                      <FormControl>
                                        <Switch
                                          checked={field.value}
                                          onCheckedChange={field.onChange}
                                        />
                                      </FormControl>
                                    </div>
                                  </FormItem>
                                )}
                              />
                              <FormField
                                control={form.control}
                                name="config.use_tools"
                                render={({ field }) => (
                                  <FormItem className="flex w-full flex-col gap-3">
                                    <div className="flex justify-between gap-3">
                                      <div className="space-y-1 leading-none">
                                        <FormLabel className="text-text-default static space-y-1.5 text-sm">
                                          {t('agents.create.enableTools')}
                                        </FormLabel>
                                        <p className="text-text-secondary text-xs">
                                          {t(
                                            'agents.create.enableToolsDescription',
                                          )}
                                        </p>
                                      </div>
                                      <FormControl>
                                        <Switch
                                          checked={field.value}
                                          onCheckedChange={field.onChange}
                                        />
                                      </FormControl>
                                    </div>
                                  </FormItem>
                                )}
                              />
                              <FormField
                                control={form.control}
                                name="config.web_search_enabled"
                                render={({ field }) => (
                                  <FormItem className="flex w-full flex-col gap-3">
                                    <div className="flex justify-between gap-3">
                                      <div className="space-y-1 leading-none">
                                        <FormLabel className="text-text-default static space-y-1.5 text-sm">
                                          Enable Web Search
                                        </FormLabel>
                                        <p className="text-text-secondary text-xs">
                                          Allows the agent to search the web for
                                          real-time information
                                        </p>
                                      </div>
                                      <FormControl>
                                        <Switch
                                          checked={field.value ?? false}
                                          disabled={
                                            !form.watch('config.use_tools')
                                          }
                                          onCheckedChange={field.onChange}
                                        />
                                      </FormControl>
                                    </div>
                                  </FormItem>
                                )}
                              />
                              {thinkingConfig.supportsThinking && (
                                <FormField
                                  control={form.control}
                                  name="config.thinking"
                                  render={({ field }) => (
                                    <FormItem className="flex w-full flex-col gap-3">
                                      <div className="flex justify-between gap-3">
                                        <div className="space-y-1 leading-none">
                                          <FormLabel className="text-text-default static space-y-1.5 text-sm">
                                            Enable Thinking
                                          </FormLabel>
                                          <p className="text-text-secondary text-xs">
                                            Enables step-by-step reasoning for
                                            better responses
                                          </p>
                                        </div>
                                        <FormControl>
                                          <Switch
                                            checked={field.value ?? false}
                                            disabled={
                                              thinkingConfig.forceEnabled
                                            }
                                            onCheckedChange={field.onChange}
                                          />
                                        </FormControl>
                                      </div>
                                    </FormItem>
                                  )}
                                />
                              )}
                              {thinkingConfig.reasoningLevel &&
                                isThinkingEnabled && (
                                  <FormField
                                    control={form.control}
                                    name="config.reasoning_effort"
                                    render={({ field }) => (
                                      <FormItem className="flex w-full flex-col gap-3">
                                        <div className="flex justify-between gap-3">
                                          <div className="space-y-1 leading-none">
                                            <FormLabel className="text-text-default static space-y-1.5 text-sm">
                                              Reasoning Effort
                                            </FormLabel>
                                            <p className="text-text-secondary text-xs">
                                              Controls how much effort the model
                                              puts into reasoning
                                            </p>
                                          </div>
                                          <FormControl>
                                            <Select
                                              value={field.value}
                                              onValueChange={field.onChange}
                                            >
                                              <SelectTrigger className="!h-10 w-[120px] py-2 pr-10 [&>svg]:!top-2.5">
                                                <SelectValue placeholder="Select effort" />
                                              </SelectTrigger>
                                              <SelectContent>
                                                <SelectItem value="low">
                                                  Low
                                                </SelectItem>
                                                <SelectItem value="medium">
                                                  Medium
                                                </SelectItem>
                                                <SelectItem value="high">
                                                  High
                                                </SelectItem>
                                              </SelectContent>
                                            </Select>
                                          </FormControl>
                                        </div>
                                      </FormItem>
                                    )}
                                  />
                                )}
                              {!shouldDisableSliders && (
                                <FormField
                                  control={form.control}
                                  name="config.temperature"
                                  render={({ field }) => (
                                    <FormItem className="flex gap-2.5">
                                      <FormControl>
                                        <HoverCard openDelay={200}>
                                          <HoverCardTrigger asChild>
                                            <div className="grid w-full gap-4">
                                              <div className="flex items-center justify-between">
                                                <Label htmlFor="temperature">
                                                  {t(
                                                    'agents.create.temperature',
                                                  )}
                                                </Label>
                                                <span className="text-text-secondary hover:border-border w-12 rounded-md border border-transparent px-2 py-0.5 text-right text-sm">
                                                  {field.value}
                                                </span>
                                              </div>
                                              <Slider
                                                aria-label="Temperature"
                                                className="[&_[role=slider]]:h-4 [&_[role=slider]]:w-4"
                                                id="temperature"
                                                max={1}
                                                onValueChange={(vals) => {
                                                  field.onChange(vals[0]);
                                                }}
                                                step={0.1}
                                                value={[field.value]}
                                              />
                                            </div>
                                          </HoverCardTrigger>
                                          <HoverCardContent
                                            align="start"
                                            className="w-[260px] px-2 py-3 text-xs"
                                            side="left"
                                          >
                                            {t(
                                              'agents.create.temperatureDescription',
                                            )}
                                          </HoverCardContent>
                                        </HoverCard>
                                      </FormControl>
                                    </FormItem>
                                  )}
                                />
                              )}
                              {!shouldDisableSliders && (
                                <FormField
                                  control={form.control}
                                  name="config.top_p"
                                  render={({ field }) => (
                                    <FormItem className="flex gap-2.5">
                                      <FormControl>
                                        <HoverCard openDelay={200}>
                                          <HoverCardTrigger asChild>
                                            <div className="grid w-full gap-4">
                                              <div className="flex items-center justify-between">
                                                <Label htmlFor="topP">
                                                  Top P
                                                </Label>
                                                <span className="text-text-secondary hover:border-border w-12 rounded-md border border-transparent px-2 py-0.5 text-right text-sm">
                                                  {field.value}
                                                </span>
                                              </div>
                                              <Slider
                                                aria-label="Top P"
                                                className="[&_[role=slider]]:h-4 [&_[role=slider]]:w-4"
                                                id="topP"
                                                max={1}
                                                min={0}
                                                onValueChange={(vals) => {
                                                  field.onChange(vals[0]);
                                                }}
                                                step={0.1}
                                                value={[field.value]}
                                              />
                                            </div>
                                          </HoverCardTrigger>
                                          <HoverCardContent
                                            align="start"
                                            className="w-[260px] px-2 py-3 text-xs"
                                            side="left"
                                          >
                                            {t('agents.create.topPDescription')}
                                          </HoverCardContent>
                                        </HoverCard>
                                      </FormControl>
                                    </FormItem>
                                  )}
                                />
                              )}
                              {!shouldDisableSliders && (
                                <FormField
                                  control={form.control}
                                  name="config.top_k"
                                  render={({ field }) => (
                                    <FormItem className="flex gap-2.5">
                                      <FormControl>
                                        <HoverCard openDelay={200}>
                                          <HoverCardTrigger asChild>
                                            <div className="grid w-full gap-4">
                                              <div className="flex items-center justify-between">
                                                <Label htmlFor="topK">
                                                  {t('agents.create.topK')}
                                                </Label>
                                                <span className="text-text-secondary hover:border-border w-12 rounded-md border border-transparent px-2 py-0.5 text-right text-sm">
                                                  {field.value}
                                                </span>
                                              </div>
                                              <Slider
                                                aria-label="Top K"
                                                className="[&_[role=slider]]:h-4 [&_[role=slider]]:w-4"
                                                id="topK"
                                                max={100}
                                                onValueChange={(vals) => {
                                                  field.onChange(vals[0]);
                                                }}
                                                step={1}
                                                value={[field.value]}
                                              />
                                            </div>
                                          </HoverCardTrigger>
                                          <HoverCardContent
                                            align="start"
                                            className="w-[260px] px-2 py-3 text-xs"
                                            side="left"
                                          >
                                            {t('agents.create.topKDescription')}
                                          </HoverCardContent>
                                        </HoverCard>
                                      </FormControl>
                                    </FormItem>
                                  )}
                                />
                              )}
                              <div className="flex justify-start">
                                <Button
                                  className="text-text-secondary"
                                  onClick={handleResetAdvancedOptions}
                                  size="sm"
                                  type="button"
                                  variant="tertiary"
                                >
                                  {t('agents.create.resetAdvancedOptions')}
                                </Button>
                              </div>
                            </div>
                          </CollapsibleContent>
                        </Collapsible>
                      </div>
                    </TabsContent>
                    <TabsContent value="workflow">
                      <div className="space-y-4">
                        <div className="space-y-1">
                          <h2 className="inline-flex items-center gap-2 text-base font-medium">
                            Workflow
                          </h2>
                        </div>
                      </div>
                    </TabsContent>

                    <TabsContent
                      className="min-h-0 flex-1 overflow-y-auto"
                      value="knowledge"
                    >
                      <div className="space-y-4">
                        <div className="space-y-1">
                          <h2 className="inline-flex items-center gap-2 text-base font-medium">
                            {t('agents.create.knowledgeBase')}
                            {Object.keys(selectedKeys ?? {}).length > 0 && (
                              <Badge className="bg-bg-quaternary border-divider text-text-tertiary inline-flex size-6 items-center justify-center rounded-full p-0 text-center text-sm">
                                {Object.keys(selectedKeys ?? {}).length}
                              </Badge>
                            )}
                          </h2>
                          <p className="text-text-secondary text-sm">
                            {t('agents.create.knowledgeBaseDescription')}
                          </p>
                        </div>

                        <div className="flex items-center justify-between gap-4 pl-0.5">
                          <SearchInput
                            classNames={{
                              container: 'mx-0.5 h-9 ',
                              input: 'bg-transparent',
                            }}
                            onChange={(e) => {
                              setSearchQueryKnowledge(e.target.value);
                            }}
                            placeholder={t('agents.create.knowledgeSearch')}
                            value={searchQueryKnowledge}
                          />

                          <Button
                            className="shrink-0"
                            onClick={() => {
                              setIsAddFilesDialogOpen(true);
                            }}
                            size="sm"
                            type="button"
                            variant="outline"
                          >
                            <PlusIcon className="mr-1 size-4" />
                            {t('agents.create.knowledgeAddNewFiles')}
                          </Button>
                        </div>
                        <div className="flex max-h-[calc(100vh-200px)] flex-1 flex-col gap-2">
                          {searchQueryKnowledge &&
                            isSearchQueryKnowledgeSynced &&
                            searchKnowledgeList?.length === 0 && (
                              <div className="text-text-secondary flex h-20 items-center justify-center">
                                {t('vectorFs.emptyState.noFiles')}
                              </div>
                            )}
                          {searchQueryKnowledge &&
                            isSearchQueryKnowledgeSynced &&
                            searchKnowledgeList?.map((file) => (
                              <div
                                className="flex items-center gap-2"
                                key={file.path}
                              >
                                <Checkbox
                                  checked={
                                    selectedKeys?.[file.path]?.checked ?? false
                                  }
                                  onCheckedChange={(checked) => {
                                    const newKeys = { ...selectedKeys };
                                    if (checked) {
                                      newKeys[file.path] = { checked: true };
                                      selectedFileKeysRef.set(
                                        String(file.path),
                                        file.path,
                                      );
                                    } else {
                                      delete newKeys[file.path];
                                      selectedFileKeysRef.delete(
                                        String(file.path),
                                      );
                                    }
                                    onSelectedKeysChange(newKeys);
                                  }}
                                />
                                <label
                                  className="flex flex-1 items-center gap-3"
                                  htmlFor={`item-${file.path}`}
                                >
                                  {file.is_directory ? (
                                    <DirectoryTypeIcon className="size-4" />
                                  ) : (
                                    <FileTypeIcon
                                      type={getFileExt(file.name)}
                                    />
                                  )}
                                  <span className="text-text-default text-sm">
                                    {file.path}
                                  </span>
                                </label>
                              </div>
                            ))}

                          {isSearchKnowledgeListPending &&
                            searchQueryKnowledge &&
                            searchQueryKnowledge !==
                              debouncedSearchQueryKnowledge &&
                            Array.from({ length: 4 }).map((_, idx) => (
                              <Skeleton
                                className="h-[30px] animate-pulse rounded"
                                key={idx}
                              />
                            ))}
                          {!searchQueryKnowledge && (
                            <Tree
                              onSelect={(e) => {
                                if (e.node.icon === 'icon-folder') {
                                  selectedFolderKeysRef.set(
                                    String(e.node.key),
                                    e.node.data.path,
                                  );
                                  return;
                                }
                                selectedFileKeysRef.set(
                                  String(e.node.key),
                                  e.node.data.path,
                                );
                              }}
                              onSelectionChange={(e) => {
                                onSelectedKeysChange(
                                  e.value as TreeCheckboxSelectionKeys,
                                );
                              }}
                              onUnselect={(e) => {
                                const nodeKey = String(e.node.key);
                                if (e.node.icon === 'icon-folder') {
                                  selectedFolderKeysRef.delete(nodeKey);
                                  const clearDescendants = (node: TreeNode) => {
                                    node.children?.forEach(
                                      (child: TreeNode) => {
                                        const childKey = String(child.key);
                                        selectedFileKeysRef.delete(childKey);
                                        selectedFolderKeysRef.delete(childKey); // Remove if it exists as a folder key
                                        if (
                                          child.children &&
                                          child.children.length > 0
                                        ) {
                                          clearDescendants(child);
                                        }
                                      },
                                    );
                                  };
                                  clearDescendants(e.node);
                                } else {
                                  // --- File Deselected ---
                                  const lastSlashIndex =
                                    nodeKey.lastIndexOf('/');
                                  const parentFolderPath =
                                    lastSlashIndex > 0
                                      ? nodeKey.substring(0, lastSlashIndex)
                                      : '/';
                                  const isParentFolderSelected =
                                    selectedFolderKeysRef.has(parentFolderPath);
                                  if (isParentFolderSelected) {
                                    selectedFolderKeysRef.delete(
                                      parentFolderPath,
                                    );
                                    const parentNode = findNodeByKey(
                                      parentFolderPath,
                                      nodes,
                                    );
                                    (parentNode?.children ?? [])
                                      .filter(
                                        (childNode: TreeNode) =>
                                          String(childNode.key) !== nodeKey &&
                                          childNode.icon !== 'icon-folder',
                                      )
                                      .forEach((childNode: TreeNode) => {
                                        if (childNode.data?.path) {
                                          selectedFileKeysRef.set(
                                            String(childNode.key),
                                            childNode.data.path,
                                          );
                                        }
                                      });
                                  } else {
                                    selectedFileKeysRef.delete(nodeKey);
                                  }
                                }
                              }}
                              propagateSelectionDown={true}
                              propagateSelectionUp={true}
                              pt={treeOptions}
                              selectionKeys={selectedKeys}
                              selectionMode="checkbox"
                              value={nodes}
                            />
                          )}
                        </div>
                      </div>
                    </TabsContent>

                    <TabsContent
                      className="min-h-0 flex-1 overflow-y-auto"
                      value="tools"
                    >
                      <div className="h-full min-h-0 space-y-6 overflow-y-auto">
                        {hasToolsRequiringConfigurations && (
                          <div className="mb-4 rounded-lg border border-amber-500/30 bg-amber-950/10 p-3">
                            <div className="mb-2 flex items-center gap-2">
                              <AlertTriangleIcon className="h-4 w-4 text-amber-500" />
                              <h3 className="text-sm font-medium text-amber-400">
                                Configuration Required
                              </h3>
                            </div>
                            <p className="mb-2 text-xs text-amber-300">
                              Please configure the following tools before using
                              this agent:
                            </p>
                            <div className="grid gap-1">
                              {toolsRequiringConfig.map((tool) => (
                                <div
                                  className="flex items-center justify-between py-1"
                                  key={tool.tool_router_key}
                                >
                                  <span className="text-sm">{tool.name}</span>
                                  <Button
                                    className={cn(
                                      buttonVariants({
                                        variant: 'outline',
                                        size: 'xs',
                                      }),
                                    )}
                                    onClick={() => {
                                      setSelectedToolConfig(
                                        tool.tool_router_key,
                                      );
                                      window.location.hash = '#configuration';
                                    }}
                                    type="button"
                                  >
                                    <BoltIcon className="mr-1 size-3" />
                                    Configure
                                  </Button>
                                </div>
                              ))}
                            </div>
                          </div>
                        )}
                        <div className="flex items-center justify-between gap-2">
                          <div className="space-y-1">
                            <h2 className="text-base font-medium">
                              {t('agents.create.tools')}
                            </h2>
                            <p className="text-text-secondary text-sm">
                              {t('agents.create.toolsDescription')}
                            </p>
                          </div>
                          <Button
                            onClick={() => {
                              void navigate('/tools');
                            }}
                            size="xs"
                            variant="outline"
                          >
                            <PlusIcon className="mr-1 size-3.5" />
                            {t('agents.create.toolsCreateNewTool')}
                          </Button>
                        </div>

                        {form.watch('tools')?.length > 0 && (
                          <div className="bg-bg-secondary mr-2 rounded-lg p-3">
                            <div className="flex items-center justify-between">
                              <h3 className="text-text-secondary mb-2 text-xs font-medium tracking-wide uppercase">
                                {t('agents.create.selectedTools')}
                              </h3>
                              <Button
                                className="text-text-secondary text-xs"
                                onClick={() => {
                                  form.setValue('tools', []);
                                  form.setValue('tools_config_override', {});
                                }}
                                size="xs"
                                variant="tertiary"
                              >
                                {t('common.clear')}
                              </Button>
                            </div>
                            <div className="divide-divider divide-y">
                              {form.watch('tools').map((toolKey) => {
                                const tool = toolsList?.find(
                                  (t) => t.tool_router_key === toolKey,
                                );
                                if (!tool) return null;
                                return (
                                  <div
                                    className="flex flex-col gap-2"
                                    key={toolKey}
                                  >
                                    <div className="flex w-full items-center gap-3 py-2">
                                      <div className="inline-flex flex-1 items-center gap-2 leading-none">
                                        <div className="text-text-default flex flex-col gap-1 text-xs">
                                          <span className="text-text-default inline-flex items-center gap-1 text-sm">
                                            {formatText(tool.name)}
                                          </span>
                                          <span className="text-text-secondary line-clamp-2 text-sm">
                                            {tool.description}
                                          </span>
                                        </div>
                                      </div>
                                      {(tool.config ?? []).length > 0 && (
                                        <Button
                                          className={cn(
                                            buttonVariants({
                                              variant: 'outline',
                                              size: 'xs',
                                            }),
                                          )}
                                          onClick={() => {
                                            setSelectedToolConfig(
                                              tool.tool_router_key,
                                            );
                                            window.location.hash =
                                              '#configuration';
                                          }}
                                          type="button"
                                        >
                                          <BoltIcon className="size-4" />
                                          Configure
                                        </Button>
                                      )}
                                      <Switch
                                        checked={form
                                          .watch('tools')
                                          .includes(tool.tool_router_key)}
                                        className="shrink-0"
                                        id={tool.tool_router_key}
                                        onCheckedChange={() => {
                                          form.setValue(
                                            'tools',
                                            form
                                              .watch('tools')
                                              .filter(
                                                (t) =>
                                                  t !== tool.tool_router_key,
                                              ),
                                          );

                                          const currentToolOverrideValue =
                                            form.getValues(
                                              'tools_config_override',
                                            )?.[tool.tool_router_key] ?? {};

                                          delete currentToolOverrideValue[
                                            tool.tool_router_key
                                          ];
                                          form.setValue(
                                            'tools_config_override',
                                            currentToolOverrideValue,
                                          );
                                        }}
                                      />
                                    </div>
                                    {(tool.config ?? [])?.length > 0 && (
                                      <div className="flex flex-col">
                                        <Collapsible>
                                          <CollapsibleTrigger asChild>
                                            <Button
                                              className={cn(
                                                buttonVariants({
                                                  variant: 'outline',
                                                  size: 'sm',
                                                }),
                                                'w-full justify-between',
                                              )}
                                            >
                                              <span>
                                                {t(
                                                  'agents.create.overrideConfigurations',
                                                )}
                                              </span>
                                              <ChevronDownIcon className="h-4 w-4" />
                                            </Button>
                                          </CollapsibleTrigger>
                                          <CollapsibleContent className="space-y-2">
                                            <TooConfigOverrideForm
                                              onChange={(value) => {
                                                form.setValue(
                                                  'tools_config_override',
                                                  {
                                                    ...form.getValues(
                                                      'tools_config_override',
                                                    ),
                                                    [tool.tool_router_key]:
                                                      value,
                                                  },
                                                );
                                              }}
                                              toolRouterKey={
                                                tool.tool_router_key
                                              }
                                              value={
                                                form.getValues(
                                                  'tools_config_override',
                                                )?.[tool.tool_router_key] ?? {}
                                              }
                                            />
                                          </CollapsibleContent>
                                        </Collapsible>
                                      </div>
                                    )}
                                  </div>
                                );
                              })}
                            </div>
                          </div>
                        )}

                        <SearchInput
                          classNames={{
                            container: 'mx-0.5 h-9 ',
                            input: 'bg-transparent',
                          }}
                          onChange={(e) => {
                            setSearchQuery(e.target.value);
                          }}
                          value={searchQuery}
                        />

                        <div className="flex flex-col gap-5 overflow-auto">
                          {(isPending ||
                            !isSearchQuerySynced ||
                            isSearchToolListPending) && (
                            <div className="grid grid-cols-1">
                              {Array.from({ length: 4 }).map((_, idx) => (
                                <div
                                  className={cn(
                                    'grid animate-pulse grid-cols-[1fr_40px] items-center justify-between gap-5 rounded-xs py-3 text-left text-sm',
                                  )}
                                  key={idx}
                                >
                                  <div className="flex w-full flex-1 flex-col gap-3">
                                    <Skeleton className="h-4 w-36 rounded-xs" />
                                    <div className="flex flex-col gap-1">
                                      <Skeleton className="h-3 w-full rounded-xs" />
                                      <Skeleton className="h-3 w-2/4 rounded-xs" />
                                    </div>
                                  </div>
                                  <Skeleton className="h-5 w-[36px] rounded-full" />
                                </div>
                              ))}
                            </div>
                          )}
                          {searchQuery &&
                            isSearchQuerySynced &&
                            searchToolList?.map((tool) => (
                              <FormField
                                control={form.control}
                                key={tool.tool_router_key}
                                name="tools"
                                render={({ field }) => (
                                  <FormItem className="flex w-full flex-col gap-3">
                                    <div className="flex items-center gap-3">
                                      <FormControl>
                                        <div className="flex w-full items-center gap-3">
                                          <div className="inline-flex flex-1 items-center gap-2 leading-none">
                                            <label
                                              className="text-text-default flex flex-col gap-1 text-xs"
                                              htmlFor={tool.tool_router_key}
                                            >
                                              <span className="text-text-default inline-flex items-center gap-1 text-sm">
                                                {formatText(tool.name)}
                                              </span>
                                              <span className="text-text-secondary line-clamp-2 text-sm">
                                                {tool.description}
                                              </span>
                                            </label>
                                          </div>
                                          {(tool.config ?? []).length > 0 && (
                                            <Button
                                              className={cn(
                                                buttonVariants({
                                                  variant: 'outline',
                                                  size: 'xs',
                                                }),
                                              )}
                                              onClick={() => {
                                                setSelectedToolConfig(
                                                  tool.tool_router_key,
                                                );
                                                window.location.hash =
                                                  '#configuration';
                                              }}
                                              type="button"
                                            >
                                              <BoltIcon className="size-4" />
                                              {t('common.configure')}
                                            </Button>
                                          )}
                                          <Switch
                                            checked={field.value.includes(
                                              tool.tool_router_key,
                                            )}
                                            className="shrink-0"
                                            id={tool.tool_router_key}
                                            onCheckedChange={() => {
                                              const configs =
                                                tool?.config ?? [];
                                              const toolConfigurationNeeded =
                                                getToolRequiresConfigurations(
                                                  configs,
                                                );
                                              if (toolConfigurationNeeded) {
                                                field.onChange(
                                                  field.value.includes(
                                                    tool.tool_router_key,
                                                  )
                                                    ? field.value.filter(
                                                        (value) =>
                                                          value !==
                                                          tool.tool_router_key,
                                                      )
                                                    : [
                                                        ...field.value,
                                                        tool.tool_router_key,
                                                      ],
                                                );

                                                return;
                                              }
                                              toast.error(
                                                'Tool configuration is required',
                                                {
                                                  description:
                                                    'Please fill in the config required in tool details',
                                                },
                                              );
                                            }}
                                          />
                                        </div>
                                      </FormControl>
                                    </div>
                                  </FormItem>
                                )}
                              />
                            ))}
                          {isSearchQuerySynced && !searchQuery && (
                            <div className="flex items-center justify-between gap-3">
                              <label
                                className="text-text-default text-xs"
                                htmlFor="all"
                              >
                                Enabled All
                              </label>
                              <Switch
                                checked={
                                  (toolsList?.length ?? 0) > 0 && // Only checked if tools exist
                                  form.watch('tools').length ===
                                    toolsList?.length
                                }
                                id="all"
                                onCheckedChange={(checked) => {
                                  const isAllConfigFilled = toolsList
                                    ?.map((tool) => tool.config)
                                    .filter((item) => !!item)
                                    .flat()
                                    ?.map((conf) => ({
                                      key_name: conf.BasicConfig.key_name,
                                      key_value:
                                        conf.BasicConfig.key_value ?? '',
                                      required: conf.BasicConfig.required,
                                    }))
                                    .every(
                                      (conf) =>
                                        !conf.required ||
                                        (conf.required &&
                                          conf.key_value !== ''),
                                    );
                                  if (!isAllConfigFilled && checked) {
                                    // Check config only when enabling all
                                    toast.error('Tool configuration', {
                                      description:
                                        'Please fill in the config required in tool details',
                                    });
                                    return;
                                  }
                                  if (checked && toolsList) {
                                    form.setValue(
                                      'tools',
                                      toolsList.map(
                                        (tool) => tool.tool_router_key,
                                      ),
                                    );
                                  } else {
                                    form.setValue('tools', []);
                                  }
                                }}
                              />
                            </div>
                          )}
                          {!searchQuery &&
                            toolsList
                              ?.filter(
                                (tool) =>
                                  !form
                                    .watch('tools')
                                    .includes(tool.tool_router_key),
                              )
                              .map((tool) => (
                                <FormField
                                  control={form.control}
                                  key={tool.tool_router_key}
                                  name="tools"
                                  render={({ field }) => (
                                    <FormItem className="flex w-full flex-col gap-3">
                                      <div className="flex items-center gap-3">
                                        <FormControl>
                                          <div className="flex w-full items-center gap-3">
                                            <div className="inline-flex flex-1 items-center gap-2 leading-none">
                                              <label
                                                className="text-text-default flex flex-col gap-1 text-xs"
                                                htmlFor={tool.tool_router_key}
                                              >
                                                <span className="text-text-default inline-flex items-center gap-1 text-sm">
                                                  {formatText(tool.name)}
                                                </span>
                                                <span className="text-text-secondary line-clamp-2 text-sm">
                                                  {tool.description}
                                                </span>
                                              </label>
                                            </div>
                                            {(tool.config ?? []).length > 0 && (
                                              <Button
                                                className={cn(
                                                  buttonVariants({
                                                    variant: 'outline',
                                                    size: 'xs',
                                                  }),
                                                )}
                                                onClick={() => {
                                                  setSelectedToolConfig(
                                                    tool.tool_router_key,
                                                  );
                                                  window.location.hash =
                                                    '#configuration';
                                                }}
                                                type="button"
                                              >
                                                <BoltIcon className="size-4" />
                                                Configure
                                              </Button>
                                            )}
                                            <Switch
                                              checked={field.value.includes(
                                                tool.tool_router_key,
                                              )}
                                              className="shrink-0"
                                              id={tool.tool_router_key}
                                              onCheckedChange={() => {
                                                const configs =
                                                  tool?.config ?? [];
                                                if (
                                                  configs
                                                    .map((conf) => ({
                                                      key_name:
                                                        conf.BasicConfig
                                                          .key_name,
                                                      key_value:
                                                        conf.BasicConfig
                                                          .key_value ?? '',
                                                      required:
                                                        conf.BasicConfig
                                                          .required,
                                                    }))
                                                    .every(
                                                      (conf) =>
                                                        !conf.required ||
                                                        (conf.required &&
                                                          conf.key_value !==
                                                            ''),
                                                    )
                                                ) {
                                                  field.onChange(
                                                    field.value.includes(
                                                      tool.tool_router_key,
                                                    )
                                                      ? field.value.filter(
                                                          (value) =>
                                                            value !==
                                                            tool.tool_router_key,
                                                        )
                                                      : [
                                                          ...field.value,
                                                          tool.tool_router_key,
                                                        ],
                                                  );

                                                  return;
                                                }
                                                toast.error(
                                                  'Tool configuration is required',
                                                  {
                                                    description:
                                                      'Please fill in the config required in tool details',
                                                  },
                                                );
                                              }}
                                            />
                                          </div>
                                        </FormControl>
                                      </div>
                                    </FormItem>
                                  )}
                                />
                              ))}
                        </div>
                      </div>
                    </TabsContent>
                    <TabsContent value="schedule">
                      <div className="space-y-4">
                        <div className="space-y-1">
                          <h2 className="text-base font-medium">
                            {t('agents.create.schedule')}
                          </h2>
                          <p className="text-text-secondary text-sm">
                            {t('agents.create.scheduleDescription')}
                          </p>
                        </div>
                        {/* Conditional rendering logic for schedule options */}
                        {/* Only show radio group if mode is 'add' OR if mode is 'edit' AND there are no existing tasks */}
                        {(mode === 'add' ||
                          (mode === 'edit' &&
                            (!agent?.cron_tasks ||
                              agent.cron_tasks.length === 0))) && (
                          <RadioGroup
                            className="space-y-3"
                            onValueChange={(value) => {
                              const newType = value as 'normal' | 'scheduled';
                              setScheduleType(newType);
                              // Clear cronExpression if switching to normal
                              if (newType === 'normal') {
                                form.setValue('cronExpression', '');
                              }
                            }}
                            value={scheduleType}
                          >
                            <div className="border-divider flex items-start space-x-3 rounded-lg border p-3">
                              <RadioGroupItem
                                className="mt-1"
                                id="schedule-always-on"
                                value="normal"
                              />
                              <div className="space-y-1">
                                <Label
                                  className="font-medium"
                                  htmlFor="schedule-always-on"
                                >
                                  {t('agents.create.scheduleNormalUsage')}
                                </Label>
                                <p className="text-text-secondary text-sm">
                                  {t(
                                    'agents.create.scheduleNormalUsageDescription',
                                  )}
                                </p>
                              </div>
                            </div>

                            <div className="border-divider flex items-start space-x-3 rounded-lg border p-3">
                              <RadioGroupItem
                                className="mt-1"
                                id="schedule-recurring"
                                value="scheduled"
                              />
                              <div className="w-full space-y-4">
                                <div className="space-y-1">
                                  <Label
                                    className="font-medium"
                                    htmlFor="schedule-recurring"
                                  >
                                    {t('agents.create.scheduleRecurring')}
                                  </Label>
                                  <p className="text-text-secondary text-sm">
                                    {t(
                                      'agents.create.scheduleRecurringDescription',
                                    )}
                                  </p>
                                </div>

                                {scheduleType === 'scheduled' && (
                                  <div className="space-y-4 py-5">
                                    <FormField
                                      control={form.control}
                                      name="aiPrompt"
                                      render={({ field }) => (
                                        <FormItem>
                                          <FormLabel>
                                            {t(
                                              'agents.create.scheduleAIInstructions',
                                            )}
                                          </FormLabel>

                                          <FormControl>
                                            <Textarea
                                              {...field}
                                              className="min-h-[220px]"
                                              id="aiPrompt"
                                              placeholder={t(
                                                'agents.create.scheduleAIInstructionsDescription',
                                              )}
                                              resize="vertical"
                                            />
                                          </FormControl>

                                          <FormMessage />
                                        </FormItem>
                                      )}
                                    />
                                    <p className="text-text-secondary text-sm">
                                      {t(
                                        'agents.create.scheduleAIInstructionsHelper',
                                      )}
                                    </p>

                                    <FormField
                                      control={form.control}
                                      name="cronExpression"
                                      render={({ field }) => (
                                        <TextField
                                          field={field}
                                          helperMessage={t(
                                            'agents.create.cronExpressionHelper',
                                          )}
                                          label={t(
                                            'agents.create.cronExpression',
                                          )}
                                        />
                                      )}
                                    />
                                    {readableCronExpression && (
                                      <div className="flex items-center gap-2 text-xs">
                                        <ScheduledTasksIcon className="size-4" />
                                        <span>
                                          This cron will run{' '}
                                          {readableCronExpression.toLowerCase()}{' '}
                                          <span className="text-text-secondary font-mono">
                                            ({form.watch('cronExpression')})
                                          </span>
                                        </span>
                                      </div>
                                    )}
                                    {!readableCronExpression &&
                                      form.watch('cronExpression') && (
                                        <p className="text-xs text-red-500">
                                          Invalid Cron Expression
                                        </p>
                                      )}
                                    <div className="flex flex-wrap gap-2">
                                      {[
                                        {
                                          label: 'every 5 min',
                                          cron: '*/5 * * * *',
                                        },
                                        {
                                          label: 'every 5 hours',
                                          cron: '0 */5 * * *',
                                        },
                                        {
                                          label: 'every monday at 8am',
                                          cron: '0 8 * * 1',
                                        },
                                        {
                                          label: 'every january 1st at 12am',
                                          cron: '0 0 1 1 *',
                                        },
                                        {
                                          label:
                                            'every 1st of the month at 12pm',
                                          cron: '0 12 1 * *',
                                        },
                                      ].map((item) => (
                                        <Badge
                                          className="cursor-pointer font-normal"
                                          key={item.cron}
                                          onClick={() => {
                                            form.setValue(
                                              'cronExpression',
                                              item.cron,
                                            );
                                          }}
                                          variant="outline"
                                        >
                                          <span className="text-xs">
                                            {item.label}
                                          </span>
                                        </Badge>
                                      ))}
                                    </div>
                                  </div>
                                )}
                              </div>
                            </div>
                          </RadioGroup>
                        )}
                        {/* Show current tasks only in edit mode if they exist */}
                        {mode === 'edit' &&
                          agent?.cron_tasks &&
                          agent?.cron_tasks?.length > 0 && (
                            <div className="mt-2 space-y-4">
                              <div className="mb-2 flex items-center gap-2">
                                <ScheduledTasksIcon className="text-text-default size-4" />
                                <h4 className="text-sm font-medium">
                                  Current Scheduled Task
                                </h4>
                              </div>
                              <div className="mt-2 space-y-3">
                                {/* Assuming only one task is manageable via this form */}
                                {agent?.cron_tasks?.slice(0, 1).map((task) => (
                                  <div
                                    className="bg-bg-secondary flex items-center justify-between rounded-md border p-2"
                                    key={task.task_id}
                                  >
                                    <div className="flex items-center gap-2 pl-1">
                                      <div className="bg-brand/70 h-2 w-2 rounded-full" />
                                      <span className="text-sm">
                                        {cronstrue.toString(task.cron, {
                                          throwExceptionOnParseError: false,
                                        })}{' '}
                                        ({task.cron})
                                      </span>
                                    </div>

                                    <Button
                                      className="text-text-secondary p-2 hover:bg-red-900/10 hover:text-red-400/90"
                                      isLoading={isRemovingTask}
                                      onClick={() =>
                                        onDeleteTask(task.task_id.toString())
                                      }
                                      size="auto"
                                      type="button"
                                      variant="tertiary"
                                    >
                                      <Trash2 className="h-4 w-4" />
                                    </Button>
                                  </div>
                                ))}
                              </div>
                              {/* Option to switch back to normal if a task exists */}
                              <Button
                                className="h-auto p-0 text-xs text-blue-400"
                                onClick={() => {
                                  setScheduleType('normal');
                                  form.setValue('cronExpression', ''); // Clear expression when switching
                                }}
                                variant="link"
                              >
                                Switch back to Normal Usage (will remove
                                schedule on save)
                              </Button>
                            </div>
                          )}
                      </div>
                    </TabsContent>
                  </Tabs>
                </div>
              </div>

              <div className="bg-bg-default sticky bottom-0 bg-gradient-to-t to-transparent pb-10">
                <div className="flex items-center justify-end gap-2">
                  <Button
                    className="min-w-[120px]"
                    disabled={isPending}
                    onClick={() => {
                      const previousTab = getPreviousTab(currentTab);

                      if (!previousTab) {
                        void navigate(-1);
                        return;
                      }

                      setCurrentTab(previousTab);
                    }}
                    size="sm"
                    type="button"
                    variant="outline"
                  >
                    {currentTab === 'persona'
                      ? t('common.cancel')
                      : t('common.back')}
                  </Button>
                  <Button
                    isLoading={isPending}
                    onClick={(e) => {
                      // Check validity before navigating or submitting
                      if (
                        scheduleType === 'scheduled' &&
                        (!form.watch('cronExpression') ||
                          !readableCronExpression)
                      ) {
                        toast.error(
                          'Invalid or empty Cron Expression for scheduled execution.',
                        );
                        e.preventDefault(); // Prevent moving forward
                        setCurrentTab('schedule'); // Stay on schedule tab
                        return;
                      }

                      const nextTab = getNextTab(currentTab);

                      if (nextTab) {
                        e.preventDefault();
                        setCurrentTab(nextTab);
                      }
                      // If no next tab (i.e. current tab is 'schedule'), the button type is 'submit',
                      // so default form submission occurs (handled by onSubmit)
                    }}
                    size="sm"
                    title={
                      scheduleType === 'scheduled' &&
                      (!form.watch('cronExpression') || !readableCronExpression)
                        ? 'Please enter a valid Cron Expression'
                        : ''
                    }
                    type={currentTab === 'schedule' ? 'submit' : 'button'}
                    className="min-w-[120px]"
                    // Disable Next/Save if scheduleType is 'scheduled' but cron expression is invalid or empty
                    disabled={
                      isPending ||
                      (scheduleType === 'scheduled' &&
                        (!form.watch('cronExpression') ||
                          !readableCronExpression))
                    }
                  >
                    {currentTab === 'schedule'
                      ? t('common.save')
                      : t('common.next')}
                  </Button>
                  {mode === 'add' && currentTab === 'schedule' && (
                    <Button
                      isLoading={isPending}
                      onClick={() => {
                        // Trigger form validation and submission with the openChat option
                        void form.handleSubmit((values) =>
                          submit(values, { openChat: true }),
                        )();
                      }}
                      size="sm"
                      title={
                        scheduleType === 'scheduled' &&
                        (!form.watch('cronExpression') ||
                          !readableCronExpression)
                          ? 'Please enter a valid Cron Expression'
                          : ''
                      }
                      type="button"
                      className="flex min-w-[120px] items-center gap-2"
                      // Also disable if schedule invalid
                      disabled={
                        isPending ||
                        (scheduleType === 'scheduled' &&
                          (!form.watch('cronExpression') ||
                            !readableCronExpression))
                      }
                    >
                      <MessageSquare className="h-4 w-4" />
                      {t('agents.create.saveAndTestAgent')}
                    </Button>
                  )}
                </div>
              </div>
            </form>
          </Form>
        </div>
      </ResizablePanel>
      {selectedToolConfig && (
        <ToolConfigModal
          isOpen={!!selectedToolConfig}
          onConfirm={() => {
            if (form.watch('tools').includes(selectedToolConfig)) {
              setSelectedToolConfig(null);
              return;
            }

            form.setValue('tools', [
              ...form.watch('tools'),
              selectedToolConfig,
            ]);
            setSelectedToolConfig(null);
          }}
          onOpenChange={(open) => {
            if (!open) {
              setSelectedToolConfig(null);
            }
          }}
          toolRouterKey={selectedToolConfig}
        />
      )}
      <UploadFileDialog
        isOpen={!!isAddFilesDialogOpen}
        onOpenChange={setIsAddFilesDialogOpen}
      />
      {/* Side Chat Panel */}
      {isSideChatOpen && mode === 'edit' && agent && (
        <>
          <ResizableHandle className="bg-divider" />
          <ResizablePanel
            className="flex h-full min-h-0 flex-col"
            collapsible
            defaultSize={30}
            maxSize={50}
            minSize={20}
          >
            <div className="h-full min-h-0 overflow-hidden">
              <ToolsProvider>
                <AgentSideChat
                  agentId={agent.agent_id}
                  onClose={() => setIsSideChatOpen(false)}
                />
              </ToolsProvider>
            </div>
          </ResizablePanel>
        </>
      )}
    </ResizablePanelGroup>
  );
}

export default AgentForm;

const ToolConfigModal = ({
  toolRouterKey,
  isOpen,
  onOpenChange,
  onConfirm,
}: {
  toolRouterKey: string;
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
  onConfirm: () => void;
}) => {
  const auth = useAuth((state) => state.auth);
  const { data, isSuccess, isPending } = useGetTool({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
    toolKey: toolRouterKey ?? '',
  });

  const tool = data?.content[0] as ShinkaiTool;
  const isEnabled = data?.content[1] as boolean;
  const toolType = data?.type as ShinkaiToolType;

  const hasAllRequiredFields = useMemo(() => {
    if (
      isSuccess &&
      'configurations' in tool &&
      tool.configurations?.properties
    ) {
      const requiredFields = tool.configurations.required || [];
      const configFormData = tool.configFormData || {};
      const hasAllRequiredFields = requiredFields.every(
        (field) =>
          field in configFormData &&
          configFormData[field] !== undefined &&
          configFormData[field] !== null &&
          configFormData[field] !== '',
      );
      return hasAllRequiredFields;
    }
    return false;
  }, [isSuccess, tool]);

  return (
    <Dialog onOpenChange={onOpenChange} open={isOpen}>
      <DialogContent
        className="flex max-h-[80vh] min-h-[600px] max-w-screen-md flex-col overflow-y-auto"
        onInteractOutside={(e) => {
          e.preventDefault();
        }}
      >
        <DialogClose asChild>
          <Button
            className="absolute top-4 right-4"
            size="icon"
            variant="tertiary"
          >
            <XIcon className="text-text-secondary h-5 w-5" />
          </Button>
        </DialogClose>
        <DialogHeader>
          <DialogTitle className="text-center">
            {isSuccess ? tool.name : 'Getting tool details...'}
          </DialogTitle>
        </DialogHeader>
        {isPending && <Skeleton className="flex-1 animate-pulse rounded" />}
        {isSuccess && (
          <ToolDetailsCard
            hideToolHeaderDetails
            isEnabled={isEnabled}
            tool={tool}
            toolKey={toolRouterKey}
            toolType={toolType}
          />
        )}
        <DialogFooter className="bg-bg-default sticky -bottom-6 bg-gradient-to-t to-transparent p-2">
          <div className="flex w-full items-center justify-between gap-2">
            <span>
              {!hasAllRequiredFields && (
                <div className="flex items-center gap-2 text-xs text-amber-400">
                  <AlertCircle className="h-4 w-4" />
                  <span>
                    Please configure all required tool configurations before
                    enabling
                  </span>
                </div>
              )}
            </span>
            <div className="flex items-center gap-2">
              <DialogClose asChild>
                <Button size="sm" variant="outline">
                  Cancel
                </Button>
              </DialogClose>
              <Button
                disabled={!hasAllRequiredFields}
                onClick={onConfirm}
                size="sm"
                type="button"
              >
                Enable Tool
              </Button>
            </div>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
};

const UploadFileDialog = ({
  isOpen,
  onOpenChange,
}: {
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
}) => {
  const { t } = useTranslation();

  const auth = useAuth((state) => state.auth);

  const uploadFileForm = useForm<UploadVRFilesFormSchema>({
    resolver: zodResolver(uploadVRFilesFormSchema),
  });
  const { isPending: isUploadingFile, mutateAsync: uploadVRFiles } =
    useUploadVRFiles({
      onSuccess: (_, _variables) => {
        toast.success(t('vectorFs.success.filesUploaded'), {
          id: 'uploading-VR-files',
          description: '',
        });
        uploadFileForm.reset();
      },
      onError: (error) => {
        toast.error(t('vectorFs.errors.filesUploaded'), {
          id: 'uploading-VR-files',
          description: error.message,
        });
      },
    });

  const onSubmit = async (values: UploadVRFilesFormSchema) => {
    if (!auth) return;
    toast.loading(t('vectorFs.pending.filesUploading'), {
      id: 'uploading-VR-files',
      description: 'This process might take from 1-2 minutes per file.',
      position: 'bottom-left',
    });
    onOpenChange(false);
    await uploadVRFiles({
      nodeAddress: auth?.node_address ?? '',
      destinationPath: '/',
      files: values.files,
      token: auth?.api_v2_key ?? '',
    });
  };

  return (
    <Dialog
      onOpenChange={(open) => {
        if (!open) {
          onOpenChange(false);
          uploadFileForm.reset();
        }
      }}
      open={!!isOpen}
    >
      <DialogContent className={cn('flex max-w-lg flex-col')}>
        <DialogClose
          className={cn(
            buttonVariants({
              variant: 'tertiary',
              size: 'icon',
            }),
            'absolute top-3 right-3 p-1',
          )}
          onClick={() => {
            onOpenChange(false);
            uploadFileForm.reset();
          }}
        >
          <XIcon className="size-4" />
        </DialogClose>
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <FileTypeIcon className="h-6 w-6" />
            {t('vectorFs.actions.uploadFile')}
          </DialogTitle>
        </DialogHeader>
        <Form {...uploadFileForm}>
          <form
            className="space-y-8"
            onSubmit={uploadFileForm.handleSubmit(onSubmit)}
          >
            <FormField
              control={uploadFileForm.control}
              name="files"
              render={({ field }) => (
                <FormItem className="mt-3">
                  <FormLabel className="sr-only">File</FormLabel>
                  <FormControl>
                    <div className="flex flex-col space-y-1">
                      <div className="flex items-center justify-center">
                        <FileUploader
                          allowMultiple
                          descriptionText={t('common.uploadAFileDescription')}
                          onChange={(acceptedFiles) => {
                            field.onChange(acceptedFiles);
                          }}
                          value={field.value}
                        />
                      </div>
                    </div>
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />
            <Button
              className="w-full"
              disabled={isUploadingFile}
              isLoading={isUploadingFile}
              size="sm"
              type="submit"
            >
              {t('common.upload')}
            </Button>
          </form>
        </Form>
      </DialogContent>
    </Dialog>
  );
};
