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
  CommandShortcut,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
  Tooltip,
  TooltipContent,
  TooltipPortal,
  TooltipTrigger,
} from '@shinkai_network/shinkai-ui';
import { formatText } from '@shinkai_network/shinkai-ui/helpers';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import { BotIcon, ChevronDownIcon } from 'lucide-react';
import { memo, useMemo, useState } from 'react';
import { useLocation } from 'react-router';
import { toast } from 'sonner';

import { useAuth } from '../../../store/auth';
import ProviderIcon from '../../ais/provider-icon';
import { actionButtonClassnames } from '../../chat/conversation-footer';
import { CODE_GENERATOR_MODEL_ID } from '../../tools/constants';

const RECOMMENDED_MODELS_FOR_TOOLS = [
  // order is important
  'shinkai_code_generator',
  'shinkai_free_trial',
];

export function AIModelSelectorToolsBase({
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
  const { isSuccess: isLlmProviderSuccess, llmProviders } = useGetLLMProviders({
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

    return <BotIcon className="mr-1 size-4" />;
  }, [llmProviders, value]);

  const selectedAgentName = useMemo(() => {
    const selectedLlmProvider = llmProviders?.find(
      (llmProvider) => llmProvider.id === value,
    );
    if (selectedLlmProvider) {
      return formatText(selectedLlmProvider.name || selectedLlmProvider.id);
    }
    return '';
  }, [llmProviders, value]);

  const toolRecommendedModels = useMemo(() => {
    return llmProviders
      ?.filter((llmProvider) =>
        RECOMMENDED_MODELS_FOR_TOOLS.includes(llmProvider.id),
      )
      ?.sort((a) => {
        if (a.id === RECOMMENDED_MODELS_FOR_TOOLS[0]) return -1;
        return 0;
      });
  }, [llmProviders]);

  const restModels = useMemo(() => {
    return llmProviders?.filter(
      (llmProvider) => !RECOMMENDED_MODELS_FOR_TOOLS.includes(llmProvider.id),
    );
  }, [llmProviders]);

  return (
    <DropdownMenu onOpenChange={setIsDialogOpen} open={isDialogOpen}>
      <Tooltip>
        <TooltipTrigger asChild>
          <DropdownMenuTrigger asChild>
            <button
              className={cn(
                actionButtonClassnames,
                'w-auto justify-between truncate',
                variant === 'card' &&
                  'bg-bg-secondary border-divider hover:bg-bg-secondary h-auto w-auto max-w-md min-w-[240px] gap-3 rounded-xl border p-1.5 px-2',
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
                    <span className="text-text-default text-sm capitalize">
                      {selectedAgentName}
                    </span>
                  </div>
                  <ChevronDownIcon className="ml-auto size-4" />
                </>
              )}
            </button>
          </DropdownMenuTrigger>
        </TooltipTrigger>
        <TooltipPortal>
          <TooltipContent
            align="center"
            className="flex flex-col gap-1"
            side="top"
          >
            <span className="text-text-default text-center text-xs">
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
      <DropdownMenuContent
        align="start"
        className="size-full max-h-[60vh] w-full max-w-[400px] overflow-y-auto border p-1 py-2"
      >
        <div className="space-y-1 px-2 py-2">
          <DropdownMenuLabel>Best Models for Tool Builder</DropdownMenuLabel>
          <p className="text-text-secondary text-xs">
            These models are optimized for tool creation specifically.
          </p>
        </div>
        <DropdownMenuRadioGroup onValueChange={onValueChange} value={value}>
          {isLlmProviderSuccess &&
            llmProviders?.length > 0 &&
            toolRecommendedModels?.map((llmProvider) => (
              <DropdownMenuRadioItem
                className="text-text-default flex cursor-pointer items-center gap-2 rounded-md px-2 py-2 transition-colors"
                key={llmProvider.id}
                value={llmProvider.id}
              >
                <div className="bg-bg-secondary border-divider flex size-5 shrink-0 items-center justify-center gap-2 rounded-lg border p-2">
                  <ProviderIcon
                    className="mt-0.5 size-3 shrink-0"
                    provider={llmProvider.model.split(':')[0]}
                  />
                </div>
                <span className="text-sm capitalize">
                  {formatText(llmProvider?.name || llmProvider.id || '')}
                  {llmProvider.model.toLowerCase() ===
                    CODE_GENERATOR_MODEL_ID.toLowerCase() && (
                    <Badge
                      className="ml-2 border bg-emerald-900/40 px-1 py-0 text-xs font-medium text-emerald-400"
                      variant="secondary"
                    >
                      Recommended
                    </Badge>
                  )}
                </span>
              </DropdownMenuRadioItem>
            ))}

          <DropdownMenuSeparator className="my-2" />
          <div className="space-y-1 px-2 py-2">
            <DropdownMenuLabel>Other Models</DropdownMenuLabel>
            <p className="text-text-secondary text-xs">
              Use your own models, but note they may not be optimized for tool
              creation specifically.
            </p>
          </div>

          {restModels?.map((llmProvider) => (
            <DropdownMenuRadioItem
              className="flex cursor-pointer items-center gap-2 rounded-md px-2 py-2 transition-colors"
              key={llmProvider.id}
              value={llmProvider.id}
            >
              <div className="bg-bg-secondary border-divider flex size-5 shrink-0 items-center justify-center gap-2 rounded-lg border p-2">
                <ProviderIcon
                  className="mt-0.5 size-4 shrink-0"
                  provider={llmProvider.model.split(':')[0]}
                />
              </div>
              <span className="text-sm capitalize">
                {formatText(llmProvider?.name || llmProvider.id || '')}
              </span>
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

export const AIModelSelectorTools = memo(AIModelSelectorToolsBase);

export function AiModelUpdateSelectionToolsBase({
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
      },
    });
  };

  return (
    <AIModelSelectorTools
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
export const AiModelUpdateSelectionTools = memo(
  AiModelUpdateSelectionToolsBase,
  (prevProps, nextProps) => prevProps.inboxId === nextProps.inboxId,
);
