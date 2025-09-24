import {
  useMutation,
  type UseMutationOptions,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import {
  type RestoreToolConversationInput,
  type RestoreToolConversationOutput,
} from './types';
import { restoreToolConversation } from '.';

type Options = UseMutationOptions<
  RestoreToolConversationOutput,
  APIError,
  RestoreToolConversationInput
>;

export const useRestoreToolConversation = (options?: Options) => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: restoreToolConversation,
    ...options,
    onSuccess: async (response, variables, onMutateResult, context) => {
      await queryClient.invalidateQueries({
        queryKey: [FunctionKeyV2.GET_CHAT_CONVERSATION_PAGINATION],
      });
      if (options?.onSuccess) {
        options.onSuccess(response, variables, onMutateResult, context);
      }
    },
  });
};
