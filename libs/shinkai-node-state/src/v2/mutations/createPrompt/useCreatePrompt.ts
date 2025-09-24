import {
  useMutation,
  type UseMutationOptions,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import { type CreatePromptInput, type CreatePromptOutput } from './types';
import { createPrompt } from './index';

type Options = UseMutationOptions<
  CreatePromptOutput,
  APIError,
  CreatePromptInput
>;

export const useCreatePrompt = (options?: Options) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: createPrompt,
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
