import {
  useMutation,
  type UseMutationOptions,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import { type KillJobInput, type KillJobOutput } from './types';
import { killJob } from './index';

type Options = UseMutationOptions<KillJobOutput, APIError, KillJobInput>;

export const useKillJob = (options?: Options) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: killJob,
    ...options,
    onSuccess: async (response, variables, context) => {
      await queryClient.invalidateQueries({
        queryKey: [FunctionKeyV2.GET_CHAT_CONFIG],
      });

      if (options?.onSuccess) {
        options.onSuccess(response, variables, context);
      }
    },
  });
};
