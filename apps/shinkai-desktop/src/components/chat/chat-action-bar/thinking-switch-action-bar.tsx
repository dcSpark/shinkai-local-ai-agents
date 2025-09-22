import { useTranslation } from '@shinkai_network/shinkai-i18n';
import { extractJobIdFromInbox } from '@shinkai_network/shinkai-message-ts/utils/inbox_name_handler';
import { useUpdateChatConfig } from '@shinkai_network/shinkai-node-state/v2/mutations/updateChatConfig/useUpdateChatConfig';
import { useGetChatConfig } from '@shinkai_network/shinkai-node-state/v2/queries/getChatConfig/useGetChatConfig';
import {
  Tooltip,
  TooltipContent,
  TooltipPortal,
  TooltipProvider,
  TooltipTrigger,
} from '@shinkai_network/shinkai-ui';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import { Brain } from 'lucide-react';
import { memo } from 'react';
import { useParams } from 'react-router';
import { toast } from 'sonner';

import { useAuth } from '../../../store/auth';
import { actionButtonClassnames } from '../conversation-footer';

interface ThinkingSwitchActionBarProps {
  checked: boolean;
  disabled?: boolean;
  onClick: () => void;
  forceEnabled?: boolean;
}

function ThinkingSwitchActionBarBase({
  disabled,
  checked,
  onClick,
  forceEnabled = false,
}: ThinkingSwitchActionBarProps) {
  const { t } = useTranslation();
  return (
    <TooltipProvider delayDuration={0}>
      <Tooltip>
        <TooltipTrigger asChild>
          <button
            className={cn(
              actionButtonClassnames,
              'w-auto gap-2',
              checked &&
                'bg-gray-900 text-cyan-400 hover:bg-gray-900 hover:text-cyan-500',
              forceEnabled && 'opacity-75',
            )}
            disabled={disabled}
            onClick={onClick}
            type="button"
          >
            <Brain
              className={cn(
                'size-4',
                checked ? 'text-cyan-400' : 'text-text-secondary',
              )}
            />
            <span>{t('shinkaiNode.models.labels.thinkingCapability')}</span>
          </button>
        </TooltipTrigger>
        <TooltipPortal>
          <TooltipContent>
            <p className="text-text-secondary mt-1 text-xs">
              {forceEnabled
                ? 'Thinking Mode is always enabled for this model and cannot be turned off.'
                : checked
                  ? 'Click to disable AI Thinking Mode'
                  : 'Click to enable AI Thinking Mode'}
            </p>
          </TooltipContent>
        </TooltipPortal>
      </Tooltip>
    </TooltipProvider>
  );
}

export const ThinkingSwitchActionBar = memo(
  ThinkingSwitchActionBarBase,
  (prevProps, nextProps) =>
    prevProps.checked === nextProps.checked &&
    prevProps.disabled === nextProps.disabled &&
    prevProps.forceEnabled === nextProps.forceEnabled,
);

export function UpdateThinkingSwitchActionBarBase({
  forceEnabled = false,
  inboxId,
}: {
  forceEnabled?: boolean;
  inboxId: string;
}) {
  const auth = useAuth((state) => state.auth);

  const { data: chatConfig } = useGetChatConfig(
    {
      nodeAddress: auth?.node_address ?? '',
      token: auth?.api_v2_key ?? '',
      jobId: inboxId ? extractJobIdFromInbox(inboxId) : '',
    },
    { enabled: !!inboxId },
  );

  const { mutateAsync: updateChatConfig, isPending } = useUpdateChatConfig({
    onError: (error) => {
      toast.error('Thinking mode update failed', {
        description: error.response?.data?.message ?? error.message,
      });
    },
  });

  const handleUpdateThinking = async () => {
    if (forceEnabled) return; // Don't allow toggling when forced enabled

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
        use_tools: chatConfig?.use_tools,
        thinking: !chatConfig?.thinking,
        reasoning_effort: chatConfig?.reasoning_effort,
        web_search_enabled: chatConfig?.web_search_enabled,
      },
    });
  };

  return (
    <ThinkingSwitchActionBar
      checked={forceEnabled || !!chatConfig?.thinking}
      disabled={isPending || forceEnabled}
      onClick={() => handleUpdateThinking()}
      forceEnabled={forceEnabled}
    />
  );
}
export const UpdateThinkingSwitchActionBar = memo(
  UpdateThinkingSwitchActionBarBase,
  (prevProps, nextProps) => prevProps.inboxId === nextProps.inboxId,
);
