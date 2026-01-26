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
import { cn } from '@shinkai_network/shinkai-ui/utils';
import { AlertTriangleIcon, SaveIcon } from 'lucide-react';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
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
  const [isDirty, setIsDirty] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [editorResetKey, setEditorResetKey] = useState(0);
  const isInternalSaveRef = useRef(false);

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
      return JSON.stringify(toolMetadata, null, 2);
    }
  }, [toolMetadata]);

  // Track previous metadata to detect external changes vs user edits
  const prevFormattedMetadataRef = useRef(formattedToolMetadata);

  // Reset editor key only when metadata changes externally (not from user save)
  useEffect(() => {
    const hasMetadataChanged =
      formattedToolMetadata !== prevFormattedMetadataRef.current;

    // Only reset if metadata changed externally (not from internal save)
    if (hasMetadataChanged && !isInternalSaveRef.current) {
      setEditorResetKey((prev) => prev + 1);
      setValidateMetadataEditorValue(null);
      setIsDirty(false);
    }

    // Reset the internal save flag after processing
    isInternalSaveRef.current = false;
    prevFormattedMetadataRef.current = formattedToolMetadata;
  }, [formattedToolMetadata]);

  const handleMetadataUpdate = useCallback(
    (value: string) => {
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
          setIsDirty(false);
          return;
        }

        // Validate the schema
        ToolMetadataRawSchema.parse(parsedValue);
        setValidateMetadataEditorValue(null);
        setIsDirty(true);
      } catch (error) {
        if (error instanceof z.ZodError) {
          setValidateMetadataEditorValue(
            'Invalid Metadata schema: ' +
              error.issues.map((issue) => issue.message).join(', '),
          );
          setIsDirty(true);
          return;
        }
        setValidateMetadataEditorValue((error as Error).message);
        setIsDirty(true);
      }
    },
    [toolMetadata],
  );

  const handleSaveMetadata = useCallback(async () => {
    const currentValue = metadataEditorRef.current?.value;
    if (!currentValue) return;

    try {
      const parsedValue = JSON.parse(currentValue);
      const parseValue = ToolMetadataRawSchema.parse(parsedValue);

      const mergedMetadata = merge(parseValue, {
        name: initialToolName,
        description: initialToolDescription,
        author: auth?.shinkai_identity ?? '',
      });

      setIsSaving(true);
      await handleSaveTool({
        toolName: initialToolName,
        toolDescription: initialToolDescription,
        toolMetadata: parseValue as unknown as ToolMetadata,
        toolCode: codeEditorRef.current?.value ?? '',
        tools: form.getValues('tools'),
        language: form.getValues('language'),
        previousToolRouterKeyWithVersion: initialToolRouterKeyWithVersion ?? '',
      });
      // Mark this as an internal save to prevent editor reset
      isInternalSaveRef.current = true;
      updateToolMetadata(mergedMetadata as unknown as ToolMetadata);
      setValidateMetadataEditorValue(null);
      setIsDirty(false);
    } catch (error) {
      if (error instanceof z.ZodError) {
        setValidateMetadataEditorValue(
          'Invalid Metadata schema: ' +
            error.issues.map((issue) => issue.message).join(', '),
        );
        return;
      }
      setValidateMetadataEditorValue((error as Error).message);
    } finally {
      setIsSaving(false);
    }
  }, [
    auth?.shinkai_identity,
    codeEditorRef,
    form,
    handleSaveTool,
    initialToolDescription,
    initialToolName,
    initialToolRouterKeyWithVersion,
    metadataEditorRef,
    updateToolMetadata,
  ]);

  return (
			<div
				className={cn(
					"bg-bg-dark flex h-full flex-col pr-3 pb-4 pl-4",
					validateMetadataEditorValue !== null &&
						"ring-1 ring-red-600 transition-shadow ring-inset",
				)}
			>
				{isMetadataGenerationSuccess && (
					<div
						className={cn("flex items-center justify-end gap-3 px-2 py-1.5")}
					>
						<Tooltip>
							<TooltipTrigger asChild>
								<Button
									className="!size-[28px] rounded-lg border-0 bg-transparent p-2"
									onClick={regenerateToolMetadata}
									size="xs"
									type="button"
									variant="tertiary"
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
						{isDirty && !validateMetadataEditorValue && (
							<Button
								className="h-7 gap-1.5 rounded-lg px-2 text-xs"
								disabled={isSaving}
								onClick={handleSaveMetadata}
								size="xs"
								type="button"
								variant="tertiary"
							>
								{isSaving ? "Applying..." : "Apply changes"}
							</Button>
						)}
					</div>
				)}
				{isMetadataGenerationPending && (
					<div className="text-text-secondary flex flex-col gap-2 py-4 text-xs">
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
							error={new Error(toolMetadataError ?? "")}
							resetErrorBoundary={regenerateToolMetadata}
						/>
					)}

				{isMetadataGenerationSuccess &&
					!isMetadataGenerationError &&
					toolMetadata && (
						<ToolCodeEditor
							key={editorResetKey}
							language="json"
							onUpdate={handleMetadataUpdate}
							ref={metadataEditorRef}
							value={formattedToolMetadata}
						/>
					)}
				{isMetadataGenerationIdle && (
					<div>
						<p className="text-text-secondary py-4 pt-6 text-center text-xs">
							No metadata generated yet.
						</p>
					</div>
				)}
			</div>
		);
}

export const MetadataPanel = MetadataPanelBase;
