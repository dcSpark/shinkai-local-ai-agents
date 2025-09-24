import {
  useMutation,
  type UseMutationOptions,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import {
  type RemoveRecurringTaskInput,
  type RemoveRecurringTaskOutput,
} from './types';
import { removeRecurringTask } from './index';

type Options = UseMutationOptions<
  RemoveRecurringTaskOutput,
  APIError,
  RemoveRecurringTaskInput
>;

export const useRemoveRecurringTask = (options?: Options) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: removeRecurringTask,
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
