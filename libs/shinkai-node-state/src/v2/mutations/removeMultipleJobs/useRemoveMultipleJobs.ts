import {
  useMutation,
  type UseMutationOptions,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import type { RemoveJobsInput, RemoveJobsOutput } from './types';
import { removeJobs } from './index';

type Options = UseMutationOptions<RemoveJobsOutput, APIError, RemoveJobsInput>;

export const useRemoveJobs = (options?: Options) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: removeJobs,
    ...options,
    onSuccess: async (response, variables, context) => {
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
        options.onSuccess(response, variables, context);
      }
    },
  });
};
