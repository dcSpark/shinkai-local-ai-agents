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
import { Globe } from 'lucide-react';
import { memo } from 'react';
import { useParams } from 'react-router';
import { toast } from 'sonner';

import { useAuth } from '../../../store/auth';
import { actionButtonClassnames } from '../conversation-footer';

interface WebSearchActionBarProps {
  checked: boolean;
  disabled?: boolean;
  onClick: () => void;
  showLabel?: boolean;
}

function WebSearchActionBarBase({
  disabled,
  checked,
  onClick,
  showLabel = false,
}: WebSearchActionBarProps) {
  const { t } = useTranslation();
  return (
    <TooltipProvider delayDuration={0}>
      <Tooltip>
        <TooltipTrigger asChild>
          <button
            className={cn(
              actionButtonClassnames,
              showLabel && 'w-auto gap-2',
              checked &&
                'bg-gray-900 text-green-400 hover:bg-gray-900 hover:text-green-500',
            )}
            disabled={disabled}
            onClick={onClick}
            type="button"
          >
            <Globe
              className={cn(
                'size-4',
                checked ? 'text-green-400' : 'text-text-secondary',
              )}
            />
            {showLabel && <span>Web Search</span>}
          </button>
        </TooltipTrigger>
        <TooltipPortal>
          <TooltipContent>
            <p className="text-text-secondary mt-1 text-xs">
              {checked 
                ? 'Click to disable Web Search'
                : 'Click to enable Web Search'
              }
            </p>
          </TooltipContent>
        </TooltipPortal>
      </Tooltip>
    </TooltipProvider>
  );
}

export const WebSearchActionBar = memo(
  WebSearchActionBarBase,
  (prevProps, nextProps) =>
    prevProps.checked === nextProps.checked &&
    prevProps.disabled === nextProps.disabled,
);

export function UpdateWebSearchActionBarBase() {
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
      toast.error('Web search update failed', {
        description: error.response?.data?.message ?? error.message,
      });
    },
  });

  const handleUpdateWebSearch = async () => {
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
        thinking: chatConfig?.thinking,
        reasoning_effort: chatConfig?.reasoning_effort,
        web_search_enabled: !chatConfig?.web_search_enabled,
      },
    });
  };

  return (
    <WebSearchActionBar
      checked={!!chatConfig?.web_search_enabled}
      disabled={isPending}
      onClick={() => handleUpdateWebSearch()}
    />
  );
}

export const UpdateWebSearchActionBar = memo(
  UpdateWebSearchActionBarBase,
);