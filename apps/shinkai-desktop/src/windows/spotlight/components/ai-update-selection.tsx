import { useTranslation } from '@shinkai_network/shinkai-i18n';
import { extractJobIdFromInbox } from '@shinkai_network/shinkai-message-ts/utils/inbox_name_handler';
import { useUpdateAgentInJob } from '@shinkai_network/shinkai-node-state/v2/mutations/updateAgentInJob/useUpdateAgentInJob';
import { useUpdateChatConfig } from '@shinkai_network/shinkai-node-state/v2/mutations/updateChatConfig/useUpdateChatConfig';
import { useGetAgents } from '@shinkai_network/shinkai-node-state/v2/queries/getAgents/useGetAgents';
import { useGetChatConfig } from '@shinkai_network/shinkai-node-state/v2/queries/getChatConfig/useGetChatConfig';
import { useGetLLMProviders } from '@shinkai_network/shinkai-node-state/v2/queries/getLLMProviders/useGetLLMProviders';
import { useGetProviderFromJob } from '@shinkai_network/shinkai-node-state/v2/queries/getProviderFromJob/useGetProviderFromJob';
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandSeparator,
  Popover,
  PopoverTrigger,
  PopoverContent,
} from '@shinkai_network/shinkai-ui';
import { AIAgentIcon } from '@shinkai_network/shinkai-ui/assets';
import { formatText } from '@shinkai_network/shinkai-ui/helpers';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import { BotIcon, ChevronDownIcon } from 'lucide-react';
import { memo, useMemo, useState } from 'react';
import { toast } from 'sonner';

import ProviderIcon from '../../../components/ais/provider-icon';
import { useAuth } from '../../../store/auth';

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
  const auth = useAuth((state) => state.auth);
  const { isSuccess: isLlmProviderSuccess, llmProviders } = useGetLLMProviders({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
  });

  const { data: agents, isSuccess: isAgentsSuccess } = useGetAgents({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
  });

  const [isPopoverOpen, setIsPopoverOpen] = useState(false);

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

  return (
    <Popover open={isPopoverOpen} onOpenChange={setIsPopoverOpen}>
      <PopoverTrigger asChild>
        <button
          className={cn(
            'bg-bg-tertiary border-divider hover:bg-bg-secondary flex h-auto w-auto max-w-md min-w-[180px] items-center justify-between gap-3 truncate rounded-xl border p-1.5 px-2',
            className,
          )}
        >
          <div className="flex items-center gap-1.5">
            {selectedIcon}
            <span className="text-sm capitalize">
              {selectedAgentName ?? 'Select'}
            </span>
          </div>
          <ChevronDownIcon className="icon h-3 w-3" />
        </button>
      </PopoverTrigger>
      <PopoverContent
        align="start"
        className="bg-bg-dark border-divider flex size-full max-h-[60vh] w-full flex-col border p-1 py-2"
      >
        <Command
          className="[&_[cmdk-input-wrapper]]:border-divider flex size-full max-w-[400px] min-w-[340px] flex-col [&_[cmdk-input-wrapper]]:pb-1"
          disablePointerSelection
          onKeyDown={(e) => {
            if (e.key === 'Escape') {
              setIsPopoverOpen(false);
            }
          }}
          onValueChange={onValueChange}
          value={value}
        >
          <CommandInput placeholder="Search..." />
          <CommandEmpty className="text-text-secondary py-5 text-center text-sm">
            No results found.
          </CommandEmpty>
          <CommandList className="flex max-h-full flex-col overflow-y-auto">
            <CommandGroup
              className="py-4"
              heading={
                <h3 className="font-clash text-sm font-medium text-white">
                  Agents
                </h3>
              }
            >
              {isAgentsSuccess &&
                agents.map((agent) => (
                  <CommandItem
                    className="hover:bg-bg-secondary data-[selected='true']:bg-bg-secondary flex cursor-pointer items-center justify-between gap-2 rounded-md px-2 py-2 text-white transition-colors"
                    key={agent.agent_id}
                    onSelect={() => {
                      setIsPopoverOpen(false);
                    }}
                    value={agent.agent_id}
                  >
                    <div className="inline-flex items-center gap-3">
                      <AIAgentIcon name={agent.name} size="xs" />
                      <span className="inline-flex items-center gap-1.5 text-sm capitalize">
                        {agent.name}
                      </span>
                    </div>
                  </CommandItem>
                ))}
            </CommandGroup>
            <CommandSeparator className="" />
            <CommandGroup
              className="py-4"
              heading={
                <h3 className="font-clash text-sm font-medium text-white">
                  AI Models
                </h3>
              }
            >
              {isLlmProviderSuccess &&
                llmProviders?.length > 0 &&
                llmProviders?.map((llmProvider) => (
                  <CommandItem
                    className="hover:bg-bg-secondary data-[selected='true']:bg-bg-secondary flex cursor-pointer items-start gap-2 rounded-md px-2 py-2 text-white transition-colors"
                    key={llmProvider.id}
                    onSelect={() => {
                      setIsPopoverOpen(false);
                    }}
                    value={llmProvider.id}
                  >
                    <div className="bg-bg-secondary border-divider flex size-6 shrink-0 items-center justify-center gap-2 rounded-lg border p-2">
                      <ProviderIcon
                        className="mt-0.5 size-4 shrink-0"
                        provider={llmProvider.model.split(':')[0]}
                      />
                    </div>
                    <span className="text-sm capitalize">
                      {formatText(llmProvider?.name || llmProvider.id || '')}
                    </span>
                  </CommandItem>
                ))}
            </CommandGroup>
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  );
}

export const AIModelSelector = memo(AIModelSelectorBase);

export function AiUpdateSelectionBase({ inboxId }: { inboxId: string }) {
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
export const AiUpdateSelection = memo(
  AiUpdateSelectionBase,
  (prevProps, nextProps) => prevProps.inboxId === nextProps.inboxId,
);
