import {
  useMutation,
  type UseMutationOptions,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import {
  type ToggleEnableToolInput,
  type ToggleEnableToolOutput,
} from './types';
import { toggleEnableTool } from './index';

type Options = UseMutationOptions<
  ToggleEnableToolOutput,
  APIError,
  ToggleEnableToolInput
>;

export const useToggleEnableTool = (options?: Options) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: toggleEnableTool,
    ...options,
    onSuccess: async (response, variables, onMutateResult, context) => {
      await queryClient.invalidateQueries({
        queryKey: [FunctionKeyV2.GET_LIST_TOOLS],
      });
      await queryClient.invalidateQueries({
        queryKey: [FunctionKeyV2.GET_SEARCH_TOOLS],
      });
      if (options?.onSuccess) {
        options.onSuccess(response, variables, onMutateResult, context);
      }
    },
  });
};
