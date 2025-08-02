import { type FormProps } from '@rjsf/core';
import { type FieldProps } from '@rjsf/utils';
import validator from '@rjsf/validator-ajv8';
import { useUploadPlaygroundToolFiles } from '@shinkai_network/shinkai-node-state/v2/mutations/uploadPlaygroundToolFiles/useUploadPlaygroundToolFiles';
import { useGetShinkaiFileProtocol } from '@shinkai_network/shinkai-node-state/v2/queries/getShinkaiFileProtocol/useGetShinkaiFileProtocol';
import {
  Button,
  CommandShortcut,
  FileUploader,
  JsonForm,
  Skeleton,
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@shinkai_network/shinkai-ui';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import equal from 'fast-deep-equal';
import { AnimatePresence, motion } from 'framer-motion';
import { AppWindow, LoaderIcon, TerminalIcon } from 'lucide-react';
import { memo, type RefObject, useEffect, useRef, useState } from 'react';

import { useAuth } from '../../../store/auth';
import { ExecutionFiles } from '../../tools/components/execution-files';
import { usePlaygroundStore } from '../context/playground-context';
import { ToolErrorFallback } from '../error-boundary';
import ToolCodeEditor from '../tool-code-editor';
import { tabTriggerClassnames } from './tool-playground';

function ExecutionPanelBase({
  isExecutionToolCodeSuccess,
  isExecutionToolCodeError,
  isExecutionToolCodePending,
  executionToolCodeError,
  mountTimestampRef,
  toolResultFiles,
  handleRunCode,
  regenerateToolMetadata,
}: {
  isExecutionToolCodeSuccess: boolean;
  isExecutionToolCodeError: boolean;
  isExecutionToolCodePending: boolean;
  executionToolCodeError?: string;
  mountTimestampRef: RefObject<Date>;
  toolResultFiles: string[];
  handleRunCode: FormProps['onSubmit'];
  regenerateToolMetadata: () => void;
}) {
  const viewTabRef = useRef<HTMLDivElement>(null);
  const consoleTabRef = useRef<HTMLDivElement>(null);
  const [formData, setFormData] = useState(null);

  const toolCodeStatus = usePlaygroundStore((state) => state.toolCodeStatus);
  const isToolCodeGenerationPending = toolCodeStatus === 'pending';

  const toolResult = usePlaygroundStore((state) => state.toolResult);

  const toolMetadata = usePlaygroundStore((state) => state.toolMetadata);
  const toolMetadataStatus = usePlaygroundStore(
    (state) => state.toolMetadataStatus,
  );
  const toolMetadataError = usePlaygroundStore(
    (state) => state.toolMetadataError,
  );
  const focusedPanel = usePlaygroundStore((state) => state.focusedPanel);
  const setFocusedPanel = usePlaygroundStore((state) => state.setFocusedPanel);

  const isMetadataGenerationIdle = toolMetadataStatus === 'idle';
  const isMetadataGenerationSuccess = toolMetadataStatus === 'success';
  const isMetadataGenerationError = toolMetadataStatus === 'error';
  const isMetadataGenerationPending = toolMetadataStatus === 'pending';

  useEffect(() => {
    if (
      isExecutionToolCodeSuccess ||
      isExecutionToolCodeError ||
      isExecutionToolCodePending
    ) {
      setTimeout(() => {
        viewTabRef.current?.scrollTo({
          top: viewTabRef.current?.scrollHeight,
          behavior: 'smooth',
        });
      }, 100);
    }
  }, [
    isExecutionToolCodeSuccess,
    isExecutionToolCodeError,
    isExecutionToolCodePending,
  ]);

  return (
    <Tabs className="flex size-full flex-col" defaultValue="view">
      <div className="bg-bg-dark border-divider flex w-full shrink-0 items-center justify-between gap-2 border-b">
        <TabsList className="grid h-8 grid-cols-2 rounded-none bg-transparent p-0">
          <TabsTrigger
            className={cn(tabTriggerClassnames)}
            data-focused={focusedPanel === 'preview'}
            onClick={() => setFocusedPanel('preview')}
            value="view"
          >
            <div className="border-divider flex size-full items-center justify-start gap-2 border-r pr-5 pl-3 text-xs font-normal">
              {isExecutionToolCodePending || isMetadataGenerationPending ? (
                <LoaderIcon className="size-4 animate-spin" />
              ) : (
                <AppWindow className="size-4 text-inherit" />
              )}
              Preview
            </div>
          </TabsTrigger>
          <TabsTrigger
            className={cn(tabTriggerClassnames)}
            data-focused={focusedPanel === 'console'}
            onClick={() => {
              setFocusedPanel('console');

              if (consoleTabRef.current) {
                consoleTabRef.current.scrollTo({
                  top: consoleTabRef.current.scrollHeight,
                  behavior: 'smooth',
                });
              }
            }}
            value="console"
          >
            <div className="border-divider flex size-full items-center justify-start gap-2 border-r pr-5 pl-3 text-xs font-normal">
              <TerminalIcon className="size-4 text-inherit" />
              Console
            </div>
          </TabsTrigger>
        </TabsList>
      </div>
      <TabsContent
        className="mt-0 flex-1 overflow-auto break-words whitespace-pre-line"
        onBlur={() => setFocusedPanel(null)}
        onFocus={() => setFocusedPanel('preview')}
        ref={viewTabRef}
        value="view"
      >
        <div className="flex size-full flex-col pr-3 pb-4 pl-4">
          <div className="flex items-start justify-between gap-8 py-3">
            <div className="text-text-secondary flex items-center gap-1 text-xs">
              {isMetadataGenerationSuccess &&
                !isToolCodeGenerationPending &&
                !isMetadataGenerationError &&
                toolMetadata && (
                  <div className="flex flex-col gap-1">
                    <h1 className="text-text-default text-base font-medium">
                      {toolMetadata.name}
                    </h1>
                    <p className="text-text-secondary text-xs">
                      {toolMetadata.description}
                    </p>
                  </div>
                )}
            </div>
            {isMetadataGenerationSuccess &&
              !isToolCodeGenerationPending &&
              !isMetadataGenerationError && (
                <Button
                  className="text-white"
                  disabled={
                    !isMetadataGenerationSuccess ||
                    isToolCodeGenerationPending ||
                    isMetadataGenerationError
                  }
                  form="parameters-form"
                  isLoading={isExecutionToolCodePending}
                  rounded="lg"
                  size="xs"
                >
                  Run
                  <CommandShortcut className="font-clash flex items-center text-xs text-white">
                    ⌘⏎
                  </CommandShortcut>
                </Button>
              )}
          </div>
          <div className="pb-6">
            {(isMetadataGenerationPending || isToolCodeGenerationPending) && (
              <div className="flex w-full flex-col items-start gap-5 text-xs">
                <div className="w-full space-y-2">
                  <Skeleton className="h-6 w-2/4 animate-pulse rounded" />
                  <Skeleton className="h-10 w-3/4 animate-pulse rounded" />
                </div>
                <div className="w-full space-y-3">
                  <Skeleton className="h-4 w-1/4 animate-pulse rounded" />
                  <Skeleton className="h-8 w-full animate-pulse rounded" />
                  <Skeleton className="h-4 w-1/4 animate-pulse rounded" />
                  <Skeleton className="h-10 w-full animate-pulse rounded" />
                </div>
                <p className="sr-only">Generating Metadata...</p>
              </div>
            )}
            {!isMetadataGenerationPending &&
              !isToolCodeGenerationPending &&
              isMetadataGenerationError && (
                <ToolErrorFallback
                  error={new Error(toolMetadataError ?? '')}
                  resetErrorBoundary={regenerateToolMetadata}
                />
              )}
            {isMetadataGenerationSuccess &&
              !isToolCodeGenerationPending &&
              !isMetadataGenerationError && (
                <div className="text-text-secondary size-full text-xs">
                  <JsonForm
                    className={cn(
                      (toolMetadata?.configurations?.properties &&
                        Object.keys(toolMetadata.configurations.properties)
                          .length > 0) ||
                        (toolMetadata?.parameters?.properties &&
                          Object.keys(toolMetadata.parameters.properties)
                            .length > 0)
                        ? 'py-4'
                        : 'py-0',
                    )}
                    formData={formData}
                    id="parameters-form"
                    noHtml5Validate={true}
                    onChange={(e) => setFormData(e.formData)}
                    onSubmit={handleRunCode}
                    schema={{
                      type: 'object',
                      properties: {
                        ...(toolMetadata?.configurations?.properties &&
                        Object.keys(toolMetadata.configurations.properties)
                          .length > 0
                          ? {
                              configs: toolMetadata.configurations,
                            }
                          : {}),
                        ...(toolMetadata?.parameters?.properties &&
                        Object.keys(toolMetadata.parameters.properties).length >
                          0
                          ? {
                              params: toolMetadata.parameters,
                            }
                          : {}),
                      },
                    }}
                    uiSchema={{
                      'ui:submitButtonOptions': { norender: true },
                      configs: {
                        'ui:title': 'Config',
                      },
                      params: {
                        'ui:title': 'Inputs',
                        ...(toolMetadata?.parameters?.properties
                          ? Object.keys(
                              toolMetadata.parameters.properties,
                            ).reduce<
                              Record<
                                string,
                                { 'ui:widget': typeof FileInputField }
                              >
                            >((acc, key) => {
                              if (key.toLowerCase().includes('file_path')) {
                                acc[key] = {
                                  'ui:widget': FileInputField,
                                };
                              }
                              return acc;
                            }, {})
                          : {}),
                      },
                    }}
                    validator={validator}
                  />

                  <AnimatePresence mode="popLayout">
                    {(isExecutionToolCodePending ||
                      isExecutionToolCodeError ||
                      isExecutionToolCodeSuccess) && (
                      <motion.div
                        animate={{ opacity: 1, x: 0 }}
                        className="flex flex-col pt-2"
                        exit={{ opacity: 0, x: 20 }}
                        initial={{ opacity: 0, x: 20 }}
                      >
                        {isExecutionToolCodePending && (
                          <div className="text-text-secondary flex flex-col items-center gap-2 py-4 text-xs">
                            <LoaderIcon className="shrink-0 animate-spin" />
                            Running Tool...
                          </div>
                        )}
                        {isExecutionToolCodeError && (
                          <div className="mt-2 flex flex-col items-center gap-2 bg-red-900/20 px-3 py-4 text-xs text-red-400">
                            <p>
                              Tool execution failed. Try generating the tool
                              code again.
                            </p>
                            <pre className="px-4 text-center break-words whitespace-break-spaces">
                              {executionToolCodeError}
                            </pre>
                          </div>
                        )}
                        <div>
                          {isExecutionToolCodeSuccess && toolResult && (
                            <ToolResult
                              toolResult={JSON.stringify(toolResult, null, 2)}
                              toolResultFiles={toolResultFiles}
                            />
                          )}
                        </div>
                      </motion.div>
                    )}
                  </AnimatePresence>
                </div>
              )}
            {isMetadataGenerationIdle && !isToolCodeGenerationPending && (
              <div>
                <p className="text-text-secondary py-4 pt-6 text-center text-xs">
                  No metadata generated yet.
                </p>
              </div>
            )}
          </div>
        </div>
      </TabsContent>
      <TabsContent
        className="mt-0 h-full overflow-y-auto break-words whitespace-pre-line"
        onBlur={() => setFocusedPanel(null)}
        onFocus={() => {
          setFocusedPanel('console');
        }}
        ref={consoleTabRef}
        value="console"
      >
        <ToolLogs
          mountTimestamp={mountTimestampRef.current}
          toolResultFiles={toolResultFiles}
        />
      </TabsContent>
    </Tabs>
  );
}

export const ExecutionPanel = memo(
  ExecutionPanelBase,
  (prevProps, nextProps) => {
    if (
      prevProps.isExecutionToolCodePending !==
      nextProps.isExecutionToolCodePending
    )
      return false;
    if (
      prevProps.isExecutionToolCodeError !== nextProps.isExecutionToolCodeError
    )
      return false;
    if (
      prevProps.isExecutionToolCodeSuccess !==
      nextProps.isExecutionToolCodeSuccess
    )
      return false;

    if (!equal(prevProps.toolResultFiles, nextProps.toolResultFiles))
      return false;

    if (prevProps.handleRunCode !== nextProps.handleRunCode) return false;

    return true;
  },
);

const logFileRegex = /log_app-id-\d+_task-id-\d+.log/;

function formatTimestamp(timestamp: string) {
  const year = timestamp.substring(0, 4);
  const month = timestamp.substring(4, 6);
  const day = timestamp.substring(6, 8);
  const hour = timestamp.substring(9, 11);
  const minute = timestamp.substring(11, 13);
  const second = timestamp.substring(13, 15);

  return `${year}-${month}-${day} ${hour}:${minute}:${second}`;
}

const ToolResultBase = ({
  toolResultFiles,
  toolResult,
}: {
  toolResultFiles: string[];
  toolResult: string;
}) => {
  return (
    <div className="flex flex-col gap-4 pb-6">
      <ExecutionFiles files={toolResultFiles} />
      <ToolCodeEditor language="json" readOnly value={toolResult} />
    </div>
  );
};

const ToolResult = memo(ToolResultBase, (prevProps, nextProps) => {
  if (!equal(prevProps.toolResultFiles, nextProps.toolResultFiles))
    return false;
  if (prevProps.toolResult !== nextProps.toolResult) return false;
  return true;
});

const ToolLogsBase = ({
  toolResultFiles,
  mountTimestamp,
}: {
  toolResultFiles: string[];
  mountTimestamp: Date;
}) => {
  const auth = useAuth((state) => state.auth);

  const logsFilePath = toolResultFiles.find((file) => logFileRegex.test(file));
  const [logsFile, setLogsFile] = useState<string | null>(null);

  const { data: logsFileBlob } = useGetShinkaiFileProtocol(
    {
      nodeAddress: auth?.node_address ?? '',
      token: auth?.api_v2_key ?? '',
      file: logsFilePath ?? '',
    },
    {
      enabled: !!logsFilePath,
    },
  );

  useEffect(() => {
    if (logsFileBlob) {
      const handleLogsFile = async () => {
        const logsFile = new Blob([logsFileBlob], {
          type: 'text/plain',
        });
        const logsFileText = await logsFile.text();
        setLogsFile(logsFileText);
      };
      void handleLogsFile();
    }
  }, [logsFileBlob]);

  function formatLogs(logString: string) {
    // Split the logs by line
    const lines = logString.split('\n').filter((line) => line.trim());

    // Find the start and end index of the last shinkai-code-result block
    let lastResultStartIndex = -1;
    let lastResultEndIndex = -1;
    let previousResultEndIndex = -1;

    for (let i = 0; i < lines.length; i++) {
      if (lines[i].includes('<shinkai-code-result>')) {
        if (lastResultStartIndex !== -1) {
          previousResultEndIndex = lastResultEndIndex;
        }
        lastResultStartIndex = i;
      } else if (lines[i].includes('</shinkai-code-result>')) {
        lastResultEndIndex = i;
      }
    }

    if (lastResultStartIndex === -1 || lastResultEndIndex === -1) {
      // No result blocks found, just display all logs
      return processLogLines(lines);
    }

    // Determine the starting index for the current execution's logs
    // This is either after the previous result block or the beginning of the log
    const currentExecutionStartIndex =
      previousResultEndIndex !== -1 ? previousResultEndIndex + 1 : 0;

    // Extract lines from the current execution (non-wrapped logs + the last result)
    const relevantLines = lines.slice(currentExecutionStartIndex);

    return processLogLines(relevantLines);
  }

  // Helper function to process and format log lines
  function processLogLines(lines: string[]) {
    return lines
      .filter((line) => {
        // Skip tag lines, but keep actual content
        if (/<\/?shinkai-code-result>/.test(line)) return false;

        const parts = line.split(',');
        // Need at least timestamp and content
        if (parts.length < 5) return false;

        return true;
      })
      .map((line, i) => {
        const parts = line.split(',');
        const timestamp = parts[0];
        const logContent = parts.slice(4).join(',');
        let readableDate;

        try {
          readableDate = formatTimestamp(timestamp);
        } catch (e) {
          readableDate = timestamp; // Fallback if timestamp formatting fails
        }

        return (
          <div
            className="px-2 py-2 font-mono text-xs hover:bg-gray-500"
            key={i}
          >
            <span className="text-text-secondary">{readableDate} </span>
            <span className="text-gray-50">{logContent}</span>
          </div>
        );
      });
  }

  return (
    <div className="bg-bg-dark rounded-md px-2 py-1 text-gray-50">
      {logsFile ? (
        <div className="space-y-1">{formatLogs(logsFile)}</div>
      ) : (
        <div className="text-text-secondary px-2 py-2 text-left text-xs">
          Results of your code will appear here when you run
        </div>
      )}
    </div>
  );
};

const ToolLogs = memo(ToolLogsBase, (prevProps, nextProps) => {
  return (
    equal(prevProps.toolResultFiles, nextProps.toolResultFiles) &&
    prevProps.mountTimestamp === nextProps.mountTimestamp
  );
});

export const FileInputField = ({ value, onChange }: FieldProps) => {
  const auth = useAuth((state) => state.auth);

  const xShinkaiAppId = usePlaygroundStore((state) => state.xShinkaiAppId);
  const xShinkaiToolId = usePlaygroundStore((state) => state.xShinkaiToolId);

  const { mutateAsync: uploadPlaygroundToolFiles } =
    useUploadPlaygroundToolFiles();

  const [acceptedFiles, setAcceptedFiles] = useState<File[]>([]);

  const handleFileChange = async (files: File[]) => {
    if (files.length === 0) {
      onChange('');
      setAcceptedFiles([]);
      return;
    }

    const { fileContent } = await uploadPlaygroundToolFiles({
      nodeAddress: auth?.node_address ?? '',
      token: auth?.api_v2_key ?? '',
      files,
      xShinkaiAppId,
      xShinkaiToolId,
    });
    setAcceptedFiles(files);
    onChange(fileContent[files[0].name]);
  };

  return (
    <div className="flex flex-col gap-2">
      <FileUploader
        maxFiles={1}
        onChange={handleFileChange}
        value={acceptedFiles}
      />
      {value && (
        <div className="inline-flex items-center gap-2">
          <span className="text-text-secondary">File Path:</span>
          <Tooltip>
            <TooltipTrigger asChild>
              <span className="text-white">{value?.split('/')?.at(-1)}</span>
            </TooltipTrigger>
            <TooltipContent>
              <p>{value}</p>
            </TooltipContent>
          </Tooltip>
        </div>
      )}
    </div>
  );
};
