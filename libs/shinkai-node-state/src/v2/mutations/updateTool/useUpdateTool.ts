import {
  useMutation,
  type UseMutationOptions,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import { type UpdateToolInput, type UpdateToolOutput } from './types';
import { updateTool } from './index';

type Options = UseMutationOptions<UpdateToolOutput, APIError, UpdateToolInput>;

export const useUpdateTool = (options?: Options) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: updateTool,
    ...options,
    onSuccess: async (response, variables, onMutateResult, context) => {
      await queryClient.invalidateQueries({
        queryKey: [FunctionKeyV2.GET_LIST_TOOLS],
      });

      await queryClient.invalidateQueries({
        queryKey: [FunctionKeyV2.GET_SEARCH_TOOLS],
      });

      await queryClient.invalidateQueries({
        queryKey: [
          FunctionKeyV2.GET_TOOL,
          {
            toolKey: variables.toolKey,
          },
        ],
      });

      if (options?.onSuccess) {
        options.onSuccess(response, variables, onMutateResult, context);
      }
    },
  });
};
