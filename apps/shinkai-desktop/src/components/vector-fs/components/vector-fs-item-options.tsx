import { useTranslation } from '@shinkai_network/shinkai-i18n';
import { useCopyFsItem } from '@shinkai_network/shinkai-node-state/v2/mutations/copyFsItem/useCopyFsItem';
import { useMoveFsItem } from '@shinkai_network/shinkai-node-state/v2/mutations/moveFsItem/useMoveFsItem';
import { useRemoveFsItem } from '@shinkai_network/shinkai-node-state/v2/mutations/removeFsItem/useRemoveFsItem';
import { useGetDownloadFile } from '@shinkai_network/shinkai-node-state/v2/queries/getDownloadFile/useGetDownloadFile';
import {
  Button,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@shinkai_network/shinkai-ui';
import React, { useEffect } from 'react';
import { toast } from 'sonner';

import { useAuth } from '../../../store/auth';
import { useVectorFsStore } from '../context/vector-fs-context';
import {
  FolderSelectionList,
  useVectorFolderSelectionStore,
} from './folder-selection-list';
import { CreateTextFileAction } from './vector-fs-general-options';

export const VectorFsItemMoveAction = () => {
  const { t } = useTranslation();
  const selectedFile = useVectorFsStore((state) => state.selectedFile);
  const closeDrawerMenu = useVectorFsStore((state) => state.closeDrawerMenu);
  const auth = useAuth((state) => state.auth);
  const setCurrentGlobalPath = useVectorFsStore(
    (state) => state.setCurrentGlobalPath,
  );
  const destinationFolderPath = useVectorFolderSelectionStore(
    (state) => state.destinationFolderPath,
  );

  const { mutateAsync: moveFsItem, isPending: isMovingFsItem } = useMoveFsItem({
    onSuccess: () => {
      setCurrentGlobalPath(destinationFolderPath ?? '/');
      closeDrawerMenu();
      toast.success(t('vectorFs.success.fileMoved'));
    },
    onError: () => {
      toast.error(t('vectorFs.errors.fileMoved'));
    },
  });

  return (
    <React.Fragment>
      <DialogHeader>
        <DialogTitle className="font-normal">
          {t('vectorFs.actions.move')}
          <span className="font-medium">
            {' '}
            &quot;{selectedFile?.name}&quot;
          </span>{' '}
          to ...
        </DialogTitle>
      </DialogHeader>
      <FolderSelectionList />
      <DialogFooter className="mt-4">
        <Button
          className="min-w-[100px]"
          disabled={destinationFolderPath === selectedFile?.path}
          isLoading={isMovingFsItem}
          onClick={async () => {
            await moveFsItem({
              nodeAddress: auth?.node_address ?? '',
              token: auth?.api_v2_key ?? '',
              originPath: selectedFile?.path ?? '',
              destinationPath: `${destinationFolderPath}/${selectedFile?.name}`,
            });
          }}
          size="sm"
        >
          {t('vectorFs.actions.move')}
        </Button>
      </DialogFooter>
    </React.Fragment>
  );
};
export const VectorFsItemDeleteAction = () => {
  const { t } = useTranslation();
  const selectedFile = useVectorFsStore((state) => state.selectedFile);
  const auth = useAuth((state) => state.auth);
  const closeDrawerMenu = useVectorFsStore((state) => state.closeDrawerMenu);

  const { mutateAsync: deleteVrItem, isPending } = useRemoveFsItem({
    onSuccess: () => {
      closeDrawerMenu();
      toast.success(t('vectorFs.success.fileDeleted'));
    },
    onError: () => {
      toast.error(t('vectorFs.errors.fileDeleted'));
    },
  });

  return (
    <React.Fragment>
      <DialogHeader>
        <DialogTitle className="font-normal">
          {t('vectorFs.actions.delete')}
          <span className="font-medium">
            {' '}
            &quot;{selectedFile?.name}&quot;
          </span>{' '}
        </DialogTitle>
      </DialogHeader>

      <p className="text-text-secondary my-3 text-base">
        {t('vectorFs.deleteFileConfirmation')}
      </p>
      <DialogFooter className="mt-4">
        <Button
          className="min-w-[100px]"
          isLoading={isPending}
          onClick={async () => {
            await deleteVrItem({
              nodeAddress: auth?.node_address ?? '',
              token: auth?.api_v2_key ?? '',
              itemPath: selectedFile?.path ?? '',
            });
          }}
          size="sm"
          variant="destructive"
        >
          {t('vectorFs.actions.delete')}
        </Button>
      </DialogFooter>
    </React.Fragment>
  );
};

export const VectorFsItemCopyAction = () => {
  const { t } = useTranslation();
  const selectedFile = useVectorFsStore((state) => state.selectedFile);
  const closeDrawerMenu = useVectorFsStore((state) => state.closeDrawerMenu);
  const auth = useAuth((state) => state.auth);
  const setCurrentGlobalPath = useVectorFsStore(
    (state) => state.setCurrentGlobalPath,
  );
  const destinationFolderPath = useVectorFolderSelectionStore(
    (state) => state.destinationFolderPath,
  );

  const { mutateAsync: copyFsItem, isPending } = useCopyFsItem({
    onSuccess: () => {
      setCurrentGlobalPath(destinationFolderPath ?? '/');
      closeDrawerMenu();
      toast.success(t('vectorFs.success.fileCopied'));
    },
    onError: () => {
      toast.error(t('vectorFs.errors.fileCopied'));
    },
  });

  return (
    <React.Fragment>
      <DialogHeader>
        <DialogTitle className="font-normal">
          {t('vectorFs.actions.copy')}
          <span className="font-medium">
            {' '}
            &quot;{selectedFile?.name}&quot;
          </span>{' '}
          to ...
        </DialogTitle>
      </DialogHeader>
      <FolderSelectionList />
      <DialogFooter className="mt-4">
        <Button
          className="min-w-[100px]"
          disabled={destinationFolderPath === selectedFile?.path}
          isLoading={isPending}
          onClick={async () => {
            await copyFsItem({
              nodeAddress: auth?.node_address ?? '',
              token: auth?.api_v2_key ?? '',
              originPath: selectedFile?.path ?? '',
              destinationPath: `${destinationFolderPath}/${selectedFile?.name}`,
            });
          }}
          size="sm"
        >
          {t('vectorFs.actions.copy')}
        </Button>
      </DialogFooter>
    </React.Fragment>
  );
};

export const VectorFsItemEditTextFileAction = () => {
  const auth = useAuth((state) => state.auth);
  const selectedFile = useVectorFsStore((state) => state.selectedFile);

  const fileNameWithoutExtension = selectedFile?.name?.split('.')?.[0] ?? '';

  const [initialValues, setInitialValues] = React.useState({
    name: fileNameWithoutExtension,
    path: selectedFile?.path ?? '',
    content: '',
  });

  const { mutateAsync: downloadFile } = useGetDownloadFile({});

  useEffect(() => {
    const fetchFileContent = async () => {
      if (selectedFile && auth) {
        try {
          const fileContentBase64 = await downloadFile({
            nodeAddress: auth.node_address,
            token: auth.api_v2_key,
            path: selectedFile.path,
          });
          const binaryString = atob(fileContentBase64);
          const bytes = new Uint8Array(binaryString.length);
          for (let i = 0; i < binaryString.length; i++) {
            bytes[i] = binaryString.charCodeAt(i);
          }
          const fileContent = new TextDecoder('utf-8').decode(bytes);
          setInitialValues({
            name: fileNameWithoutExtension,
            path: selectedFile.path,
            content: fileContent,
          });
        } catch (error) {
          console.error('Error downloading file content:', error);
        }
      }
    };

    void fetchFileContent();
  }, [selectedFile, auth, downloadFile]);

  return <CreateTextFileAction initialValues={initialValues} mode="edit" />;
};
