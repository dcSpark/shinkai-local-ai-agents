import {
  useMutation,
  type UseMutationOptions,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type MoveFsItemInput, type MoveFsItemOutput } from './types';
import { moveFsItem } from './index';

type Options = UseMutationOptions<MoveFsItemOutput, Error, MoveFsItemInput>;

export const useMoveFsItem = (options?: Options) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: moveFsItem,
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
