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
import {
  ToolsDisabledIcon,
  ToolsIcon,
} from '@shinkai_network/shinkai-ui/assets';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import { memo } from 'react';
import { useParams } from 'react-router';
import { toast } from 'sonner';

import { useAuth } from '../../../store/auth';
import { actionButtonClassnames } from '../conversation-footer';

interface ToolsSwitchActionBarProps {
  checked: boolean;
  disabled?: boolean;
  onClick: () => void;
}

function ToolsSwitchActionBarBase({
  disabled,
  checked,
  onClick,
}: ToolsSwitchActionBarProps) {
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
            )}
            disabled={disabled}
            onClick={onClick}
            type="button"
          >
            {checked ? (
              <ToolsIcon className="size-4" />
            ) : (
              <ToolsDisabledIcon className="size-4" />
            )}
            <span>{t('tools.label')}</span>
          </button>
        </TooltipTrigger>
        <TooltipPortal>
          <TooltipContent>
            {checked ? 'Disable' : 'Enable'} AI Actions (Tools)
          </TooltipContent>
        </TooltipPortal>
      </Tooltip>
    </TooltipProvider>
  );
}

export const ToolsSwitchActionBar = memo(
  ToolsSwitchActionBarBase,
  (prevProps, nextProps) => prevProps.checked === nextProps.checked,
);

export function UpdateToolsSwitchActionBarBase() {
  const auth = useAuth((state) => state.auth);
  const { inboxId: encodedInboxId = '' } = useParams();
  const inboxId = decodeURIComponent(encodedInboxId);

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
      toast.error('Use tools update failed', {
        description: error.response?.data?.message ?? error.message,
      });
    },
  });

  const handleUpdateTool = async () => {
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
        use_tools: !chatConfig?.use_tools,
        thinking: chatConfig?.thinking,
        reasoning_effort: chatConfig?.reasoning_effort,
        web_search_enabled: chatConfig?.web_search_enabled,
      },
    });
  };

  return (
    <ToolsSwitchActionBar
      checked={!!chatConfig?.use_tools}
      disabled={isPending}
      onClick={() => handleUpdateTool()}
    />
  );
}
export const UpdateToolsSwitchActionBar = memo(UpdateToolsSwitchActionBarBase);
