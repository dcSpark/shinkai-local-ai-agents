import {
  useMutation,
  type UseMutationOptions,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import { type UpdateAgentInput, type UpdateAgentOutput } from './types';
import { updateAgent } from './index';

type Options = UseMutationOptions<
  UpdateAgentOutput,
  APIError,
  UpdateAgentInput
>;

export const useUpdateAgent = (options?: Options) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: updateAgent,
    ...options,
    onSuccess: async (response, variables, onMutateResult, context) => {
      await queryClient.invalidateQueries({
        queryKey: [FunctionKeyV2.GET_AGENTS],
      });

      await queryClient.invalidateQueries({
        queryKey: [
          FunctionKeyV2.GET_AGENT,
          {
            agentId: variables.agent.agent_id,
            nodeAddress: variables.nodeAddress,
          },
        ],
      });

      if (options?.onSuccess) {
        options.onSuccess(response, variables, onMutateResult, context);
      }
    },
  });
};
