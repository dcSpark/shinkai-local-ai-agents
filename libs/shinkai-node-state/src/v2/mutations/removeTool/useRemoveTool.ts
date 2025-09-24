import {
  useMutation,
  type UseMutationOptions,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import { type RemoveToolInput, type RemoveToolOutput } from './types';
import { removeTool } from './index';

type Options = UseMutationOptions<RemoveToolOutput, APIError, RemoveToolInput>;

export const useRemoveTool = (options?: Options) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: removeTool,
    ...options,
    onSuccess: async (response, variables, onMutateResult, context) => {
      await queryClient.invalidateQueries({
        queryKey: [
          FunctionKeyV2.GET_PLAYGROUND_TOOLS,
          {
            nodeAddress: variables.nodeAddress,
            token: variables.token,
          },
        ],
      });

      if (options?.onSuccess) {
        options.onSuccess(response, variables, onMutateResult, context);
      }
    },
  });
};
