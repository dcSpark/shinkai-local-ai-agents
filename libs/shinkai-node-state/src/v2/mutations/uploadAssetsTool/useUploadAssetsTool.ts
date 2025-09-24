import {
  type UseMutationOptions,
  useMutation,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import {
  type UploadAssetsToToolInput,
  type UploadAssetsToToolOutput,
} from './types';
import { uploadAssetsToTool } from '.';

type Options = UseMutationOptions<
  UploadAssetsToToolOutput,
  APIError,
  UploadAssetsToToolInput
>;

export const useUploadAssetsTool = (options?: Options) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: uploadAssetsToTool,
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
