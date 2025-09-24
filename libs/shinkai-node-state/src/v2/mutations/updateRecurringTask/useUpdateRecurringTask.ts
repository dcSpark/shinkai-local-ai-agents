import {
  useMutation,
  type UseMutationOptions,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import {
  type UpdateRecurringTaskInput,
  type UpdateRecurringTaskOutput,
} from './types';
import { updateRecurringTask } from './index';

type Options = UseMutationOptions<
  UpdateRecurringTaskOutput,
  APIError,
  UpdateRecurringTaskInput
>;

export const useUpdateRecurringTask = (options?: Options) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: updateRecurringTask,
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
