import {
  useMutation,
  type UseMutationOptions,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import { type RemovePromptInput, type RemovePromptOutput } from './types';
import { removePrompt } from './index';

type Options = UseMutationOptions<
  RemovePromptOutput,
  APIError,
  RemovePromptInput
>;

export const useRemovePrompt = (options?: Options) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: removePrompt,
    ...options,
    onSuccess: async (response, variables, onMutateResult, context) => {
      await queryClient.invalidateQueries({
        queryKey: [
          FunctionKeyV2.GET_LIST_PROMPTS,
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
