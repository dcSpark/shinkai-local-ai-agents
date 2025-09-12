import { Button } from '@shinkai_network/shinkai-ui';
import { X, Zap } from 'lucide-react';
import { type ExternalToast, toast } from 'sonner';

export const EMBEDDING_MIGRATION_TOAST_ID = 'embedding-migration-toast-id';

const defaultToastOptions: ExternalToast = {
  id: EMBEDDING_MIGRATION_TOAST_ID,
  position: 'top-right',
};

export const startEmbeddingMigrationToast = () => {
  return toast.loading(
    <div className="flex items-center gap-2">
      {/* <RefreshCw className="h-4 w-4 animate-spin" /> */}
      <span>Migrating embeddings model...</span>
    </div>,
    {
      ...defaultToastOptions,
      description: 'This may take a few minutes. You can continue using the app.',
    }
  );
};

export const embeddingMigrationSuccessToast = (model: string) => {
  toast.dismiss(EMBEDDING_MIGRATION_TOAST_ID);
  return toast.success(`Successfully migrated embeddings model`, {
    position: 'top-right',
    description: `Embedding model has been updated to ${model}.`,
  });
};

export const embeddingMigrationErrorToast = (model: string, error?: string) => {
  toast.dismiss(EMBEDDING_MIGRATION_TOAST_ID);
  return toast.error(`Failed to migrate to ${model}`, {
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
  return toast.info(
    <div className="space-y-3">
      <div>
        <h4 className="font-semibold">Embedding Model Update Available</h4>
        <p className="text-sm opacity-90">
          You're using <strong>{currentModel}</strong>, but the recommended model is <strong>{defaultModel}</strong>
        </p>
      </div>
      <div className="flex gap-2">
        <Button
          size="sm"
          onClick={() => {
            onMigrateToDefault();
            toast.dismiss();
          }}
          className="h-8"
        >
          <Zap className="h-3 w-3 mr-1" />
          Migrate Now
        </Button>
        <Button
          size="sm"
          variant="outline"
          onClick={() => {
            onDismiss();
            toast.dismiss();
          }}
          className="h-8"
        >
          <X className="h-3 w-3 mr-1" />
          Dismiss
        </Button>
      </div>
    </div>,
    {
      position: 'top-right',
      duration: 10000, // Show for 10 seconds
      closeButton: true,
    }
  );
};
