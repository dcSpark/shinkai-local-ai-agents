import {
  type UseMutationOptions,
  useMutation,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import { type CreateJobInput, type CreateJobOutput } from './types';
import { createJob } from '.';

type Options = UseMutationOptions<CreateJobOutput, APIError, CreateJobInput>;

export const useCreateJob = (options?: Options) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: createJob,
    ...options,
    onSuccess: async (response, variables, onMutateResult, context) => {
      await queryClient.invalidateQueries({
        queryKey: [FunctionKeyV2.GET_INBOXES_WITH_PAGINATION],
      });

      await queryClient.invalidateQueries({
        queryKey: [FunctionKeyV2.GET_JOB_SCOPE],
      });
      await queryClient.invalidateQueries({
        queryKey: [FunctionKeyV2.GET_VR_FILES],
      });
      await queryClient.invalidateQueries({
        queryKey: [FunctionKeyV2.GET_AGENT_INBOXES, variables.llmProvider],
      });

      if (options?.onSuccess) {
        options.onSuccess(response, variables, onMutateResult, context);
      }
    },
  });
};
