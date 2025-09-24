import {
  useMutation,
  type UseMutationOptions,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type CopyVRItemInput, type CopyVRItemOutput } from './types';
import { copyFsItem } from './index';

type Options = UseMutationOptions<CopyVRItemOutput, Error, CopyVRItemInput>;

export const useCopyFsItem = (options?: Options) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: copyFsItem,
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
