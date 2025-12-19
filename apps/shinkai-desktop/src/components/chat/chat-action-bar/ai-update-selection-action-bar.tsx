import { useTranslation } from '@shinkai_network/shinkai-i18n';
import { extractJobIdFromInbox } from '@shinkai_network/shinkai-message-ts/utils/inbox_name_handler';
import { useUpdateAgentInJob } from '@shinkai_network/shinkai-node-state/v2/mutations/updateAgentInJob/useUpdateAgentInJob';
import { useUpdateChatConfig } from '@shinkai_network/shinkai-node-state/v2/mutations/updateChatConfig/useUpdateChatConfig';
import { useGetAgents } from '@shinkai_network/shinkai-node-state/v2/queries/getAgents/useGetAgents';
import { useGetChatConfig } from '@shinkai_network/shinkai-node-state/v2/queries/getChatConfig/useGetChatConfig';
import { useGetLLMProviders } from '@shinkai_network/shinkai-node-state/v2/queries/getLLMProviders/useGetLLMProviders';
import { useGetProviderFromJob } from '@shinkai_network/shinkai-node-state/v2/queries/getProviderFromJob/useGetProviderFromJob';
import {
  Badge,
  buttonVariants,
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandSeparator,
  CommandShortcut,
  Dialog,
  DialogContent,
  DialogTitle,
  DialogTrigger,
  Tooltip,
  TooltipContent,
  TooltipPortal,
  TooltipTrigger,
} from '@shinkai_network/shinkai-ui';
import { AIAgentIcon } from '@shinkai_network/shinkai-ui/assets';
import { formatText } from '@shinkai_network/shinkai-ui/helpers';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import { BoltIcon, BotIcon, ChevronDownIcon, PlusIcon } from 'lucide-react';
import { memo, useMemo, useState } from 'react';
import { Link, useLocation } from 'react-router';
import { toast } from 'sonner';

import OLLAMA_MODELS_REPOSITORY from '../../../lib/shinkai-node-manager/ollama-models-repository.json';
import { useAuth } from '../../../store/auth';
import { useShinkaiNodeManager } from '../../../store/shinkai-node-manager';
import ProviderIcon from '../../ais/provider-icon';
import { CODE_GENERATOR_MODEL_ID } from '../../tools/constants';
import { actionButtonClassnames } from '../conversation-footer';

const ollamaDescriptionMap = OLLAMA_MODELS_REPOSITORY.reduce(
  (acc, model) => {
    acc[model.name] = model.description;
    return acc;
  },
  {} as Record<string, string>,
);

const nonOllamaProviderModels = {
  'shinkai-backend:free_text_inference':
    'Shinkai AI model for text generation.',
  'shinkai-backend:code_generator':
    'Shinkai AI model for generating tool code.',
  'openai:gpt-4o':
    'Powerful OpenAI model known for its ability to generate human-like text and handle complex tasks.',
  'openai:gpt-4o-mini':
    'Lightweight OpenAI model for concise text and image generation.',
  'openai:gpt-4o-2024-08-06':
    'Latest OpenAI GPT-4 model for diverse and accurate content generation.',
  'openai:gpt-4-1106-preview':
    'OpenAI GPT-4 Turbo model optimized for speed and cost.',
  'openai:gpt-4-vision-preview':
    'OpenAI GPT-4 model with image understanding capabilities.',
  'openai:gpt-3.5-turbo-1106':
    'Cost‑efficient OpenAI model for general text tasks.',
  'openai:gpt-4.1': 'Newest GPT‑4.1 model for high quality responses.',
  'openai:gpt-4.1-mini': 'Smaller GPT‑4.1 model offering lower cost.',
  'openai:gpt-4.1-nano': 'Fastest GPT‑4.1 variant for quick replies.',
  'openai:gpt-5.1':
    'OpenAI flagship model for coding and agentic tasks with configurable reasoning effort.',
  'openai:gpt-5.2':
    'Latest OpenAI GPT‑5.2 model for high quality reasoning and generation.',
  'openai:gpt-5.2-pro':
    'Higher‑end GPT‑5.2 variant optimized for deeper reasoning and longer tasks.',
  'openai:4o-preview': 'Preview version of GPT‑4o with multimodal support.',
  'openai:4o-mini': 'Compact GPT‑4o model balancing speed and quality.',
  'openai:o1': 'OpenAI lightweight reasoning model.',
  'openai:o1-mini': 'Smaller variant of OpenAI o1 model.',
  'openai:o3-mini': 'Mini version of OpenAI o3 model.',
  'gemini:gemini-3-pro-preview':
    'Google flagship model for complex reasoning across modalities with advanced thinking and coding capabilities.',
  'gemini:gemini-3-flash':
    'Fast Google Gemini 3 model optimized for low latency and high throughput.',
  'claude:claude-opus-4-5':
    'Anthropic Claude Opus 4.5 for top-tier reasoning and writing.',
  'claude:claude-sonnet-4-5':
    'Anthropic Claude Sonnet 4.5 balancing speed and quality.',
  'claude:claude-haiku-4-5':
    'Anthropic Claude Haiku 4.5 optimized for fast responses.',
  'grok:grok-4-1-fast':
    'xAI Grok 4.1 Fast optimized for speed with strong general capabilities.',
} as Record<string, string>;

