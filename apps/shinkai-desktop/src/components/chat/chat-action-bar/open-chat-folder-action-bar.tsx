import { useTranslation } from '@shinkai_network/shinkai-i18n';
import {
  Tooltip,
  TooltipContent,
  TooltipPortal,
  TooltipTrigger,
} from '@shinkai_network/shinkai-ui';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import { Folder } from 'lucide-react';

import { actionButtonClassnames } from '../conversation-footer';

type OpenChatFolderActionBarProps = {
  onClick: () => void;
  disabled?: boolean;
};

function OpenChatFolderActionBarBase({
  onClick,
  disabled,
}: OpenChatFolderActionBarProps) {
  const { t } = useTranslation();

  return (
    <>
      <Tooltip>
        <TooltipTrigger asChild>
          <button
            className={cn(actionButtonClassnames, 'p-2', {
              'opacity-50': disabled,
            })}
            disabled={disabled}
            onClick={onClick}
            type="button"
          >
            <Folder className="h-full w-full" />
          </button>
        </TooltipTrigger>
        <TooltipPortal>
          <TooltipContent align="center" side="top">
            {t('chat.openChatFolder')}
          </TooltipContent>
        </TooltipPortal>
      </Tooltip>
    </>
  );
}
export const OpenChatFolderActionBar = OpenChatFolderActionBarBase;
