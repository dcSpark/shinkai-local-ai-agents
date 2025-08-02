import {
  Tooltip,
  TooltipContent,
  TooltipPortal,
  TooltipTrigger,
} from '@shinkai_network/shinkai-ui';
import {
  WebSearchDisabledIcon,
  WebSearchIcon,
} from '@shinkai_network/shinkai-ui/assets';
import { cn } from '@shinkai_network/shinkai-ui/utils';

import { actionButtonClassnames } from '../conversation-footer';

type OpenChatFolderActionBarProps = {
  onClick: () => void;
  disabled?: boolean;
  checked?: boolean;
  onCheckedChange?: (checked: boolean) => void;
};

export function WebSearchActionBar({
  onClick,
  disabled,
  checked,
}: OpenChatFolderActionBarProps) {
  return (
    <>
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
              <WebSearchIcon className="size-4" />
            ) : (
              <WebSearchDisabledIcon className="size-4" />
            )}
            <span>Web</span>
          </button>
        </TooltipTrigger>
        <TooltipPortal>
          <TooltipContent align="center" side="top">
            {checked ? 'Disable' : 'Enable'} Web Search
          </TooltipContent>
        </TooltipPortal>
      </Tooltip>
    </>
  );
}
