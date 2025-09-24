import {
  useMutation,
  type UseMutationOptions,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import { type CreateAgentInput, type CreateAgentOutput } from './types';
import { createAgent } from './index';

type Options = UseMutationOptions<
  CreateAgentOutput,
  APIError,
  CreateAgentInput
>;

export const useCreateAgent = (options?: Options) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: createAgent,
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
