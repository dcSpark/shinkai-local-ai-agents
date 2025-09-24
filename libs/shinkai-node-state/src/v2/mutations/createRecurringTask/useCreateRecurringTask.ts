import {
  useMutation,
  type UseMutationOptions,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import {
  type CreateRecurringTaskInput,
  type CreateRecurringTaskOutput,
} from './types';
import { createRecurringTask } from './index';

type Options = UseMutationOptions<
  CreateRecurringTaskOutput,
  APIError,
  CreateRecurringTaskInput
>;

export const useCreateRecurringTask = (options?: Options) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: createRecurringTask,
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
