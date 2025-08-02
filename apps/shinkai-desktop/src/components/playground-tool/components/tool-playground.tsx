import { type ToolMetadata } from '@shinkai_network/shinkai-message-ts/api/tools/types';
import { useCopyToolAssets } from '@shinkai_network/shinkai-node-state/v2/mutations/copyToolAssets/useCopyToolAssets';
import {
  Form,
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from '@shinkai_network/shinkai-ui';
import {
  MetadataIcon,
  PythonIcon,
  TypeScriptIcon,
  UnknownLanguageIcon,
} from '@shinkai_network/shinkai-ui/assets';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import { LoaderIcon } from 'lucide-react';
import { useCallback, useEffect } from 'react';
import { useParams } from 'react-router';

import { useAuth } from '../../../store/auth';
import { usePlaygroundStore } from '../context/playground-context';
import {
  type CreateToolCodeFormSchema,
  useToolForm,
} from '../hooks/use-tool-code';
import { useToolFlow } from '../hooks/use-tool-flow';
import PlaygroundToolLayout from '../layout';
import { detectLanguage } from '../utils/code';
import { CodePanel } from './code-panel';
import { ExecutionPanel } from './execution-panel';
import { MetadataPanel } from './metadata-panel';
import { PlaygroundChat } from './playground-chat';
import { PlaygroundHeader } from './playground-header';

export const getLanguageIcon = (currentLanguage: string) => {
  if (currentLanguage === 'Python') {
    return <PythonIcon className="size-4" />;
  }
  if (currentLanguage === 'TypeScript') {
    return <TypeScriptIcon className="size-4" />;
  }
  return <UnknownLanguageIcon className="size-4" />;
};

export const tabTriggerClassnames = cn(
  'relative flex size-full min-w-[120px] rounded-xs p-0 pt-0.5',
  'data-[state=active]:bg-bg-dark data-[state=active]:text-text-default data-[state=active]:shadow-[0_2px_0_0_var(--color-gray-1000)]',
  'before:absolute before:top-0 before:right-0 before:left-0 before:h-0.5',
  'before:bg-cyan-500 before:opacity-0 before:transition-opacity',
  'data-[focused=true]:before:opacity-100',
);

function PlaygroundToolEditor({
  createToolCodeFormInitialValues,
  toolCodeInitialValues,
  toolMetadataInitialValues,
  initialChatInboxId,
  toolName,
  toolDescription,
  initialToolRouterKeyWithVersion,
}: {
  createToolCodeFormInitialValues?: Partial<CreateToolCodeFormSchema>;
  toolMetadataInitialValues?: {
    metadata: ToolMetadata | null;
    state?: 'idle' | 'pending' | 'success' | 'error';
    error?: string | null;
  };
  toolCodeInitialValues?: {
    code: string;
    state?: 'idle' | 'pending' | 'success' | 'error';
    error?: string | null;
  };
  toolName?: string;
  toolDescription?: string;
  initialChatInboxId?: string;
  initialToolRouterKeyWithVersion?: string;
}) {
  const auth = useAuth((state) => state.auth);
  const { toolRouterKey } = useParams();
  const form = useToolForm(createToolCodeFormInitialValues);

  useEffect(() => {
    if (createToolCodeFormInitialValues?.language) {
      form.setValue('language', createToolCodeFormInitialValues.language);
    }
    if (createToolCodeFormInitialValues?.tools) {
      form.setValue('tools', createToolCodeFormInitialValues.tools);
    }
    if (createToolCodeFormInitialValues?.llmProviderId) {
      form.setValue(
        'llmProviderId',
        createToolCodeFormInitialValues.llmProviderId,
      );
    }
  }, [
    createToolCodeFormInitialValues?.language,
    createToolCodeFormInitialValues?.tools,
    createToolCodeFormInitialValues?.llmProviderId,
    form,
  ]);

  const {
    baseToolCodeRef,
    fetchPreviousPage,
    hasPreviousPage,
    isChatConversationLoading,
    isFetchingPreviousPage,
    isChatConversationSuccess,
    chatConversationData,
    toolHistory,
    executeToolCodeQuery,
    executeToolCode,
    toolResultFiles,
    isDirtyCodeEditor,
    setIsDirtyCodeEditor,
    handleApplyChangesCodeSubmit,
    resetToolCode,
    handleContinueConversation,
    generateMetadata,
    toolMetadata,
    mountTimestamp,
  } = useToolFlow({
    form,
    initialInboxId: initialChatInboxId,
    initialState: {
      code: toolCodeInitialValues?.code,
      metadata: toolMetadataInitialValues?.metadata,
      codeState: toolCodeInitialValues?.state,
      metadataState: toolMetadataInitialValues?.state,
      error: toolCodeInitialValues?.error || toolMetadataInitialValues?.error,
    },
    isPlaygroundMode: true,
  });

  const handleCreateToolCode = useCallback(
    (data: CreateToolCodeFormSchema) => {
      void handleContinueConversation(data.message);
    },
    [handleContinueConversation],
  );

  const toolCode = usePlaygroundStore((state) => state.toolCode);
  const chatInboxId = usePlaygroundStore((state) => state.chatInboxId);
  const xShinkaiAppId = usePlaygroundStore((state) => state.xShinkaiAppId);
  const xShinkaiToolId = usePlaygroundStore((state) => state.xShinkaiToolId);
  const focusedPanel = usePlaygroundStore((state) => state.focusedPanel);
  const setFocusedPanel = usePlaygroundStore((state) => state.setFocusedPanel);
  const toolCodeStatus = usePlaygroundStore((state) => state.toolCodeStatus);
  const isToolCodeGenerationPending = toolCodeStatus === 'pending';
  const isToolMetadataPending = usePlaygroundStore(
    (state) => state.toolMetadataStatus === 'pending',
  );

  // When opening a playground, we need to copy the tool's real assets into the new execution environment
  const { mutateAsync: copyToolAssets } = useCopyToolAssets();
  useEffect(() => {
    void copyToolAssets({
      nodeAddress: auth?.node_address ?? '',
      token: auth?.api_v2_key ?? '',
      xShinkaiAppId,
      currentToolKeyPath: toolRouterKey ?? '',
    });
  }, [
    copyToolAssets,
    auth?.api_v2_key,
    auth?.node_address,
    xShinkaiAppId,
    xShinkaiToolId,
    toolRouterKey,
  ]);

  return (
    <Form {...form}>
      <PlaygroundToolLayout
        topElement={
          <PlaygroundHeader
            toolHistory={toolHistory}
            toolName={toolName ?? ''}
            toolDescription={toolDescription ?? ''}
            baseToolCodeRef={baseToolCodeRef}
            initialToolRouterKeyWithVersion={
              initialToolRouterKeyWithVersion ?? ''
            }
          />
        }
        leftElement={
          <PlaygroundChat
            chatConversationData={chatConversationData}
            chatInboxId={chatInboxId ?? ''}
            fetchPreviousPage={fetchPreviousPage}
            handleCreateToolCode={handleCreateToolCode}
            hasPreviousPage={hasPreviousPage}
            isChatConversationLoading={isChatConversationLoading}
            isChatConversationSuccess={isChatConversationSuccess}
            isFetchingPreviousPage={isFetchingPreviousPage}
            toolName={toolName ?? ''}
          />
        }
        rightTopElement={
          <Tabs className="flex size-full flex-col" defaultValue="code">
            <div className="bg-bg-dark border-divider flex h-8 w-full shrink-0 items-center justify-between gap-2 border-b">
              <TabsList className="grid h-full grid-cols-2 rounded-none bg-transparent p-0">
                <TabsTrigger
                  className={tabTriggerClassnames}
                  value="code"
                  data-focused={focusedPanel === 'code'}
                  onClick={() => setFocusedPanel('code')}
                >
                  <div className="border-divider flex size-full items-center justify-start gap-2 border-r pr-5 pl-3 text-xs font-normal">
                    {isToolCodeGenerationPending ? (
                      <LoaderIcon className="size-4 animate-spin" />
                    ) : (
                      getLanguageIcon(detectLanguage(toolCode))
                    )}
                    Code
                  </div>
                </TabsTrigger>
                <TabsTrigger
                  className={tabTriggerClassnames}
                  value="metadata"
                  data-focused={focusedPanel === 'metadata'}
                  onClick={() => setFocusedPanel('metadata')}
                >
                  <div className="border-divider flex size-full items-center justify-start gap-2 border-r pr-5 pl-3 text-xs font-normal">
                    {isToolMetadataPending ? (
                      <LoaderIcon className="size-4 animate-spin" />
                    ) : (
                      <MetadataIcon className="size-4 text-inherit" />
                    )}
                    Metadata
                  </div>
                </TabsTrigger>
              </TabsList>
            </div>
            <TabsContent
              className="mt-0 flex-1 space-y-4 overflow-y-auto break-words whitespace-pre-line data-[state=inactive]:hidden"
              forceMount
              value="code"
              onFocus={() => setFocusedPanel('code')}
              onBlur={() => setFocusedPanel(null)}
            >
              <CodePanel
                baseToolCodeRef={baseToolCodeRef}
                handleApplyChangesCodeSubmit={handleApplyChangesCodeSubmit}
                isDirtyCodeEditor={isDirtyCodeEditor}
                resetToolCode={resetToolCode}
                setIsDirtyCodeEditor={setIsDirtyCodeEditor}
              />
            </TabsContent>
            <TabsContent
              className="mt-0 flex-1 space-y-4 overflow-y-auto break-words whitespace-pre-line data-[state=inactive]:hidden"
              forceMount
              value="metadata"
              onFocus={() => setFocusedPanel('metadata')}
              onBlur={() => setFocusedPanel(null)}
            >
              <MetadataPanel
                initialToolRouterKeyWithVersion={
                  initialToolRouterKeyWithVersion ?? ''
                }
                initialToolName={toolName ?? ''}
                initialToolDescription={toolDescription ?? ''}
                regenerateToolMetadata={generateMetadata}
                toolMetadata={toolMetadata}
              />
            </TabsContent>
          </Tabs>
        }
        rightBottomElement={
          <ExecutionPanel
            executionToolCodeError={
              executeToolCodeQuery.error?.response?.data?.message ??
              executeToolCodeQuery.error?.message
            }
            handleRunCode={executeToolCode}
            isExecutionToolCodeError={executeToolCodeQuery.isError}
            isExecutionToolCodePending={executeToolCodeQuery.isPending}
            isExecutionToolCodeSuccess={executeToolCodeQuery.isSuccess}
            mountTimestampRef={mountTimestamp}
            regenerateToolMetadata={generateMetadata}
            toolResultFiles={toolResultFiles}
          />
        }
      />
    </Form>
  );
}

export default PlaygroundToolEditor;
