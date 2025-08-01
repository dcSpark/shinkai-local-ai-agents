// import { ToolMetadata } from '@shinkai_network/shinkai-message-ts/api/tools/types';
import { Button, Skeleton } from '@shinkai_network/shinkai-ui';
import { debounce } from '@shinkai_network/shinkai-ui/helpers';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import { AnimatePresence, motion } from 'framer-motion';
import { memo, useCallback } from 'react';

// import { useFormContext } from 'react-hook-form';
import { usePlaygroundStore } from '../context/playground-context';
// import { useAutoSaveTool } from '../hooks/use-create-tool-and-save';
// import { CreateToolCodeFormSchema } from '../hooks/use-tool-code';
import PrismErrorBoundary from '../prism-error-boundary';
import ToolCodeEditor from '../tool-code-editor';
import { detectLanguage } from '../utils/code';

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

function CodePanelBase({
  handleApplyChangesCodeSubmit,
  resetToolCode,
  isDirtyCodeEditor,
  setIsDirtyCodeEditor,
  baseToolCodeRef,
}: {
  handleApplyChangesCodeSubmit: (e: React.FormEvent<HTMLFormElement>) => void;
  resetToolCode: () => void;
  isDirtyCodeEditor: boolean;
  setIsDirtyCodeEditor: (isDirty: boolean) => void;
  baseToolCodeRef: React.RefObject<string>;
}) {
  const codeEditorRef = usePlaygroundStore((state) => state.codeEditorRef);
  const resetCounter = usePlaygroundStore((state) => state.resetCounter);
  // const toolMetadata = usePlaygroundStore((state) => state.toolMetadata);
  const toolCode = usePlaygroundStore((state) => state.toolCode);
  const isToolCodeGenerationPending = usePlaygroundStore(
    (state) => state.toolCodeStatus === 'pending',
  );
  const isToolCodeGenerationSuccess = usePlaygroundStore(
    (state) => state.toolCodeStatus === 'success',
  );
  // const form = useFormContext<CreateToolCodeFormSchema>();

  // const isToolCodeGenerationPending = toolCodeStatus === 'pending';
  // const isToolCodeGenerationSuccess = toolCodeStatus === 'success';

  const handleCodeUpdate = debounce((currentCode: string) => {
    setIsDirtyCodeEditor(currentCode !== baseToolCodeRef.current);
    // handleAutoSave({
    //   toolMetadata: toolMetadata as ToolMetadata,
    //   toolCode: currentCode,
    //   tools: form.getValues('tools'),
    //   language: form.getValues('language'),
    // });
  }, 350);

  const renderCodeElement = useCallback(() => {
    if (isToolCodeGenerationPending) {
      return (
        <div className="flex w-full flex-col items-start gap-1 px-4 py-4 text-xs">
          {[...Array(20)].map((_, lineIndex) => (
            <div className="mb-2 flex gap-3" key={lineIndex}>
              <Skeleton className="h-4 w-12 rounded" />
              <div className="flex-1">
                <div className="flex flex-wrap gap-2">
                  {[...Array(Math.floor(Math.random() * 4) + 1)].map(
                    (_, blockIndex) => (
                      <Skeleton
                        className={cn(getRandomWidth(), 'h-4 rounded')}
                        key={blockIndex}
                      />
                    ),
                  )}
                </div>
              </div>
            </div>
          ))}
          <p className="sr-only">Generating Code...</p>
        </div>
      );
    }

    if (
      !isToolCodeGenerationPending &&
      !toolCode &&
      !isToolCodeGenerationSuccess
    ) {
      return (
        <p className="text-text-secondary pt-6 text-center text-xs">
          No code generated yet. <br />
          Ask Shinkai AI to generate your tool code.
        </p>
      );
    }
    if (isToolCodeGenerationSuccess && toolCode) {
      let detectedLanguage = 'Unknown';
      try {
        detectedLanguage = detectLanguage(toolCode);
      } catch (error) {
        console.error('Error detecting language:', error);
      }

      return (
        <form
          className="flex size-full flex-col"
          key={resetCounter}
          onSubmit={handleApplyChangesCodeSubmit}
        >
          <div className="bg-bg-dark border-divider sticky top-0 z-10 flex h-[40px] shrink-0 items-center justify-between rounded-t-sm border-b px-3 py-2">
            <span className="text-text-secondary inline-flex items-center gap-2 pl-2 text-xs font-medium">
              {' '}
              {detectedLanguage}{' '}
              {isDirtyCodeEditor && (
                <span className="size-2 shrink-0 rounded-full bg-orange-500" />
              )}
            </span>
            <AnimatePresence mode="popLayout">
              {isDirtyCodeEditor && (
                <motion.div
                  animate={{ opacity: 1 }}
                  className="flex items-center justify-end gap-2"
                  exit={{ opacity: 0 }}
                  initial={{ opacity: 0 }}
                  transition={{ duration: 0.2 }}
                >
                  <Button
                    className="!h-[28px] rounded-lg border-0"
                    onClick={resetToolCode}
                    size="xs"
                    variant="tertiary"
                  >
                    Reset
                  </Button>
                  <Button
                    className="!h-[28px] rounded-lg border-0"
                    size="xs"
                    type="submit"
                    variant="tertiary"
                  >
                    Apply Changes
                  </Button>
                </motion.div>
              )}
            </AnimatePresence>
          </div>
          <div className="relative flex-1">
            <PrismErrorBoundary>
              <ToolCodeEditor
                language="ts"
                name="editor"
                onUpdate={handleCodeUpdate}
                ref={codeEditorRef}
                value={toolCode}
              />
            </PrismErrorBoundary>
          </div>
        </form>
      );
    }
  }, [
    codeEditorRef,
    handleApplyChangesCodeSubmit,
    handleCodeUpdate,
    isDirtyCodeEditor,
    isToolCodeGenerationPending,
    isToolCodeGenerationSuccess,
    resetCounter,
    resetToolCode,
    toolCode,
  ]);

  return (
    <div className="flex size-full min-h-[220px] flex-col">
      <div className="flex h-full flex-col">
        {/*{toolCode && (*/}
        {/*  <Tooltip>*/}
        {/*    <TooltipTrigger asChild>*/}
        {/*      <div>*/}
        {/*        <CopyToClipboardIcon*/}
        {/*          className="text-gray-80 flex h-7 w-7 items-center justify-center rounded-lg border border-gray-200 bg-transparent transition-colors hover:bg-gray-300 hover:text-white [&>svg]:h-3 [&>svg]:w-3"*/}
        {/*          string={toolCode ?? ''}*/}
        {/*        />*/}
        {/*      </div>*/}
        {/*    </TooltipTrigger>*/}
        {/*    <TooltipPortal>*/}
        {/*      <TooltipContent className="flex flex-col items-center gap-1">*/}
        {/*        <p>Copy Code</p>*/}
        {/*      </TooltipContent>*/}
        {/*    </TooltipPortal>*/}
        {/*  </Tooltip>*/}
        {/*)}*/}
        {/* </div> */}
        {renderCodeElement()}
      </div>
    </div>
  );
}

export const CodePanel = memo(CodePanelBase, (prevProps, nextProps) => {
  if (prevProps.isDirtyCodeEditor !== nextProps.isDirtyCodeEditor) return false;
  return true;
});
