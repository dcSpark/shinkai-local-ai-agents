import {
  useMutation,
  type UseMutationOptions,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import { type DisableAllToolsInput, type DisableAllToolsOutput } from './types';
import { disableAllTools } from '.';

type Options = UseMutationOptions<
  DisableAllToolsOutput,
  APIError,
  DisableAllToolsInput
>;

export const useDisableAllTools = (options?: Options) => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: disableAllTools,
    ...options,
    onSuccess: async (response, variables, onMutateResult, context) => {
      await queryClient.invalidateQueries({
        queryKey: [FunctionKeyV2.GET_LIST_TOOLS],
      });
      if (options?.onSuccess) {
        options.onSuccess(response, variables, onMutateResult, context);
      }
    },
  });
};
