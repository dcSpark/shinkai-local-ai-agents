import {
  useMutation,
  type UseMutationOptions,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import {
  type ImportAgentFromUrlInput,
  type ImportAgentFromUrlOutput,
} from './types';
import { importAgentFromUrl } from '.';

type Options = UseMutationOptions<
  ImportAgentFromUrlOutput,
  APIError,
  ImportAgentFromUrlInput
>;

export const useImportAgentFromUrl = (options?: Options) => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: importAgentFromUrl,
    ...options,
    onSuccess: async (response, variables, onMutateResult, context) => {
      await queryClient.invalidateQueries({
        queryKey: [FunctionKeyV2.GET_AGENTS],
      });
      if (options?.onSuccess) {
        options.onSuccess(response, variables, onMutateResult, context);
      }
    },
  });
};
