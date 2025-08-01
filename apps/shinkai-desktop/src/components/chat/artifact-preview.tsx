import {
  Button,
  CopyToClipboardIcon,
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
  Tooltip,
  TooltipContent,
  TooltipPortal,
  TooltipProvider,
  TooltipTrigger,
} from '@shinkai_network/shinkai-ui';
import { save } from '@tauri-apps/plugin-dialog';
import * as fs from '@tauri-apps/plugin-fs';
import { BaseDirectory } from '@tauri-apps/plugin-fs';
import { ChevronsRight, DownloadIcon } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { Prism as SyntaxHighlighter } from 'react-syntax-highlighter';
import { oneDark } from 'react-syntax-highlighter/dist/cjs/styles/prism';

import { useChatStore } from './context/chat-context';

const ArtifactPreview = () => {
  const artifact = useChatStore((state) => state.selectedArtifact);
  const setArtifact = useChatStore((state) => state.setSelectedArtifact);

  const contentRef = useRef<HTMLDivElement>(null);
  const iframeRef = useRef<HTMLIFrameElement>(null);
  const [iframeLoaded, setIframeLoaded] = useState(false);

  const handleRender = () => {
    if (!iframeRef.current?.contentWindow) return;

    iframeRef.current?.contentWindow?.postMessage(
      { type: 'UPDATE_COMPONENT', code: artifact?.code },
      '*',
    );
  };

  const handleMessage = (event: any) => {
    if (event?.data?.type === 'INIT_COMPLETE') {
      setIframeLoaded(true);
      handleRender();
    }
  };

  useEffect(() => {
    window.addEventListener('message', handleMessage);
    return () => window.removeEventListener('message', handleMessage);
  }, []);

  useEffect(() => {
    handleRender();
  }, [artifact]);

  return (
    <TooltipProvider delayDuration={0}>
      <Tabs
        className="flex h-screen w-full flex-col overflow-hidden"
        defaultValue="preview"
      >
        <div className={'flex h-screen flex-grow justify-stretch p-3'}>
          <div className="flex size-full flex-col overflow-hidden">
            <div className="flex items-center justify-between gap-2">
              <div className="flex items-center gap-2 px-2">
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button
                      className="text-text-secondary flex items-center gap-2"
                      onClick={() => {
                        setArtifact(null);
                      }}
                      size="icon"
                      variant="tertiary"
                    >
                      <ChevronsRight className="h-4 w-4" />
                      <span className="sr-only">Close Artifact Panel</span>
                    </Button>
                  </TooltipTrigger>
                  <TooltipPortal>
                    <TooltipContent className="flex flex-col items-center gap-1">
                      <p>Close Artifact Panel</p>
                    </TooltipContent>
                  </TooltipPortal>
                </Tooltip>
                <h1 className="line-clamp-1 text-sm font-medium text-white">
                  {artifact?.title}
                </h1>
              </div>
              <TabsList className="grid grid-cols-2 rounded-lg border border-gray-400 bg-transparent p-0.5">
                <TabsTrigger
                  className="flex h-8 items-center gap-1.5 text-xs font-semibold"
                  value="source"
                >
                  Code
                </TabsTrigger>
                <TabsTrigger
                  className="flex h-8 items-center gap-1.5 text-xs font-semibold"
                  value="preview"
                >
                  Preview
                </TabsTrigger>
              </TabsList>
            </div>

            <TabsContent
              className="mt-1 h-full overflow-y-scroll px-4 py-2 font-mono break-words whitespace-pre-line"
              value="source"
            >
              <div className="flex h-10 items-center justify-between gap-3 rounded-t-lg bg-gray-300 pr-3 pl-4">
                {/* by default App.tsx */}
                <h2 className="text-text-secondary text-xs font-semibold">
                  App.tsx
                </h2>
                {iframeLoaded && (
                  <div className="flex items-center gap-2">
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <div>
                          <CopyToClipboardIcon
                            className="text-text-secondary flex h-7 w-7 items-center justify-center rounded-lg border border-gray-200 bg-transparent transition-colors hover:bg-gray-300 hover:text-white [&>svg]:h-3 [&>svg]:w-3"
                            string={artifact?.code ?? ''}
                          />
                        </div>
                      </TooltipTrigger>
                      <TooltipPortal>
                        <TooltipContent className="flex flex-col items-center gap-1">
                          <p>Copy Code</p>
                        </TooltipContent>
                      </TooltipPortal>
                    </Tooltip>
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <button
                          className="text-text-secondary flex h-7 w-7 items-center justify-center rounded-lg border border-gray-200 bg-transparent transition-colors hover:bg-gray-300 hover:text-white [&>svg]:h-3 [&>svg]:w-3"
                          onClick={async () => {
                            const file = new Blob([artifact?.code ?? ''], {
                              type: 'text/plain',
                            });
                            const dataUrl = await new Promise<string>(
                              (resolve) => {
                                const reader = new FileReader();
                                reader.onload = () =>
                                  resolve(reader.result as string);
                                reader.readAsDataURL(file);
                              },
                            );
                            const path = await save({
                              defaultPath: `${artifact?.title?.replace(
                                /[^a-z0-9]/gi,
                                '_',
                              )}.tsx`,
                            });
                            if (path) {
                              const arrayBuffer = await fetch(dataUrl).then(
                                (response) => response.arrayBuffer(),
                              );
                              const content = new Uint8Array(arrayBuffer);
                              await fs.writeFile(path, content, {
                                baseDir: BaseDirectory.Download,
                              });
                            }
                          }}
                        >
                          <DownloadIcon className="h-4 w-4" />
                        </button>
                      </TooltipTrigger>
                      <TooltipPortal>
                        <TooltipContent className="flex flex-col items-center gap-1">
                          <p>Download Code</p>
                        </TooltipContent>
                      </TooltipPortal>
                    </Tooltip>
                  </div>
                )}
              </div>
              <SyntaxHighlighter
                PreTag="div"
                codeTagProps={{ style: { fontSize: '0.8rem' } }}
                customStyle={{
                  margin: 0,
                  width: '100%',
                  padding: '0.5rem 1rem',
                  borderRadius: 0,
                }}
                language={'jsx'}
                style={oneDark}
              >
                {artifact?.code ?? ''}
              </SyntaxHighlighter>
            </TabsContent>
            <TabsContent
              className="h-full w-full flex-grow px-4 py-2"
              value="preview"
            >
              <div className="size-full" ref={contentRef}>
                <iframe
                  className="size-full"
                  loading="lazy"
                  ref={iframeRef}
                  src={'/src/windows/shinkai-artifacts/index.html'}
                />
              </div>
            </TabsContent>
          </div>
        </div>
      </Tabs>
    </TooltipProvider>
  );
};
export default ArtifactPreview;
