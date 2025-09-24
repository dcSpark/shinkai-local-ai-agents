import {
  useMutation,
  type UseMutationOptions,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { getPlaygroundTool } from '../../queries/getPlaygroundTool';
import { type APIError } from '../../types';
import { type DuplicateToolInput, type DuplicateToolOutput } from './types';
import { duplicateTool } from '.';

type Options = UseMutationOptions<
  DuplicateToolOutput,
  APIError,
  DuplicateToolInput
>;

export const useDuplicateTool = (options?: Options) => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: duplicateTool,
    ...options,
    onSuccess: async (response, variables, onMutateResult, context) => {
      await queryClient.invalidateQueries({
        queryKey: [FunctionKeyV2.GET_LIST_TOOLS],
      });

      await queryClient.prefetchQuery({
        queryKey: [
          FunctionKeyV2.GET_PLAYGROUND_TOOL,
          {
            toolRouterKey: response.tool_router_key,
            token: variables.token,
            nodeAddress: variables.nodeAddress,
            xShinkaiOriginalToolRouterKey: variables.toolKey,
          },
        ],
        queryFn: () =>
          getPlaygroundTool({
            toolRouterKey: response.tool_router_key,
            token: variables.token,
            nodeAddress: variables.nodeAddress,
            xShinkaiOriginalToolRouterKey: variables.toolKey,
          }),
      });

      if (options?.onSuccess) {
        options.onSuccess(response, variables, onMutateResult, context);
      }
    },
  });
};
