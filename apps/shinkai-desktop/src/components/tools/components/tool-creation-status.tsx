// ToolCreationStatus.tsx
import { Skeleton } from '@shinkai_network/shinkai-ui';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import { AnimatePresence, motion } from 'framer-motion';
import { LoaderIcon } from 'lucide-react';
import { memo } from 'react';

import { ToolCreationState } from '../../playground-tool/context/playground-context';
import { type ToolCreationStepStatus } from '../../playground-tool/hooks/use-tool-flow';
import ToolCodeEditor from '../../playground-tool/tool-code-editor';

export const getRandomWidth = () => {
  const widths = [
    'w-12',
    'w-16',
    'w-20',
    'w-24',
    'w-32',
    'w-40',
    'w-48',
    'w-56',
    'w-64',
    'w-72',
  ];
  return widths[Math.floor(Math.random() * widths.length)];
};

const LoadingSkeleton = memo(() => (
  <motion.div
    animate={{ opacity: 1 }}
    className="size-w flex flex-1 flex-col items-start gap-1 overflow-hidden rounded-md px-4 py-4 text-xs"
    exit={{ opacity: 0 }}
    initial={{ opacity: 0 }}
    transition={{ duration: 0.5, ease: 'easeInOut' }}
  >
    {[...Array(20)].map((_, lineIndex) => (
      <div className="mb-2 flex gap-3" key={lineIndex}>
        <Skeleton className="bg-bg-default h-4 w-12 rounded" />
        <div className="flex-1">
          <div className="flex flex-wrap gap-2">
            {[...Array(Math.floor(Math.random() * 4) + 1)].map(
              (_, blockIndex) => (
                <Skeleton
                  className={cn(getRandomWidth(), 'bg-bg-default h-4 rounded')}
                  key={blockIndex}
                />
              ),
            )}
          </div>
        </div>
      </div>
    ))}
  </motion.div>
));

LoadingSkeleton.displayName = 'LoadingSkeleton';

const StatusIndicator = memo(
  ({
    status,
    pendingText,
    successText,
    errorText,
  }: {
    status: ToolCreationStepStatus;
    pendingText: string;
    successText: string;
    errorText: string;
  }) => (
    <div className="flex items-center gap-3">
      {status === 'pending' ? (
        <LoaderIcon className="size-4 animate-spin text-cyan-500" />
      ) : status === 'success' ? (
        <div className="flex h-5 w-5 items-center justify-center rounded-full bg-cyan-500/20 text-cyan-500">
          ✓
        </div>
      ) : status === 'error' ? (
        <div className="flex h-5 w-5 items-center justify-center rounded-full bg-red-500/20 text-red-500">
          ✗
        </div>
      ) : null}
      <h3 className="font-medium text-zinc-100">
        {status === 'pending'
          ? pendingText
          : status === 'error'
            ? errorText
            : status === 'success'
              ? successText
              : null}
      </h3>
    </div>
  ),
);

StatusIndicator.displayName = 'StatusIndicator';

const ErrorDisplay = memo(
  ({ error, message }: { error: string; message: string }) => (
    <motion.div
      animate={{ opacity: 1 }}
      className="flex size-full flex-1 flex-col items-start gap-1 overflow-auto rounded-md px-4 py-4 text-xs"
      exit={{ opacity: 0 }}
      initial={{ opacity: 0 }}
      transition={{ duration: 0.5, ease: 'easeInOut' }}
    >
      <p className="text-red-500">{message}</p>
      <p className="break-words text-red-500">{error}</p>
    </motion.div>
  ),
);

ErrorDisplay.displayName = 'ErrorDisplay';

const CodeGenerationStep = memo(
  ({
    toolCodeStatus,
    toolCode,
    error,
    language,
  }: {
    toolCodeStatus: ToolCreationStepStatus;
    toolCode: string;
    error: string;
    language: string;
  }) => (
    <motion.div
      animate={{ y: 0, opacity: 1 }}
      className="border-divider bg-bg-dark flex h-full w-full flex-1 flex-col gap-3 overflow-hidden rounded-lg p-3"
      exit={{ y: -100, opacity: 0 }}
      initial={{ y: 100, opacity: 0, rotateX: -20 }}
      key={toolCodeStatus}
      transition={{ duration: 0.5, ease: 'easeInOut' }}
    >
      <StatusIndicator
        errorText="Generating Code Failed"
        pendingText="Generating Code..."
        status={toolCodeStatus}
        successText="Code Generated"
      />

      <AnimatePresence mode="wait">
        {toolCodeStatus === 'pending' && <LoadingSkeleton />}

        {toolCodeStatus === 'success' && (
          <motion.div
            animate={{ opacity: 1 }}
            className={cn('flex-1 overflow-auto rounded-md')}
            exit={{ opacity: 0 }}
            initial={{ opacity: 0 }}
            transition={{ duration: 0.5, ease: 'easeInOut' }}
          >
            <ToolCodeEditor
              language={language.toLowerCase()}
              readOnly
              value={toolCode}
            />
          </motion.div>
        )}

        {toolCodeStatus === 'error' && (
          <ErrorDisplay error={error} message="Failed to generate code" />
        )}
      </AnimatePresence>
    </motion.div>
  ),
);

CodeGenerationStep.displayName = 'CodeGenerationStep';

