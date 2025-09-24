import {
  useMutation,
  type UseMutationOptions,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type CreateToolInput, type CreateToolOutput } from './types';
import { createTool } from './index';

type Options = UseMutationOptions<CreateToolOutput, Error, CreateToolInput>;

export const useCreateTool = (options?: Options) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: createTool,
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
