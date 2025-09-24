import {
  useMutation,
  type UseMutationOptions,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type RemoveFsItemInput, type RemoveFsItemOutput } from './types';
import { removeFsItem } from './index';

type Options = UseMutationOptions<RemoveFsItemOutput, Error, RemoveFsItemInput>;

export const useRemoveFsItem = (options?: Options) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: removeFsItem,
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
