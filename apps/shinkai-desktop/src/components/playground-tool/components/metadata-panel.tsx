import { ReloadIcon } from '@radix-ui/react-icons';
import { type ToolMetadata } from '@shinkai_network/shinkai-message-ts/api/tools/types';
import {
  Button,
  Skeleton,
  Tooltip,
  TooltipContent,
  TooltipPortal,
  TooltipTrigger,
} from '@shinkai_network/shinkai-ui';
import { debounce } from '@shinkai_network/shinkai-ui/helpers';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import { AlertTriangleIcon } from 'lucide-react';
import { useMemo, useState } from 'react';
import { useFormContext } from 'react-hook-form';
import { merge } from 'ts-deepmerge';
import { z } from 'zod';

import { useAuth } from '../../../store/auth';
import { usePlaygroundStore } from '../context/playground-context';
import { ToolErrorFallback } from '../error-boundary';
import { type CreateToolCodeFormSchema } from '../hooks/use-tool-code';
import { useToolSave } from '../hooks/use-tool-save';
import { ToolMetadataRawSchema } from '../schemas';
import ToolCodeEditor from '../tool-code-editor';

function MetadataPanelBase({
  regenerateToolMetadata,
  initialToolRouterKeyWithVersion,
  initialToolName,
  initialToolDescription,
  toolMetadata,
}: {
  regenerateToolMetadata: () => void;
  initialToolRouterKeyWithVersion: string;
  initialToolName: string;
  initialToolDescription: string;
  toolMetadata: ToolMetadata | null;
}) {
  const [validateMetadataEditorValue, setValidateMetadataEditorValue] =
    useState<string | null>(null);

  const metadataEditorRef = usePlaygroundStore(
    (state) => state.metadataEditorRef,
  );
  const codeEditorRef = usePlaygroundStore((state) => state.codeEditorRef);
  const updateToolMetadata = usePlaygroundStore(
    (state) => state.updateToolMetadata,
  );
  const toolMetadataStatus = usePlaygroundStore(
    (state) => state.toolMetadataStatus,
  );
  const toolMetadataError = usePlaygroundStore(
    (state) => state.toolMetadataError,
  );

  const toolCodeStatus = usePlaygroundStore((state) => state.toolCodeStatus);

  const isToolCodeGenerationPending = toolCodeStatus === 'pending';

  const isMetadataGenerationIdle = toolMetadataStatus === 'idle';
  const isMetadataGenerationSuccess = toolMetadataStatus === 'success';
  const isMetadataGenerationPending = toolMetadataStatus === 'pending';
  const isMetadataGenerationError = toolMetadataStatus === 'error';
  const auth = useAuth((state) => state.auth);
  const form = useFormContext<CreateToolCodeFormSchema>();

  const { handleSaveTool } = useToolSave();

  const handleMetadataUpdate = debounce((value: string) => {
    try {
      const parsedValue = JSON.parse(value);

      const { author, name, description, ...metadataWithoutBasicInfo } =
        toolMetadata ?? {};
      const formattedMetadataWithoutBasicInfo = ToolMetadataRawSchema.parse(
        metadataWithoutBasicInfo,
      );

      if (
        value === JSON.stringify(formattedMetadataWithoutBasicInfo, null, 2)
      ) {
        setValidateMetadataEditorValue(null);
        return;
      }
      const parseValue = ToolMetadataRawSchema.parse(parsedValue);

      const mergedMetadata = merge(parseValue, {
        name: initialToolName,
        description: initialToolDescription,
        author: auth?.shinkai_identity ?? '',
      });

      void handleSaveTool({
        toolName: initialToolName,
        toolDescription: initialToolDescription,
        toolMetadata: parseValue as unknown as ToolMetadata,
        toolCode: codeEditorRef.current?.value ?? '',
        tools: form.getValues('tools'),
        language: form.getValues('language'),
        previousToolRouterKeyWithVersion: initialToolRouterKeyWithVersion ?? '',
      });
      updateToolMetadata(mergedMetadata as unknown as ToolMetadata);
      setValidateMetadataEditorValue(null);
    } catch (error) {
      console.log(error, 'error');
      if (error instanceof z.ZodError) {
        setValidateMetadataEditorValue(
          'Invalid Metadata schema:' +
            error.issues.map((issue) => issue.message).join(', '),
        );
        return;
      }
      setValidateMetadataEditorValue((error as Error).message);
      return;
    }
  }, 750);

  const formattedToolMetadata = useMemo(() => {
    if (!toolMetadata) return '';
    try {
      const parsedToolMetadata = ToolMetadataRawSchema.parse(toolMetadata);
      return JSON.stringify(parsedToolMetadata, null, 2);
    } catch (err) {
      console.error('Error formatting tool metadata:', err, toolMetadata);
      setValidateMetadataEditorValue(
        `Error parsing metadata: ${(err as Error).message}`,
      );
      return JSON.stringify(toolMetadata, null, 2); // Return raw metadata even if schema validation fails
    }
  }, [toolMetadata]);

  return (
    <div
      className={cn(
        'bg-bg-dark flex h-full flex-col pr-3 pb-4 pl-4',
        validateMetadataEditorValue !== null &&
          'ring-1 ring-red-600 transition-shadow ring-inset',
      )}
    >
      {isMetadataGenerationSuccess && (
        <div className={cn('flex items-center justify-end gap-3 px-2 py-1.5')}>
          {validateMetadataEditorValue !== null && (
            <Tooltip>
              <TooltipTrigger asChild>
                <div className="text-red flex items-center">
                  <AlertTriangleIcon className="size-4" />
                </div>
              </TooltipTrigger>
              <TooltipPortal>
                <TooltipContent
                  className="text-text-default flex max-w-[300px] flex-col gap-2.5 text-xs"
                  side="bottom"
                >
                  <p className="font-medium">Invalid metadata format</p>
                  <span className="text-text-tertiary">
                    {validateMetadataEditorValue}
                  </span>
                  <p className="text-text-tertiary text-xs">
                    This value will not be saved.
                  </p>
                </TooltipContent>
              </TooltipPortal>
            </Tooltip>
          )}
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                className="!size-[28px] rounded-lg border-0 bg-transparent p-2"
                onClick={regenerateToolMetadata}
                size="xs"
                type="button"
                variant="ghost"
              >
                <ReloadIcon className="size-full" />
              </Button>
            </TooltipTrigger>
            <TooltipPortal>
              <TooltipContent
                className="text-text-default flex max-w-[300px] flex-col gap-2.5 text-xs"
                side="bottom"
              >
                <p>Regenerate metadata</p>
              </TooltipContent>
            </TooltipPortal>
          </Tooltip>
        </div>
      )}
      {isMetadataGenerationPending && (
        <div className="text-gray-80 flex flex-col gap-2 py-4 text-xs">
          <div className="space-y-3 font-mono text-sm">
            <div className="ml-4 flex items-center gap-2">
              <Skeleton className="h-4 w-24 bg-zinc-800" />
              <Skeleton className="h-4 w-32 bg-zinc-800" />
            </div>
            <div className="ml-4 flex items-center gap-2">
              <Skeleton className="h-4 w-28 bg-zinc-800" />
              <Skeleton className="h-4 w-96 bg-zinc-800" />
            </div>
            <div className="ml-4 flex items-center gap-2">
              <Skeleton className="h-4 w-20 bg-zinc-800" />
              <Skeleton className="h-4 w-4 bg-zinc-800" />
            </div>
            {[...Array(3)].map((_, i) => (
              <div className="ml-8 flex items-center gap-2" key={i}>
                <Skeleton className="h-4 w-28 bg-zinc-800" />
              </div>
            ))}
            <div className="ml-4 flex items-center gap-2">
              <Skeleton className="h-4 w-4 bg-zinc-800" />
            </div>
            <div className="ml-4 flex items-center gap-2">
              <Skeleton className="h-4 w-32 bg-zinc-800" />
              <Skeleton className="h-4 w-4 bg-zinc-800" />
            </div>
            {[...Array(4)].map((_, i) => (
              <div className="ml-8 flex items-center gap-2" key={i}>
                <Skeleton className="h-4 w-40 bg-zinc-800" />
                <Skeleton className="h-4 w-24 bg-zinc-800" />
              </div>
            ))}
            <div className="ml-4 flex items-center gap-2">
              <Skeleton className="h-4 w-28 bg-zinc-800" />
              <Skeleton className="h-4 w-4 bg-zinc-800" />
            </div>
            {[...Array(5)].map((_, i) => (
              <div className="ml-8 flex items-center gap-2" key={i}>
                <Skeleton className="h-4 w-36 bg-zinc-800" />
                {i % 2 === 0 && <Skeleton className="h-4 w-48 bg-zinc-800" />}
              </div>
            ))}
          </div>
          <span className="sr-only">Generating Metadata...</span>
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
        !isMetadataGenerationError &&
        toolMetadata && (
          <ToolCodeEditor
            language="json"
            onUpdate={handleMetadataUpdate}
            ref={metadataEditorRef}
            value={formattedToolMetadata}
          />
        )}
      {isMetadataGenerationIdle && (
        <div>
          <p className="text-gray-80 py-4 pt-6 text-center text-xs">
            No metadata generated yet.
          </p>
        </div>
      )}
    </div>
  );
}

export const MetadataPanel = MetadataPanelBase;
