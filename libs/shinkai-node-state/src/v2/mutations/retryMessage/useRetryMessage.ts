import {
  useMutation,
  type UseMutationOptions,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import { type RetryMessageInput, type RetryMessageOutput } from './types';
import { retryMessage } from './index';

type Options = UseMutationOptions<
  RetryMessageOutput,
  APIError,
  RetryMessageInput
>;

export const useRetryMessage = (options?: Options) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: retryMessage,
    ...options,
    onSuccess: async (response, variables, onMutateResult, context) => {
      await queryClient.invalidateQueries({
        queryKey: [
          FunctionKeyV2.GET_CHAT_CONVERSATION_PAGINATION,
          { inboxId: variables.inboxId },
        ],
      });

      if (options?.onSuccess) {
        options.onSuccess(response, variables, onMutateResult, context);
      }
    },
  });
};
