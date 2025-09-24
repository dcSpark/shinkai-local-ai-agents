import {
  type MutationOptions,
  useMutation,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import {
  type SetToolMcpEnabledInput,
  type SetToolMcpEnabledOutput,
} from './types';
import { setToolMcpEnabled } from './index';

type Options = MutationOptions<
  SetToolMcpEnabledOutput,
  APIError,
  SetToolMcpEnabledInput
>;

export const useSetToolMcpEnabled = (options?: Options) => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: setToolMcpEnabled,
    ...options,
    onSuccess: async (data, variables, onMutateResult, context) => {
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
            toolKey: variables.toolRouterKey,
          },
        ],
      });

      if (options?.onSuccess) {
        options.onSuccess(data, variables, onMutateResult, context);
      }
    },
  });
};
