import {
  useMutation,
  type UseMutationOptions,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import { type UpdatePromptInput, type UpdatePromptOutput } from './types';
import { updatePrompt } from './index';

type Options = UseMutationOptions<
  UpdatePromptOutput,
  APIError,
  UpdatePromptInput
>;

export const useUpdatePrompt = (options?: Options) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: updatePrompt,
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
