import { useStartEmbeddingMigration } from '@shinkai_network/shinkai-node-state/v2/mutations/startEmbeddingMigration/useStartEmbeddingMigration';
import { useGetEmbeddingMigrationStatus } from '@shinkai_network/shinkai-node-state/v2/queries/getEmbeddingMigrationStatus/useGetEmbeddingMigrationStatus';
import { useCallback, useEffect, useRef } from 'react';

import { toast } from 'sonner';
import { useAuth } from '../../store/auth';
import { useSettings } from '../../store/settings';
import { useShinkaiNodeGetDefaultEmbeddingModelQuery } from '../shinkai-node-manager/shinkai-node-manager-client';
import { embeddingModelMismatchToast } from './embedding-migration-toasts';

export const useEmbeddingStartupCheck = () => {
  const auth = useAuth((state) => state.auth);
  const hasShownToastRef = useRef<boolean>(false);
  const isInitialCheckRef = useRef<boolean>(true);

  const isPromptDismissed = useSettings(
    (state) => state.embeddingModelMismatchPromptDismissed,
  );
  const setPromptDismissed = useSettings(
    (state) => state.setEmbeddingModelMismatchPromptDismissed,
  );

  const { data: defaultEmbeddingModel } =
    useShinkaiNodeGetDefaultEmbeddingModelQuery({
      staleTime: Infinity, // Static value, never changes
    });

  const { data: embeddingMigrationStatus } = useGetEmbeddingMigrationStatus(
    { nodeAddress: auth?.node_address ?? '', token: auth?.api_v2_key ?? '' },
    {
      enabled: !!auth,
      refetchOnMount: true,
      refetchOnWindowFocus: false,
      staleTime: Infinity,
    },
  );

  const { mutateAsync: startEmbeddingMigration } = useStartEmbeddingMigration({
    onSuccess: () => {
      setPromptDismissed(true);
    },
    onError: (error) => {
      toast.error('Failed to start embedding migration', {
        description: error.response?.data?.message ?? error.message,
      });
    },
  });

  const handleMigrateToDefault = useCallback(async () => {
    if (!auth || !defaultEmbeddingModel) return;
    await startEmbeddingMigration({
      nodeAddress: auth.node_address,
      token: auth.api_v2_key,
      force: true,
      embedding_model: defaultEmbeddingModel,
    });
  }, [auth, defaultEmbeddingModel, startEmbeddingMigration]);

  useEffect(() => {
    // Only run this check once on initial load
    if (
      !isInitialCheckRef.current ||
      !embeddingMigrationStatus ||
      !defaultEmbeddingModel ||
      hasShownToastRef.current ||
      isPromptDismissed
    ) {
      return;
    }

    const currentModel = embeddingMigrationStatus.current_embedding_model;

    // Check if current model is different from default and migration is not in progress
    if (
      currentModel !== defaultEmbeddingModel &&
      !embeddingMigrationStatus.migration_in_progress &&
      embeddingMigrationStatus.status === 'ready'
    ) {
      embeddingModelMismatchToast({
        currentModel,
        defaultModel: defaultEmbeddingModel,
        onMigrateToDefault: handleMigrateToDefault,
        onDismiss: () => setPromptDismissed(true),
      });

      hasShownToastRef.current = true;
    }

    isInitialCheckRef.current = false;
  }, [
    embeddingMigrationStatus,
    defaultEmbeddingModel,
    handleMigrateToDefault,
    setPromptDismissed,
    isPromptDismissed,
  ]);
};
