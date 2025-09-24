import {
  useMutation,
  type UseMutationOptions,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import { type MoveFolderOutput, type MoveVRFolderInput } from './types';
import { moveFolder } from './index';

type Options = UseMutationOptions<
  MoveFolderOutput,
  APIError,
  MoveVRFolderInput
>;

export const useMoveFolder = (options?: Options) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: moveFolder,
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
