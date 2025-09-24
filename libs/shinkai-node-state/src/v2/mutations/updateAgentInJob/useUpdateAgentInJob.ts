import {
  type UseMutationOptions,
  useQueryClient,
  useMutation,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import {
  type UpdateAgentInJobInput,
  type UpdateAgentInJobOutput,
} from './types';
import { updateAgentInJob } from '.';

type Options = UseMutationOptions<
  UpdateAgentInJobOutput,
  APIError,
  UpdateAgentInJobInput
>;

export const useUpdateAgentInJob = (options?: Options) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: updateAgentInJob,
    ...options,
    onSuccess: async (response, variables, onMutateResult, context) => {
      await queryClient.invalidateQueries({
        queryKey: [
          FunctionKeyV2.GET_PROVIDER_FROM_JOB,
          { jobId: variables.jobId },
        ],
      });

      if (options?.onSuccess) {
        options.onSuccess(response, variables, onMutateResult, context);
      }
    },
  });
};
