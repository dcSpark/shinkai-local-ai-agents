import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from '@shinkai_network/shinkai-ui';
import { type ReactNode } from 'react';

import { useSettings } from '../../store/settings';

type PlaygroundToolLayoutProps = {
  leftElement: ReactNode;
  rightElement?: ReactNode;
  rightTopElement?: ReactNode;
  rightBottomElement?: ReactNode;
  topElement: ReactNode;
};

export default function PlaygroundToolLayout({
  topElement,
  leftElement,
  rightElement,
  rightTopElement,
  rightBottomElement,
}: PlaygroundToolLayoutProps) {
  const playgroundChatPanelSize = useSettings(
    (state) => state.playgroundChatPanelSize,
  );

  const playgroundCodePanelSize = useSettings(
    (state) => state.playgroundCodePanelSize,
  );

  const setPlaygroundChatPanelSize = useSettings(
    (state) => state.setPlaygroundChatPanelSize,
  );

  const setPlaygroundCodePanelSize = useSettings(
    (state) => state.setPlaygroundCodePanelSize,
  );

  return (
    <div className="bg-bg-dark flex h-full flex-col pt-4">
      {topElement}
      <div className="flex h-full" style={{ contain: 'strict' }}>
        <ResizablePanelGroup direction="horizontal">
          <ResizablePanel
            className="flex h-full flex-col rounded-xs pb-3"
            defaultSize={playgroundChatPanelSize}
            minSize={10}
            onResize={(size) => {
              setPlaygroundChatPanelSize(size);
            }}
          >
            {leftElement}
          </ResizablePanel>
          <ResizableHandle className="bg-bg-darker w-2" />
          <ResizablePanel
            className="flex h-full flex-col overflow-hidden rounded-xs pb-3"
            minSize={10}
          >
            {rightElement ?? (
              <div className={'flex flex-grow justify-stretch'}>
                <div className="flex size-full flex-col">
                  <ResizablePanelGroup direction="vertical">
                    <ResizablePanel
                      defaultSize={playgroundCodePanelSize}
                      minSize={3}
                      onResize={(size) => {
                        setPlaygroundCodePanelSize(size);
                      }}
                    >
                      {rightTopElement}
                    </ResizablePanel>
                    <ResizableHandle className="bg-bg-darker !h-2" />
                    <ResizablePanel minSize={3}>
                      {rightBottomElement}
                    </ResizablePanel>
                  </ResizablePanelGroup>
                </div>
              </div>
            )}
          </ResizablePanel>
        </ResizablePanelGroup>
      </div>
    </div>
  );
}
