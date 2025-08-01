import { type ToolMetadata } from '@shinkai_network/shinkai-message-ts/api/tools/types';
import { extractJobIdFromInbox } from '@shinkai_network/shinkai-message-ts/utils';
import { usePublishTool } from '@shinkai_network/shinkai-node-state/v2/mutations/publishTool/usePublishTool';
import { useRestoreToolConversation } from '@shinkai_network/shinkai-node-state/v2/mutations/restoreToolConversation/useRestoreToolConversation';
import {
  Button,
  Popover,
  PopoverContent,
  PopoverTrigger,
  TooltipContent,
  TooltipPortal,
  TooltipTrigger,
  Badge,
  Separator,
  Tooltip,
} from '@shinkai_network/shinkai-ui';
import { StoreIcon } from '@shinkai_network/shinkai-ui/assets';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import {
  ArrowLeft,
  Loader2,
  Redo2Icon,
  SaveIcon,
  Undo2Icon,
} from 'lucide-react';
import { memo } from 'react';
import { useFormContext } from 'react-hook-form';
import { useNavigate, useParams } from 'react-router';
import { toast } from 'sonner';

import { useAuth } from '../../../store/auth';
import { SHINKAI_STORE_URL } from '../../../utils/store';
import { DockerStatus } from '../../tools/components/docker-status';
import { usePlaygroundStore } from '../context/playground-context';
import { type CreateToolCodeFormSchema } from '../hooks/use-tool-code';
import { useToolSave } from '../hooks/use-tool-save';
import EditToolBasicInfoDialog from './edit-tool-basic-info-dialog';
import { ManageSourcesButton } from './manage-sources-button';

