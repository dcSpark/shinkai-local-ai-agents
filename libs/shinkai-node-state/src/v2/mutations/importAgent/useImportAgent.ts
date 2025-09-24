import {
  useMutation,
  type UseMutationOptions,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import { type ImportAgentInput, type ImportAgentOutput } from './types';
import { importAgent } from '.';

type Options = UseMutationOptions<
  ImportAgentOutput,
  APIError,
  ImportAgentInput
>;

export const useImportAgent = (options?: Options) => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: importAgent,
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
