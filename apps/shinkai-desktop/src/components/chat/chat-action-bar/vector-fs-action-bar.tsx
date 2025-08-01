import { useTranslation } from '@shinkai_network/shinkai-i18n';
import { extractJobIdFromInbox } from '@shinkai_network/shinkai-message-ts/utils';
import { useUpdateJobScope } from '@shinkai_network/shinkai-node-state/v2/mutations/updateJobScope/useUpdateJobScope';
import { useGetListDirectoryContents } from '@shinkai_network/shinkai-node-state/v2/queries/getDirectoryContents/useGetListDirectoryContents';
import { useGetJobFolderName } from '@shinkai_network/shinkai-node-state/v2/queries/getJobFolderName/useGetJobFolderName';
import { useGetJobScope } from '@shinkai_network/shinkai-node-state/v2/queries/getJobScope/useGetJobScope';
import {
  Badge,
  Tooltip,
  TooltipContent,
  TooltipPortal,
  TooltipTrigger,
} from '@shinkai_network/shinkai-ui';
import {
  DirectoryTypeIcon,
  FilesIcon,
  FileTypeIcon,
} from '@shinkai_network/shinkai-ui/assets';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import { X } from 'lucide-react';
import { useParams } from 'react-router';
import { toast } from 'sonner';

import { useAuth } from '../../../store/auth';
import { useSetJobScope } from '../context/set-job-scope-context';
import { actionButtonClassnames } from '../conversation-footer';

type OpenChatFolderActionBarProps = {
  onClick: () => void;
  disabled?: boolean;
  aiFilesCount?: number;
  showLabel?: boolean;
};

function VectorFsActionBarBase({
  onClick,
  aiFilesCount = 0,
  disabled,
  showLabel,
}: OpenChatFolderActionBarProps) {
  const { t } = useTranslation();
  if (!showLabel) {
    return (
      <>
        <Tooltip>
          <TooltipTrigger asChild>
            <button
              className={cn(
                actionButtonClassnames,
                'w-auto gap-2',
                disabled && 'opacity-50',
                aiFilesCount > 0 && 'bg-cyan-950 hover:bg-cyan-950',
              )}
              disabled={disabled}
              onClick={onClick}
              type="button"
            >
              {aiFilesCount > 0 ? (
                <Badge className="bg-bg-dark text-text-default border-divider inline-flex size-4 items-center justify-center rounded-full p-0 text-center text-[10px]">
                  {aiFilesCount}
                </Badge>
              ) : (
                <FilesIcon className="size-4" />
              )}
              {t('vectorFs.localFiles')}
            </button>
          </TooltipTrigger>
          <TooltipPortal>
            <TooltipContent align="center" side="top">
              {t('vectorFs.localFiles')}
            </TooltipContent>
          </TooltipPortal>
        </Tooltip>
      </>
    );
  }
  return (
    <button
      className={cn(actionButtonClassnames, 'w-full justify-start gap-2.5')}
      disabled={disabled}
      onClick={onClick}
      type="button"
    >
      <FilesIcon className="size-4" />
      <span className="">{t('vectorFs.localFiles')}</span>
      {aiFilesCount > 0 ? (
        <Badge className="bg-bg-dark border-divider text-text-default inline-flex size-4 items-center justify-center rounded-full p-0 text-center text-[10px]">
          {aiFilesCount}
        </Badge>
      ) : null}
    </button>
  );
}

export function UpdateVectorFsActionBar() {
  const auth = useAuth((state) => state.auth);
  const { inboxId: encodedInboxId = '' } = useParams();
  const inboxId = decodeURIComponent(encodedInboxId);
  const setSetJobScopeOpen = useSetJobScope(
    (state) => state.setSetJobScopeOpen,
  );

  const { data: jobScope, isSuccess } = useGetJobScope(
    {
      nodeAddress: auth?.node_address ?? '',
      token: auth?.api_v2_key ?? '',
      jobId: inboxId ? extractJobIdFromInbox(inboxId) : '',
    },
    { enabled: !!inboxId },
  );

  const { data: jobFolderData } = useGetJobFolderName(
    {
      jobId: inboxId ? extractJobIdFromInbox(inboxId) : '',
      nodeAddress: auth?.node_address ?? '',
      token: auth?.api_v2_key ?? '',
    },
    {
      enabled: !!inboxId,
    },
  );

  const { data: fileInfoArray, isSuccess: isVRFilesSuccess } =
    useGetListDirectoryContents(
      {
        nodeAddress: auth?.node_address ?? '',
        token: auth?.api_v2_key ?? '',
        path: decodeURIComponent(jobFolderData?.folder_name ?? '') ?? '',
      },
      {
        enabled: !!jobFolderData?.folder_name,
        retry: 1,
      },
    );

  const hasFilesJobFolder = isVRFilesSuccess && fileInfoArray.length > 0;

  const filesAndFoldersCount = isSuccess
    ? jobScope.vector_fs_folders.length +
      jobScope.vector_fs_items.length +
      (hasFilesJobFolder ? 1 : 0)
    : 0;

  const handleUpdateVectorFs = async () => {
    setSetJobScopeOpen(true);
  };

  return (
    <VectorFsActionBarBase
      aiFilesCount={filesAndFoldersCount}
      onClick={handleUpdateVectorFs}
    />
  );
}

