import {
  type UseMutationOptions,
  useMutation,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import {
  type UploadPlaygroundToolFilesInput,
  type UploadPlaygroundToolFilesOutput,
} from './types';
import { uploadPlaygroundToolFiles } from '.';

type Options = UseMutationOptions<
  UploadPlaygroundToolFilesOutput,
  APIError,
  UploadPlaygroundToolFilesInput
>;

export const useUploadPlaygroundToolFiles = (options?: Options) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: uploadPlaygroundToolFiles,
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
