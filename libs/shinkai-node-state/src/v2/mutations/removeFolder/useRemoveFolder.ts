import {
  useMutation,
  type UseMutationOptions,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import { type RemoveFolderInput, type RemoveFolderOutput } from './types';
import { removeFolder } from './index';

type Options = UseMutationOptions<
  RemoveFolderOutput,
  APIError,
  RemoveFolderInput
>;

export const useRemoveFolder = (options?: Options) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: removeFolder,
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
