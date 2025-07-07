import {
  useMutation,
  type UseMutationOptions,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import { type RemoveToolInput, type RemoveToolOutput } from './types';
import { removeToolOffering } from './index';

type Options = UseMutationOptions<RemoveToolOutput, APIError, RemoveToolInput>;

export const useRemoveToolOffering = (options?: Options) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: removeToolOffering,
    ...options,
    onSuccess: async (response, variables, context) => {
      await queryClient.invalidateQueries({
        queryKey: [FunctionKeyV2.GET_TOOLS_WITH_OFFERINGS],
      });

      if (options?.onSuccess) {
        options.onSuccess(response, variables, context);
      }
    },
  });
};
