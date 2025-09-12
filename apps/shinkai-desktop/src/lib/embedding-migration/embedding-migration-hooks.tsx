import { useGetEmbeddingMigrationStatus } from '@shinkai_network/shinkai-node-state/v2/queries/getEmbeddingMigrationStatus/useGetEmbeddingMigrationStatus';
import { useEffect, useRef, useState } from 'react';

import { useAuth } from '../../store/auth';
import {
  embeddingMigrationErrorToast,
  embeddingMigrationSuccessToast,
  startEmbeddingMigrationToast,
} from './embedding-migration-toasts';

export const useEmbeddingMigrationToast = () => {
  const auth = useAuth((state) => state.auth);
  const [shouldPoll, setShouldPoll] = useState(false);
  const previousStatusRef = useRef<{
    migration_in_progress: boolean;
    current_embedding_model: string;
    status: string;
  } | null>(null);
  const targetModelRef = useRef<string>('');

  const { data: embeddingMigrationStatus } = useGetEmbeddingMigrationStatus(
    { nodeAddress: auth?.node_address ?? '', token: auth?.api_v2_key ?? '' },
    { 
      enabled: !!auth, 
      // Only poll when migration is in progress
      refetchInterval: shouldPoll ? 2000 : false,
      refetchIntervalInBackground: true
    }
  );

  // Control polling based on migration status
  useEffect(() => {
    if (embeddingMigrationStatus) {
      const isInProgress = embeddingMigrationStatus.migration_in_progress;
      setShouldPoll(isInProgress);
      
      // If migration started, ensure we start polling immediately
      if (isInProgress && !shouldPoll) {
        console.log('Migration started, beginning polling...');
      }
    }
  }, [embeddingMigrationStatus, shouldPoll]);

  useEffect(() => {
    if (!embeddingMigrationStatus) return;

    // Initialize previous status on first load
    if (!previousStatusRef.current) {
      previousStatusRef.current = {
        migration_in_progress: embeddingMigrationStatus.migration_in_progress,
        current_embedding_model: embeddingMigrationStatus.current_embedding_model,
        status: embeddingMigrationStatus.status,
      };
      
      // If migration is already in progress on first load, show toast immediately
      if (embeddingMigrationStatus.migration_in_progress) {
        targetModelRef.current = embeddingMigrationStatus.current_embedding_model;
        startEmbeddingMigrationToast();
      }
      return;
    }

    const previousStatus = previousStatusRef.current;
    const currentStatus = embeddingMigrationStatus;

    // Migration started (wasn't in progress before, now it is)
    if (!previousStatus.migration_in_progress && currentStatus.migration_in_progress) {
      // Store the target model (the one we're migrating to)
      targetModelRef.current = currentStatus.current_embedding_model;
      startEmbeddingMigrationToast();
    }

    // Migration completed successfully
    if (
      previousStatus.migration_in_progress && 
      !currentStatus.migration_in_progress &&
      currentStatus.status === 'ready'
    ) {
      embeddingMigrationSuccessToast(currentStatus.current_embedding_model);
    }

    // Migration failed
    if (
      previousStatus.migration_in_progress && 
      !currentStatus.migration_in_progress &&
      currentStatus.status !== 'ready'
    ) {
      embeddingMigrationErrorToast(
        targetModelRef.current || currentStatus.current_embedding_model,
        `Migration status: ${currentStatus.status}`
      );
    }

    // Update previous status
    previousStatusRef.current = {
      migration_in_progress: currentStatus.migration_in_progress,
      current_embedding_model: currentStatus.current_embedding_model,
      status: currentStatus.status,
    };
  }, [embeddingMigrationStatus]);
};
