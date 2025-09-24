import { Button } from '@shinkai_network/shinkai-ui';
import { PartyIcon } from '@shinkai_network/shinkai-ui/assets';
import { invoke } from '@tauri-apps/api/core';
import { relaunch } from '@tauri-apps/plugin-process';

import { type ExternalToast, toast } from 'sonner';

export const EMBEDDING_MIGRATION_TOAST_ID = 'embedding-migration-toast-id';

const defaultToastOptions: ExternalToast = {
  id: EMBEDDING_MIGRATION_TOAST_ID,
  position: 'top-right',
};

const mapModelToName = (model: string) => {
  switch (model) {
    case 'snowflake-arctic-embed:xs':
      return 'Snowflake';
    case 'embeddinggemma:300m':
      return 'Gemma';
    default:
      return model;
  }
};

export const startEmbeddingMigrationToast = () => {
  return toast.loading('Updating embedding model...', {
    ...defaultToastOptions,
    description:
      'This may take a few minutes, but you can keep using the app. The app will restart automatically when complete.',
  });
};

export const embeddingMigrationSuccessToast = async (model: string) => {
  toast.dismiss(EMBEDDING_MIGRATION_TOAST_ID);
  toast.success(`Embeddings updated successfully`, {
    position: 'top-right',
    description: `Embeddings are now updated to ${model}. Enjoy smarter and faster results. Restarting the app...`,
  });
  setTimeout(async () => {
    await invoke('shinkai_node_kill');
    await relaunch();
  }, 3000);
};

export const embeddingMigrationErrorToast = (model: string, error?: string) => {
  toast.dismiss(EMBEDDING_MIGRATION_TOAST_ID);
  return toast.error(`Failed to update to ${model}`, {
    position: 'top-right',
    description: error || 'Migration process encountered an error.',
  });
};

export const embeddingModelMismatchToast = ({
  currentModel,
  defaultModel,
  onMigrateToDefault,
  onDismiss,
}: {
  currentModel: string;
  defaultModel: string;
  onMigrateToDefault: () => void;
  onDismiss: () => void;
}) => {
  return toast.info('Embeddings Update Available', {
    description: (
      <div className="space-y-3">
        <p className="text-xs">
          Switch from {mapModelToName(currentModel)} to{' '}
          {mapModelToName(defaultModel)} embeddings for better search and
          retrieval results, plus improved performance.
        </p>
        <div className="flex justify-end gap-2">
          <Button
            size="xs"
            onClick={() => {
              onMigrateToDefault();
              toast.dismiss();
            }}
            variant="outline"
          >
            Update
          </Button>
          <Button
            size="xs"
            variant="tertiary"
            onClick={() => {
              onDismiss();
              toast.dismiss();
            }}
          >
            Later
          </Button>
        </div>
      </div>
    ),

    position: 'top-right',
    duration: 10000, // Show for 10 seconds
    closeButton: true,
    icon: <PartyIcon className="size-4" />,
    classNames: {
      icon: 'self-start pt-2',
    },
  });
};
