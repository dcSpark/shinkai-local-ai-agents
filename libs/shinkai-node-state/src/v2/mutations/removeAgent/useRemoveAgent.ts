import {
  type UseMutationOptions,
  useQueryClient,
  useMutation,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import { type RemoveAgentInput, type RemoveAgentOutput } from './types';
import { removeAgent } from '.';

type Options = UseMutationOptions<
  RemoveAgentOutput,
  APIError,
  RemoveAgentInput
>;

export const useRemoveAgent = (options?: Options) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: removeAgent,
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
