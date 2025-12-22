import {
  useMutation,
  type UseMutationOptions,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import { type PublishAgentInput, type PublishAgentOutput } from './types';
import { publishAgent } from './index';

type Options = UseMutationOptions<
  PublishAgentOutput,
  APIError,
  PublishAgentInput
>;

export const usePublishAgent = (options?: Options) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: publishAgent,
    ...options,
    onSuccess: async (response, variables, context) => {
      await queryClient.invalidateQueries({
        queryKey: [FunctionKeyV2.GET_AGENTS],
      });

      if (options?.onSuccess) {
        options.onSuccess(response, variables, context);
      }
    },
  });
};