const MetadataGenerationStep = memo(
  ({
    toolMetadataStatus,
    toolMetadataString,
    error,
  }: {
    toolMetadataStatus: ToolCreationStepStatus;
    toolMetadataString: string;
    error: string;
  }) => (
    <motion.div
      animate={{ y: 0, opacity: 1, rotateX: 0 }}
      className="border-divider bg-bg-dark flex h-full w-full flex-1 flex-col gap-3 overflow-hidden rounded-lg p-3"
      exit={{ y: -100, opacity: 0, rotateX: 20 }}
      initial={{ y: 100, opacity: 0, rotateX: -20 }}
      key={toolMetadataStatus}
      transition={{ duration: 0.5, ease: 'easeInOut' }}
    >
      <div className="flex items-center gap-3">
        <div className="flex items-center gap-2">
          {toolMetadataStatus === 'pending' && (
            <>
              <LoaderIcon className="size-4 animate-spin text-cyan-500" />
              <h3 className="font-medium text-zinc-100">
                Generating Preview...
              </h3>
            </>
          )}
          {toolMetadataStatus === 'success' && (
            <>
              <div className="flex h-5 w-5 items-center justify-center rounded-full bg-cyan-500/20 text-cyan-500">
                <span className="font-bold">✓</span>
              </div>
              <h3 className="font-medium text-zinc-100">
                Metadata Successfully Generated
              </h3>
            </>
          )}
          {toolMetadataStatus === 'error' && (
            <h3 className="font-medium text-red-500">
              Failed to Generate Metadata
            </h3>
          )}
        </div>
      </div>

      <AnimatePresence mode="wait">
        {toolMetadataStatus === 'pending' && <LoadingSkeleton />}

        {toolMetadataStatus === 'success' && (
          <motion.div
            animate={{ opacity: 1 }}
            className="flex w-full flex-1 flex-col items-start gap-1 overflow-auto rounded-md text-xs"
            exit={{ opacity: 0 }}
            initial={{ opacity: 0 }}
            transition={{ duration: 0.5, ease: 'easeInOut' }}
          >
            <ToolCodeEditor
              language="json"
              readOnly
              style={{
                width: '100%',
              }}
              value={toolMetadataString ?? ''}
            />
          </motion.div>
        )}

        {toolMetadataStatus === 'error' && (
          <ErrorDisplay error={error} message="Failed to generate metadata" />
        )}
      </AnimatePresence>
    </motion.div>
  ),
);

MetadataGenerationStep.displayName = 'MetadataGenerationStep';

const SavingToolStep = memo(({ isSavingTool }: { isSavingTool: boolean }) => (
  <motion.div
    animate={{ y: 0, opacity: 1, rotateX: 0 }}
    className="border-divider bg-bg-dark flex h-full w-full flex-1 flex-col gap-3 overflow-hidden rounded-lg p-3"
    exit={{ y: -100, opacity: 0, rotateX: 20 }}
    initial={{ y: 100, opacity: 0, rotateX: -20 }}
    key={isSavingTool ? 'saving-tool' : 'tool-saved'}
    transition={{ duration: 0.5, ease: 'easeInOut' }}
  >
    <div className="flex items-center gap-3">
      <div className="flex items-center gap-2">
        {isSavingTool ? (
          <>
            <LoaderIcon className="size-4 animate-spin text-cyan-500" />
            <h3 className="font-medium text-zinc-100">Saving Tool...</h3>
          </>
        ) : (
          <>
            <div className="flex h-5 w-5 items-center justify-center rounded-full bg-cyan-500/20 text-cyan-500">
              <span className="font-bold">✓</span>
            </div>
            <h3 className="font-medium text-zinc-100">
              Tool Saved Successfully
            </h3>
          </>
        )}
      </div>
    </div>

    <AnimatePresence mode="wait">
      {isSavingTool && <LoadingSkeleton />}
    </AnimatePresence>
  </motion.div>
));

SavingToolStep.displayName = 'SavingToolStep';
const ToolCreationStatus = memo(
  ({
    currentStep,
    toolCodeStatus,
    toolMetadataStatus,
    toolCode,
    toolMetadataString,
    error,
    language,
    isSavingTool,
  }: {
    currentStep: string;
    toolCodeStatus: ToolCreationStepStatus;
    toolMetadataStatus: ToolCreationStepStatus;
    toolCode: string;
    toolMetadataString: string;
    error: string;
    language: string;
    isSavingTool: boolean;
  }) => {
    return (
      <div className="flex size-full flex-col items-center justify-center gap-1 p-1 text-xs">
        <div className="relative mx-auto h-[400px] w-full max-w-2xl overflow-hidden rounded-lg md:order-2 md:h-[450px] lg:h-[600px]">
          <AnimatePresence mode="wait">
            {currentStep === ToolCreationState.CREATING_CODE && (
              <CodeGenerationStep
                error={error}
                language={language}
                toolCode={toolCode}
                toolCodeStatus={toolCodeStatus}
              />
            )}

            {currentStep === ToolCreationState.CREATING_METADATA && (
              <MetadataGenerationStep
                error={error}
                toolMetadataStatus={toolMetadataStatus}
                toolMetadataString={toolMetadataString}
              />
            )}

            {currentStep === ToolCreationState.SAVING_TOOL && (
              <SavingToolStep isSavingTool={isSavingTool} />
            )}
          </AnimatePresence>
        </div>
      </div>
    );
  },
);
ToolCreationStatus.displayName = 'ToolCreationStatus';
export default ToolCreationStatus;
