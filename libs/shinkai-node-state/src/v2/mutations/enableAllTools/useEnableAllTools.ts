import {
  useMutation,
  type UseMutationOptions,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import { type EnableAllToolsInput, type EnableAllToolsOutput } from './types';
import { enableAllTools } from '.';

type Options = UseMutationOptions<
  EnableAllToolsOutput,
  APIError,
  EnableAllToolsInput
>;

export const useEnableAllTools = (options?: Options) => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: enableAllTools,
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
