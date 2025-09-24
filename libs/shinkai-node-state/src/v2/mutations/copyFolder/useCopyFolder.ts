import {
  useMutation,
  type UseMutationOptions,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type CopyFolderInput, type CopyFolderOutput } from './types';
import { copyFolder } from './index';

type Options = UseMutationOptions<CopyFolderOutput, Error, CopyFolderInput>;

export const useCopyFolder = (options?: Options) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: copyFolder,
    ...options,
    onSuccess: async (response, variables, onMutateResult, context) => {
      await queryClient.invalidateQueries({
        queryKey: [FunctionKeyV2.GET_VR_FILES],
      });

      if (options?.onSuccess) {
        options.onSuccess(response, variables, onMutateResult, context);
      }
    },
  });
};
