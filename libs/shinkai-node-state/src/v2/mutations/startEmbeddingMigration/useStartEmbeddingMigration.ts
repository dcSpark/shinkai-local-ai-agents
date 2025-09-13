import {
  useMutation,
  useQueryClient,
  type UseMutationOptions,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import {
  type StartEmbeddingMigrationInput,
  type StartEmbeddingMigrationOutput,
} from './types';
import { startEmbeddingMigration } from '.';

type Options = UseMutationOptions<
  StartEmbeddingMigrationOutput,
  APIError,
  StartEmbeddingMigrationInput
>;

export const useStartEmbeddingMigration = (options?: Options) => {
  const queryClient = useQueryClient();
  const response = useMutation({
    mutationFn: startEmbeddingMigration,
    ...options,
    onSuccess: async (...onSuccessParameters) => {
      await queryClient.invalidateQueries({
        queryKey: [FunctionKeyV2.GET_EMBEDDING_MIGRATION_STATUS],
      });
      if (options?.onSuccess) {
        options.onSuccess(...onSuccessParameters);
      }
    },
  });
  return response;
};
