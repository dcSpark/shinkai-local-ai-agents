import {
  useMutation,
  type UseMutationOptions,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import { type RemoveJobInput, type RemoveJobOutput } from './types';
import { removeJob } from './index';

type Options = UseMutationOptions<RemoveJobOutput, APIError, RemoveJobInput>;

export const useRemoveJob = (options?: Options) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: removeJob,
    ...options,
    onSuccess: async (response, variables, onMutateResult, context) => {
      await queryClient.invalidateQueries({
        queryKey: [
          FunctionKeyV2.GET_INBOXES_WITH_PAGINATION,
          {
            nodeAddress: variables.nodeAddress,
            token: variables.token,
          },
        ],
      });

      if (options?.onSuccess) {
        options.onSuccess(response, variables, onMutateResult, context);
      }
    },
  });
};