export function AIModelSelectorBase({
  value,
  onValueChange,
  className,
  variant = 'simple',
}: {
  value: string;
  onValueChange: (value: string) => void;
  className?: string;
  variant?: 'simple' | 'card';
}) {
  const { t } = useTranslation();
  const location = useLocation();
  const auth = useAuth((state) => state.auth);
  const { isSuccess: isLlmProviderSuccess, llmProviders } = useGetLLMProviders(
    {
      nodeAddress: auth?.node_address ?? '',
      token: auth?.api_v2_key ?? '',
    },
    {
      enabled: true,
      select: (data) => {
        return data
          .map((provider) => {
            let description =
              'A versatile AI model for text generation and understanding.';
            if (provider.model.includes('ollama')) {
              const model = provider.model.split(':');
              description = ollamaDescriptionMap[model[1]] || '';
            } else if (provider.model.includes('claude')) {
              description =
                nonOllamaProviderModels[provider.model] ||
                'Safe and thoughtful Anthropic AI model with advanced coding capabilities';
            } else {
              description = nonOllamaProviderModels[provider.model] || '';
            }

            return {
              ...provider,
              description: description,
            };
          })
          .filter(
            (model) =>
              model.model.toLowerCase() !==
              CODE_GENERATOR_MODEL_ID.toLowerCase(),
          );
      },
    },
  );

  const { data: agents, isSuccess: isAgentsSuccess } = useGetAgents({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
  });
  const isRegularChatPage =
    location.pathname.includes('inboxes') || location.pathname.includes('home');

  const [isDialogOpen, setIsDialogOpen] = useState(false);

  const selectedIcon = useMemo(() => {
    const selectedProvider = llmProviders?.find(
      (llmProvider) => llmProvider.id === value,
    );
    if (selectedProvider) {
      return (
        <ProviderIcon
          className="mx-1 size-4"
          provider={selectedProvider.model.split(':')[0]}
        />
      );
    }
    const selectedAgent = agents?.find((agent) => agent.agent_id === value);
    if (selectedAgent) {
      return (
        <AIAgentIcon
          name={selectedAgent.name}
          size={variant === 'simple' ? 'xs' : 'xs'}
        />
      );
    }
    return <BotIcon className="mr-1 size-4" />;
  }, [agents, llmProviders, value, variant]);

  const selectedAgentName = useMemo(() => {
    const selectedAgent = agents?.find((agent) => agent.agent_id === value);
    if (selectedAgent) {
      return selectedAgent.name;
    }
    const selectedLlmProvider = llmProviders?.find(
      (llmProvider) => llmProvider.id === value,
    );
    if (selectedLlmProvider) {
      return formatText(selectedLlmProvider.name || selectedLlmProvider.id);
    }
    return '';
  }, [agents, llmProviders, value]);

  const isLocalShinkaiNodeIsUse = useShinkaiNodeManager(
    (state) => state.isInUse,
  );

  return (
    <Dialog onOpenChange={setIsDialogOpen} open={isDialogOpen}>
      <Tooltip>
        <TooltipTrigger asChild>
          <DialogTrigger asChild>
            <button
              type="button"
              className={cn(
                actionButtonClassnames,

                'w-auto justify-between truncate',
                variant === 'card' &&
                  'bg-bg-secondary hover:bg-bg-tertiary h-auto w-auto max-w-md min-w-[240px] gap-3 rounded-xl border border-gray-500 p-1.5 px-2',
                className,
              )}
            >
              {variant === 'simple' && (
                <>
                  <div className="flex items-center gap-1.5">
                    {selectedIcon}
                    <span className="capitalize">
                      {selectedAgentName ?? 'Select'}
                    </span>
                  </div>
                  <ChevronDownIcon className="icon h-3 w-3" />
                </>
              )}
              {variant === 'card' && (
                <>
                  {selectedIcon}
                  <div className="flex flex-col items-start justify-start text-left">
                    <span className="text-base font-medium capitalize">
                      {selectedAgentName}
                    </span>
                  </div>
                  <ChevronDownIcon className="ml-auto size-4" />
                </>
              )}
            </button>
          </DialogTrigger>
        </TooltipTrigger>
        <TooltipPortal>
          <TooltipContent
            align="center"
            className="flex flex-col gap-1"
            side="top"
          >
            <span className="text-center text-sm">
              {t('llmProviders.switch')}
            </span>
            {isRegularChatPage && (
              <div className="flex items-center gap-4 text-left">
                <div className="text-text-secondary flex items-center justify-center gap-2 text-xs">
                  <CommandShortcut>⌘ [</CommandShortcut> or
                  <CommandShortcut>⌘ ]</CommandShortcut>
                </div>
                <div className="flex items-center justify-center gap-2">
                  <span className="text-text-secondary text-xs">
                    Prev / Next AI
                  </span>
                </div>
              </div>
            )}
          </TooltipContent>
        </TooltipPortal>
      </Tooltip>
      <DialogContent className="size-full max-h-[60vh] w-full max-w-3xl border p-1 py-2">
        <DialogTitle className="sr-only">
          {t('llmProviders.switch')}
        </DialogTitle>
        <Command
          className="[&_[cmdk-input-wrapper]]:border-divider h-full [&_[cmdk-input-wrapper]]:pb-1"
          disablePointerSelection
          onKeyDown={(e) => {
            if (e.key === 'Escape') {
              setIsDialogOpen(false);
            }
          }}
          onValueChange={onValueChange}
          value={value}
        >
          <CommandInput placeholder={t('common.search')} />
          <CommandEmpty className="text-text-secondary py-5 text-center text-sm">
            {t('common.noResultsFound')}
          </CommandEmpty>
          <CommandList className="flex max-h-full flex-col">
            <CommandGroup
              className="py-4"
              heading={
                <div className="flex items-center justify-between gap-2 pb-2">
                  <div className="space-y-0.5">
                    <h3 className="font-clash text-text-default text-base font-medium">
                      {t('agents.label')}
                    </h3>
                    <p className="text-text-secondary text-sm font-normal">
                      {t('agentsPage.exploreAgentsDescription')}
                    </p>
                  </div>
                  <Link
                    className={cn(
                      buttonVariants({
                        size: 'xs',
                      }),
                    )}
                    to="/add-agent"
                  >
                    <PlusIcon className="size-4" />
                    {t('common.add')}
                  </Link>
                </div>
              }
            >
              {isAgentsSuccess &&
                agents.map((agent) => (
                  <CommandItem
                    className="flex cursor-pointer items-center justify-between gap-1.5 rounded-md px-2 py-2 transition-colors"
                    key={agent.agent_id}
                    onSelect={() => {
                      setIsDialogOpen(false);
                    }}
                    value={agent.agent_id}
                  >
                    <div className="inline-flex items-center gap-3">
                      <AIAgentIcon name={agent.name} size="sm" />
                      <div className="flex flex-col gap-1">
                        <span className="inline-flex items-center gap-1.5 text-base font-medium capitalize">
                          {agent.name}
                        </span>
                        <span className="text-text-secondary line-clamp-1 text-sm">
                          {agent.ui_description}
                        </span>
                      </div>
                    </div>
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <Link
                          className="text-text-secondary hover:text-text-default size-8 shrink-0 rounded-lg p-2"
                          to={`/agents/edit/${agent.agent_id}`}
                        >
                          <BoltIcon className="size-full" />
                        </Link>
                      </TooltipTrigger>
                      <TooltipPortal>
                        <TooltipContent
                          align="center"
                          alignOffset={-10}
                          className="z-[2000000001] max-w-md"
                          side="top"
                        >
                          <p>{t('agents.configureAgent')}</p>
                        </TooltipContent>
                      </TooltipPortal>
                    </Tooltip>
                  </CommandItem>
                ))}
            </CommandGroup>
            <CommandSeparator className="" />
            <CommandGroup
              className="py-4"
              heading={
                <div className="flex items-center justify-between gap-2 pb-2">
                  <div className="space-y-0.5">
                    <h3 className="font-clash text-text-default text-base font-medium">
                      {t('aisPage.label')}
                    </h3>
                    <p className="text-text-secondary text-sm font-normal">
                      {t('aisPage.shortDescription')}
                    </p>
                  </div>
                  <Link
                    className={cn(
                      buttonVariants({
                        size: 'xs',
                      }),
                    )}
                    to={
                      isLocalShinkaiNodeIsUse ? '/install-ai-models' : '/add-ai'
                    }
                  >
                    <PlusIcon className="size-4" />
                    {t('common.add')}
                  </Link>
                </div>
              }
            >
              {isLlmProviderSuccess &&
                llmProviders?.length > 0 &&
                llmProviders?.map((llmProvider) => (
                  <CommandItem
                    className="flex cursor-pointer items-start gap-3 rounded-md px-2 py-2 transition-colors"
                    key={llmProvider.id}
                    onSelect={() => {
                      setIsDialogOpen(false);
                    }}
                    value={llmProvider.id}
                  >
                    <div className="bg-bg-secondary border-border flex size-8 shrink-0 items-center justify-center gap-2 rounded-lg border p-2">
                      <ProviderIcon
                        className="mt-0.5 size-5 shrink-0"
                        provider={llmProvider.model.split(':')[0]}
                      />
                    </div>
                    <div className="flex flex-col gap-0.5">
                      <span className="text-base font-medium capitalize">
                        {formatText(llmProvider?.name || llmProvider.id || '')}
                        {location.pathname.includes('tools') &&
                          llmProvider.model.toLowerCase() ===
                            CODE_GENERATOR_MODEL_ID.toLowerCase() && (
                            <Badge
                              className="ml-2 border bg-emerald-900/40 px-1 py-0 text-xs font-medium text-emerald-400"
                              variant="secondary"
                            >
                              {t('common.recommended')}
                            </Badge>
                          )}
                      </span>
                      <span className="text-text-secondary line-clamp-2 text-sm">
                        {llmProvider?.description}
                      </span>
                    </div>
                  </CommandItem>
                ))}
            </CommandGroup>
          </CommandList>
        </Command>
      </DialogContent>
    </Dialog>
  );
}