export function VectorFsActionBarPreview() {
  const auth = useAuth((state) => state.auth);
  const { inboxId: encodedInboxId = '' } = useParams();
  const inboxId = decodeURIComponent(encodedInboxId);
  const setSetJobScopeOpen = useSetJobScope(
    (state) => state.setSetJobScopeOpen,
  );

  const { data: jobScope, isSuccess } = useGetJobScope(
    {
      nodeAddress: auth?.node_address ?? '',
      token: auth?.api_v2_key ?? '',
      jobId: inboxId ? extractJobIdFromInbox(inboxId) : '',
    },
    { enabled: !!inboxId },
  );

  const allItems = isSuccess
    ? [
        ...(jobScope?.vector_fs_folders ?? []).map((folder) => ({
          type: 'folder',
          name: folder,
        })),
        ...(jobScope?.vector_fs_items ?? []).map((item) => ({
          type: 'file',
          name: item,
        })),
      ]
    : [];

  const { mutateAsync: updateJobScope, isPending: isUpdatingJobScope } =
    useUpdateJobScope({
      onSuccess: () => {
        setSetJobScopeOpen(false);
      },
      onError: (error) => {
        toast.error('Failed to update conversation context', {
          description: error.response?.data?.message ?? error.message,
        });
      },
    });

  return (
    <>
      {allItems.length > 0 && (
        <div className="no-scrollbar bg-bg-quaternary/10 scroll border-divider h-16 overflow-hidden border-b">
          <div className="flex items-center gap-3 overflow-x-auto p-2.5">
            {allItems.map((item) => (
              <div
                className="border-divider relative flex h-10 w-[180px] shrink-0 items-center gap-1.5 rounded-lg border px-1 py-1.5 pr-2.5"
                key={item.name}
              >
                <div className="flex w-6 shrink-0 items-center justify-center">
                  {item.type === 'file' ? (
                    <FileTypeIcon className="text-text-secondary size-4 shrink-0" />
                  ) : (
                    <DirectoryTypeIcon className="text-text-secondary size-4 shrink-0" />
                  )}
                </div>

                <div className="text-left text-xs">
                  <span className="line-clamp-1 break-all">{item.name}</span>
                </div>
                <button
                  className={cn(
                    'bg-bg-tertiary hover:bg-bg-quaternary text-gray-80 border-divider absolute -top-2 -right-2 h-5 w-5 cursor-pointer rounded-full border p-1 transition-colors hover:text-white',
                    isUpdatingJobScope && 'opacity-50',
                  )}
                  disabled={isUpdatingJobScope}
                  onClick={async () => {
                    const filteredFolders = (
                      jobScope?.vector_fs_folders ?? []
                    ).filter((folder) => folder !== item.name);
                    const filteredFiles = (
                      jobScope?.vector_fs_items ?? []
                    ).filter((file) => file !== item.name);

                    await updateJobScope({
                      jobId: extractJobIdFromInbox(inboxId),
                      nodeAddress: auth?.node_address ?? '',
                      token: auth?.api_v2_key ?? '',
                      jobScope: {
                        vector_fs_items: filteredFiles,
                        vector_fs_folders: filteredFolders,
                      },
                    });
                  }}
                  type="button"
                >
                  <X className="h-full w-full" />
                </button>
              </div>
            ))}
          </div>
        </div>
      )}
    </>
  );
}
export const VectorFsActionBar = VectorFsActionBarBase;
