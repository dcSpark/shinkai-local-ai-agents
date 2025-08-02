import { CodeLanguage } from '@shinkai_network/shinkai-message-ts/api/tools/types';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
  Tooltip,
  TooltipContent,
  TooltipPortal,
  TooltipProvider,
  TooltipTrigger,
} from '@shinkai_network/shinkai-ui';
import { PythonIcon, TypeScriptIcon } from '@shinkai_network/shinkai-ui/assets';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import { ChevronDownIcon } from 'lucide-react';

import { actionButtonClassnames } from '../../chat/conversation-footer';

export const LANGUAGE_TOOLS = [
  {
    label: CodeLanguage.Python,
    value: CodeLanguage.Python,
    icon: <PythonIcon className="size-4 text-white" />,
  },
  {
    label: CodeLanguage.Typescript,
    value: CodeLanguage.Typescript,
    icon: <TypeScriptIcon className="size-4 text-white" />,
  },
];

export function LanguageToolSelector({
  value,
  onValueChange,
}: {
  value: string;
  onValueChange: (value: string) => void;
}) {
  return (
    <DropdownMenu>
      <TooltipProvider delayDuration={0}>
        <Tooltip>
          <TooltipTrigger asChild>
            <DropdownMenuTrigger
              className={cn(
                actionButtonClassnames,
                'flex w-auto items-center justify-between gap-2 truncate [&[data-state=open]>.icon]:rotate-180',
              )}
            >
              <div className="flex items-center gap-1">
                {
                  (
                    LANGUAGE_TOOLS.find(
                      (language) => language.value === value,
                    ) ?? LANGUAGE_TOOLS[0]
                  ).icon
                }
                {value}
              </div>
              <ChevronDownIcon className="icon h-3 w-3" />
            </DropdownMenuTrigger>
          </TooltipTrigger>
          <TooltipPortal>
            <TooltipContent align="center" side="top">
              Switch code language
            </TooltipContent>
          </TooltipPortal>
          <DropdownMenuContent
            align="start"
            className="min-w-[200px] overflow-y-auto p-1 py-2"
            side="top"
          >
            <DropdownMenuRadioGroup onValueChange={onValueChange} value={value}>
              {LANGUAGE_TOOLS.map((language) => (
                <DropdownMenuRadioItem
                  className="flex cursor-pointer items-center justify-between gap-1.5 rounded-md px-2 py-2 text-white transition-colors hover:bg-gray-200 aria-checked:bg-gray-200"
                  key={language.value}
                  value={language.value}
                >
                  <div className="inline-flex gap-1.5">
                    {language.icon}
                    <div className="flex flex-col gap-1">
                      <span className="text-xs">{language.label}</span>
                    </div>
                  </div>
                </DropdownMenuRadioItem>
              ))}
            </DropdownMenuRadioGroup>
          </DropdownMenuContent>
        </Tooltip>
      </TooltipProvider>
    </DropdownMenu>
  );
}
