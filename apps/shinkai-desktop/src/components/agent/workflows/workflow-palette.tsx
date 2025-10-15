import {
  Tooltip,
  TooltipContent,
  TooltipPortal,
  TooltipTrigger,
} from '@shinkai_network/shinkai-ui';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import { type DragEvent } from 'react';

import { PALETTE_ITEMS } from './constants';
import { type NodePaletteType } from './types';

export type WorkflowPaletteProps = {
  selectedType: NodePaletteType;
  onSelect: (type: NodePaletteType) => void;
  onDragStart: (
    event: DragEvent<HTMLButtonElement>,
    type: NodePaletteType,
  ) => void;
};

const WorkflowPalette = ({
  selectedType,
  onSelect,
  onDragStart,
}: WorkflowPaletteProps) => (
  <div className="pointer-events-none absolute top-2 left-2 z-30 w-64 space-y-4">
    <div className="bg-bg-secondary pointer-events-auto rounded-2xl border-none p-2 backdrop-blur">
      {PALETTE_ITEMS.map((item) => {
        return (
          <div key={item.type}>
            <Tooltip>
              <TooltipTrigger asChild>
                <button
                  aria-label={`${item.label}: ${item.description}`}
                  className={cn(
                    'relative w-full cursor-grab overflow-hidden rounded-2xl px-2 py-1.5 text-left transition-all duration-200 active:cursor-grabbing',
                    'hover:bg-bg-default backdrop-blur-sm',
                  )}
                  draggable
                  onClick={() => onSelect(item.type)}
                  onDragStart={(event) => onDragStart(event, item.type)}
                  type="button"
                >
                  <div className="flex items-center gap-3">
                    <div
                      className="flex h-9 w-9 items-center justify-center rounded-xl"
                      style={{
                        backgroundColor: item.accentSoftColor,
                        color: item.accentColor,
                      }}
                    >
                      {item.icon}
                    </div>
                    <div className="min-w-0 flex-1">
                      <span className="text-text-default block text-sm font-medium">
                        {item.label}
                      </span>
                    </div>
                  </div>
                </button>
              </TooltipTrigger>
              <TooltipPortal>
                <TooltipContent align="center" side="right">
                  {item.description}
                </TooltipContent>
              </TooltipPortal>
            </Tooltip>
          </div>
        );
      })}
    </div>
  </div>
);

export default WorkflowPalette;