function PlaygroundHeaderBase({
  toolHistory,
  toolName,
  toolDescription,
  baseToolCodeRef,
  initialToolRouterKeyWithVersion,
}: {
  toolHistory: {
    messageId: string;
    code: string;
  }[];
  toolName: string;
  toolDescription: string;
  baseToolCodeRef: React.RefObject<string>;
  initialToolRouterKeyWithVersion: string;
}) {
  const auth = useAuth((state) => state.auth);
  const toolCode = usePlaygroundStore((state) => state.toolCode);
  const setToolCode = usePlaygroundStore((state) => state.setToolCode);
  const chatInboxId = usePlaygroundStore((state) => state.chatInboxId);
  const setResetCounter = usePlaygroundStore((state) => state.setResetCounter);
  const resetPlaygroundStore = usePlaygroundStore(
    (state) => state.resetPlaygroundStore,
  );

  const toolMetadata = usePlaygroundStore((state) => state.toolMetadata);
  const form = useFormContext<CreateToolCodeFormSchema>();
  const codeEditorRef = usePlaygroundStore((state) => state.codeEditorRef);

  const navigate = useNavigate();
  const { toolRouterKey } = useParams();

  const { handleSaveTool, isSavingTool } = useToolSave();

  const handleSaveChanges = async () => {
    await handleSaveTool({
      toolName: toolName,
      toolDescription: toolDescription,
      toolMetadata: toolMetadata as ToolMetadata,
      toolCode: codeEditorRef?.current?.value ?? '',
      tools: form.getValues('tools'),
      language: form.getValues('language'),
      onSuccess: () => {
        toast.success('Your tool has been saved successfully!');
      },
    });
  };

  const {
    mutateAsync: restoreToolConversation,
    isPending: isRestoringToolConversation,
  } = useRestoreToolConversation({
    onSuccess: () => {
      toast.success('Successfully restore changes');
    },
    onError: (error) => {
      toast.error('Failed to restore tool conversation', {
        position: 'top-right',
        description: error.response?.data?.message ?? error.message,
      });
    },
  });

  const { mutateAsync: publishTool, isPending: isPublishingTool } =
    usePublishTool({
      onSuccess: (response) => {
        open(
          `${SHINKAI_STORE_URL}/store/revisions/complete?id=${response.response.revisionId}`,
        );
      },
      onError: (error) => {
        toast.error('Failed to publish tool', {
          description: error.response?.data?.message ?? error.message,
        });
      },
    });

  const restoreCode = async () => {
    const currentIdx = toolHistory.findIndex(
      (history) => history.code === toolCode,
    );

    await restoreToolConversation({
      token: auth?.api_v2_key ?? '',
      nodeAddress: auth?.node_address ?? '',
      jobId: chatInboxId ? extractJobIdFromInbox(chatInboxId ?? '') : '',
      messageId: toolHistory[currentIdx].messageId,
    });
  };

  const goPreviousToolCode = () => {
    const currentIdx = toolHistory.findIndex(
      (history) => history.code === toolCode,
    );
    const prevTool = toolHistory[currentIdx - 1];

    const messageEl = document.getElementById(prevTool.messageId);
    baseToolCodeRef.current = prevTool.code;
    setToolCode(prevTool.code);
    setResetCounter((prev) => prev + 1);
    if (messageEl) {
      // wait til requestAnimationFrame for scrolling
      setTimeout(() => {
        messageEl.scrollIntoView({
          behavior: 'smooth',
          block: 'start',
        });
      }, 100);
    }
  };

  const goNextToolCode = () => {
    const currentIdx = toolHistory.findIndex(
      (history) => history.code === toolCode,
    );
    const nextTool = toolHistory[currentIdx + 1];
    baseToolCodeRef.current = nextTool.code;
    setToolCode(nextTool.code);
    setResetCounter((prev) => prev + 1);

    const messageEl = document.getElementById(nextTool.messageId);
    if (messageEl) {
      setTimeout(() => {
        messageEl.scrollIntoView({
          behavior: 'smooth',
          block: 'start',
        });
      }, 100);
    }
  };

  return (
    <div className="border-divider flex items-center justify-between gap-2 border-b px-4 pb-2.5">
      <div className="flex items-center gap-2">
        <Button
          className="text-text-secondary border-none"
          onClick={() => {
            resetPlaygroundStore();
            void navigate('/tools');
          }}
          rounded="lg"
          size="xs"
          type="button"
          variant="outline"
        >
          <ArrowLeft className="size-4" />
        </Button>

        <EditToolBasicInfoDialog
          initialToolRouterKeyWithVersion={initialToolRouterKeyWithVersion}
          toolDescription={toolDescription}
          toolName={toolName}
        />
      </div>

      <div className="flex items-center justify-end gap-2.5">
        <DockerStatus />
        {toolHistory.length > 1 && (
          <div className="flex items-center gap-4">
            {toolCode === toolHistory?.at(-1)?.code ? (
              <Tooltip>
                <TooltipTrigger asChild>
                  <Badge
                    className="border-cyan-600 bg-cyan-900/20 font-normal text-cyan-400"
                    variant="inputAdornment"
                  >
                    Latest
                  </Badge>
                </TooltipTrigger>
                <TooltipPortal>
                  <TooltipContent side="bottom">
                    <p>This is your latest version</p>
                  </TooltipContent>
                </TooltipPortal>
              </Tooltip>
            ) : (
              <Tooltip>
                <TooltipTrigger asChild>
                  <Badge
                    className="bg-gray-350 cursor-pointer border-0 px-2.5 py-2 hover:bg-gray-400"
                    onClick={restoreCode}
                    variant="secondary"
                  >
                    {isRestoringToolConversation ? (
                      <Loader2 className={cn('mr-2 h-4 w-4 animate-spin')} />
                    ) : null}
                    <span>Restore</span>
                  </Badge>
                </TooltipTrigger>
                <TooltipPortal>
                  <TooltipContent className="max-w-sm" side="bottom">
                    <p>
                      Restore to this version. This action will undo all changes
                      made since the selected version
                    </p>
                  </TooltipContent>
                </TooltipPortal>
              </Tooltip>
            )}
            <div className="flex items-center gap-2">
              <Tooltip>
                <TooltipTrigger
                  asChild
                  disabled={toolCode === toolHistory?.at(0)?.code}
                >
                  <Button
                    className="size-[30px] rounded-lg p-1 disabled:pointer-events-none disabled:bg-transparent disabled:text-gray-100"
                    onClick={goPreviousToolCode}
                    size="auto"
                    variant="ghost"
                  >
                    <Undo2Icon className="h-4 w-4" />
                  </Button>
                </TooltipTrigger>
                <TooltipPortal>
                  <TooltipContent side="bottom">
                    <p>View previous version</p>
                  </TooltipContent>
                </TooltipPortal>
              </Tooltip>
              <Tooltip>
                <TooltipTrigger
                  asChild
                  disabled={toolCode === toolHistory?.at(-1)?.code}
                >
                  <Button
                    className="size-[30px] rounded-lg p-1 disabled:pointer-events-none disabled:bg-transparent disabled:text-gray-100"
                    onClick={goNextToolCode}
                    size="auto"
                    variant="ghost"
                  >
                    <Redo2Icon className="h-4 w-4" />
                  </Button>
                </TooltipTrigger>
                <TooltipPortal>
                  <TooltipContent side="bottom">
                    <p>View next version</p>
                  </TooltipContent>
                </TooltipPortal>
              </Tooltip>
            </div>
            <Separator className="my-1 bg-gray-300" orientation="vertical" />
          </div>
        )}
        <ManageSourcesButton />
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              className="flex h-[34px] w-[42px] p-0"
              isLoading={isSavingTool}
              onClick={handleSaveChanges}
              rounded="lg"
              size="auto"
              type="button"
              variant="outline"
            >
              {!isSavingTool && <SaveIcon className="h-4 w-4" />}
            </Button>
          </TooltipTrigger>
          <TooltipPortal>
            <TooltipContent side="bottom">
              <p>Save Changes</p>
            </TooltipContent>
          </TooltipPortal>
        </Tooltip>
        <Popover>
          <PopoverTrigger asChild>
            <Button
              className="min-w-[100px]"
              disabled={
                !toolCode || !toolMetadata || !chatInboxId || isPublishingTool
              }
              isLoading={isPublishingTool}
              rounded="lg"
              size="xs"
            >
              Publish
            </Button>
          </PopoverTrigger>
          <PopoverContent align="end" className="w-[450px]" sideOffset={10}>
            <div className="space-y-4">
              <div className="space-y-3">
                <h4 className="flex items-center gap-2 leading-none font-medium">
                  <StoreIcon className="h-4 w-4" />
                  Publish to the App Store
                </h4>
                <p className="text-official-gray-300 text-xs">
                  Publishing will make your tool available in the{' '}
                  <a
                    className="text-white underline"
                    href={SHINKAI_STORE_URL}
                    rel="noreferrer"
                    target="_blank"
                  >
                    Shinkai AI Store
                  </a>{' '}
                  . Please ensure it works as expected.
                </p>
              </div>
              {/* create short preview with the tool name and description */}
              <div className="bg-official-gray-900 flex flex-col gap-2 rounded-lg p-4 text-white">
                <h5 className="text-sm leading-none font-medium">
                  {toolMetadata?.name}
                </h5>
                <p className="text-xs">{toolMetadata?.description}</p>
                <p className="text-xs text-white">
                  Author: {auth?.shinkai_identity}
                </p>
              </div>
              <Button
                className="w-full"
                onClick={() => {
                  void publishTool({
                    toolKey: toolRouterKey ?? '',
                    nodeAddress: auth?.node_address ?? '',
                    token: auth?.api_v2_key ?? '',
                  });
                }}
                rounded="lg"
                size="xs"
              >
                Publish
              </Button>
            </div>
          </PopoverContent>
        </Popover>
      </div>
    </div>
  );
}

export const PlaygroundHeader = memo(PlaygroundHeaderBase, (prev, next) => {
  if (prev.toolHistory.length !== next.toolHistory.length) return false;
  if (prev.toolName !== next.toolName) return false;
  if (prev.toolDescription !== next.toolDescription) return false;
  if (
    prev.initialToolRouterKeyWithVersion !==
    next.initialToolRouterKeyWithVersion
  )
    return false;
  return true;
});
