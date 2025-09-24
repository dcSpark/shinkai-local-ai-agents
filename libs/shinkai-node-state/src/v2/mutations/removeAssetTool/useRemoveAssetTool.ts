import {
  type UseMutationOptions,
  useMutation,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import {
  type RemoveAssetToToolInput,
  type RemoveAssetToToolOutput,
} from './types';
import { removeToolAsset } from '.';

type Options = UseMutationOptions<
  RemoveAssetToToolOutput,
  APIError,
  RemoveAssetToToolInput
>;

export const useRemoveAssetTool = (options?: Options) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: removeToolAsset,
    ...options,
    onSuccess: async (response, variables, onMutateResult, context) => {
      await queryClient.invalidateQueries({
        queryKey: [FunctionKeyV2.GET_ALL_TOOL_ASSETS],
      });

      if (options?.onSuccess) {
        options.onSuccess(response, variables, onMutateResult, context);
      }
    },
  });
};
