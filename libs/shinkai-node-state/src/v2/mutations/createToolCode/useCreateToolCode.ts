import {
  useMutation,
  type UseMutationOptions,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import { type CreateToolCodeInput, type CreateToolCodeOutput } from './types';
import { createToolCode } from '.';

type Options = UseMutationOptions<
  CreateToolCodeOutput,
  APIError,
  CreateToolCodeInput
>;

export const useCreateToolCode = (options?: Options) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: createToolCode,
    ...options,
    onSuccess: async (response, variables, onMutateResult, context) => {
      await queryClient.invalidateQueries({
        queryKey: [
          FunctionKeyV2.GET_CHAT_CONVERSATION_PAGINATION,
          { inboxId: response.inbox },
        ],
      });
      if (options?.onSuccess) {
        options.onSuccess(response, variables, onMutateResult, context);
      }
    },
  });
};
