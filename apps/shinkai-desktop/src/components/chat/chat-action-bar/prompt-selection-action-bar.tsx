import {
  Tooltip,
  TooltipContent,
  TooltipPortal,
  TooltipProvider,
  TooltipTrigger,
} from '@shinkai_network/shinkai-ui';
import { PromptLibraryIcon } from '@shinkai_network/shinkai-ui/assets';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import { memo } from 'react';

import { usePromptSelectionStore } from '../../prompt/context/prompt-selection-context';
import { actionButtonClassnames } from '../conversation-footer';

function PromptSelectionActionBarBase({
  disabled,
  showLabel,
  shortcut,
}: {
  disabled?: boolean;
  showLabel?: boolean;
  shortcut?: string;
}) {
  const setPromptSelectionDrawerOpen = usePromptSelectionStore(
    (state) => state.setPromptSelectionDrawerOpen,
  );

  if (!showLabel) {
    return (
      <TooltipProvider delayDuration={0}>
        <Tooltip>
          <TooltipTrigger asChild>
            <button
              className={cn(actionButtonClassnames)}
              disabled={disabled}
              onClick={() => {
                setPromptSelectionDrawerOpen(true);
              }}
              type="button"
            >
              <PromptLibraryIcon className="h-full w-full" />
            </button>
          </TooltipTrigger>
          <TooltipPortal>
            <TooltipContent align="center" side="top">
              Prompt Library
              {shortcut && (
                <span className="text-text-secondary ml-2 text-xs">
                  {shortcut}
                </span>
              )}
            </TooltipContent>
          </TooltipPortal>
        </Tooltip>
      </TooltipProvider>
    );
  }
  return (
    <button
      className={cn(
        actionButtonClassnames,
        'w-full justify-between gap-2.5',
        shortcut && 'pr-2',
      )}
      disabled={disabled}
      onClick={() => {
        setPromptSelectionDrawerOpen(true);
      }}
      type="button"
    >
      <div className="flex items-center gap-2.5">
        <PromptLibraryIcon className="size-4" />
        <span>Prompt Library</span>
      </div>
      {shortcut && (
        <span className="text-text-secondary ml-auto text-[10px]">
          {shortcut}
        </span>
      )}
    </button>
  );
}

const PromptSelectionActionBar = memo(
  PromptSelectionActionBarBase,
  (prevProps, nextProps) => {
    return prevProps.disabled === nextProps.disabled;
  },
);
export default PromptSelectionActionBar;