export const AIModelSelector = memo(AIModelSelectorBase);

export function AiUpdateSelectionActionBarBase({
  inboxId,
}: {
  inboxId: string;
}) {
  const { t } = useTranslation();
  const auth = useAuth((state) => state.auth);

  const { data: provider } = useGetProviderFromJob({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
    jobId: inboxId ? extractJobIdFromInbox(inboxId) : '',
  });

  const { mutateAsync: updateAgentInJob, isPending } = useUpdateAgentInJob({
    onError: (error) => {
      toast.error(t('llmProviders.errors.updateAgent'), {
        description: error.message,
      });
    },
  });

  const { data: agents } = useGetAgents({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
  });

  const { data: chatConfig } = useGetChatConfig(
    {
      nodeAddress: auth?.node_address ?? '',
      token: auth?.api_v2_key ?? '',
      jobId: inboxId ? extractJobIdFromInbox(inboxId) : '',
    },
    { enabled: !!inboxId },
  );

  const { mutateAsync: updateChatConfig } = useUpdateChatConfig({
    onError: (error) => {
      toast.error('Use tools update failed', {
        description: error.response?.data?.message ?? error.message,
      });
    },
  });

  const handleUpdateToolUsage = async (enabled?: boolean) => {
    await updateChatConfig({
      nodeAddress: auth?.node_address ?? '',
      token: auth?.api_v2_key ?? '',
      jobId: extractJobIdFromInbox(inboxId),
      jobConfig: {
        stream: chatConfig?.stream,
        custom_prompt: chatConfig?.custom_prompt ?? '',
        temperature: chatConfig?.temperature,
        top_p: chatConfig?.top_p,
        top_k: chatConfig?.top_k,
        use_tools: enabled,
        thinking: chatConfig?.thinking,
        reasoning_effort: chatConfig?.reasoning_effort,
        web_search_enabled: chatConfig?.web_search_enabled,
      },
    });
  };

  return (
    <AIModelSelector
      onValueChange={async (value) => {
        if (!provider || isPending) return;
        const jobId = extractJobIdFromInbox(inboxId ?? '');
        await updateAgentInJob({
          nodeAddress: auth?.node_address ?? '',
          token: auth?.api_v2_key ?? '',
          jobId,
          newAgentId: value,
        });
        const selectedAgent = agents?.find((agent) => agent.agent_id === value);
        const hasTools = (selectedAgent?.tools ?? [])?.length > 0;
        await handleUpdateToolUsage(hasTools);
      }}
      value={provider?.agent?.id ?? ''}
    />
  );
}
export const AiUpdateSelectionActionBar = memo(
  AiUpdateSelectionActionBarBase,
  (prevProps, nextProps) => prevProps.inboxId === nextProps.inboxId,
);
