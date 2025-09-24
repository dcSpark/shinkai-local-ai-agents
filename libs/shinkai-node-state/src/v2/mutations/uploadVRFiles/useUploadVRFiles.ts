import {
  useMutation,
  type UseMutationOptions,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKey } from '../../../lib/constants';
import { type UploadVRFilesInput, type UploadVRFilesOutput } from './types';
import { uploadVRFiles } from '.';

type Options = UseMutationOptions<
  UploadVRFilesOutput,
  Error,
  UploadVRFilesInput
>;

export const useUploadVRFiles = (options?: Options) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: uploadVRFiles,
    ...options,
    onSuccess: async (response, variables, onMutateResult, context) => {
      await queryClient.invalidateQueries({
        queryKey: [FunctionKey.GET_VR_FILES],
      });

      if (options?.onSuccess) {
        options.onSuccess(response, variables, onMutateResult, context);
      }
    },
  });
};
