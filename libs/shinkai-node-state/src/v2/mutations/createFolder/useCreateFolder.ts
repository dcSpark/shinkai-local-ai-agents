import {
  type UseMutationOptions,
  useQueryClient,
  useMutation,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type CreateFolderInput, type CreateFolderOutput } from './types';
import { createFolder } from '.';

type Options = UseMutationOptions<CreateFolderOutput, Error, CreateFolderInput>;

export const useCreateFolder = (options?: Options) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: createFolder,
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
