import {
  useMutation,
  type UseMutationOptions,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import { type RunTaskNowInput, type RunTaskNowOutput } from './types';
import { runTaskNow } from './index';

type Options = UseMutationOptions<RunTaskNowOutput, APIError, RunTaskNowInput>;

export const useRunTaskNow = (options?: Options) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: runTaskNow,
    ...options,
    onSuccess: async (response, variables, onMutateResult, context) => {
      await queryClient.invalidateQueries({
        queryKey: [FunctionKeyV2.GET_RECURRING_TASKS],
      });

      if (options?.onSuccess) {
        options.onSuccess(response, variables, onMutateResult, context);
      }
    },
  });
};
