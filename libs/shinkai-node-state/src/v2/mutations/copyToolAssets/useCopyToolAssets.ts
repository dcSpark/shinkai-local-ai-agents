import {
  useMutation,
  type UseMutationOptions,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import { type CopyToolAssetsInput, type CopyToolAssetsOutput } from './types';
import { copyToolAssets } from './index';

type Options = UseMutationOptions<
  CopyToolAssetsOutput,
  APIError,
  CopyToolAssetsInput
>;

export const useCopyToolAssets = (options?: Options) => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: copyToolAssets,
    ...options,
    onSuccess: async (response, variables, onMutateResult, context) => {
      await queryClient.invalidateQueries({
        queryKey: [
          FunctionKeyV2.GET_ALL_TOOL_ASSETS,
          {
            nodeAddress: variables.nodeAddress,
            token: variables.token,
            xShinkaiAppId: variables.xShinkaiAppId,
          },
        ],
      });
      await queryClient.invalidateQueries({
        queryKey: [
          FunctionKeyV2.GET_TOOL,
          {
            toolKey: variables.currentToolKeyPath,
          },
        ],
      });

      if (options?.onSuccess) {
        options.onSuccess(response, variables, onMutateResult, context);
      }
    },
  });
};
