import {
  useMutation,
  type UseMutationOptions,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import {
  type UpdateChatConfigInput,
  type UpdateChatConfigOutput,
} from './types';
import { updateChatConfig } from './index';

type Options = UseMutationOptions<
  UpdateChatConfigOutput,
  APIError,
  UpdateChatConfigInput
>;

export const useUpdateChatConfig = (options?: Options) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: updateChatConfig,
    ...options,
    onSuccess: async (response, variables, onMutateResult, context) => {
      await queryClient.invalidateQueries({
        queryKey: [FunctionKeyV2.GET_CHAT_CONFIG],
      });

      if (options?.onSuccess) {
        options.onSuccess(response, variables, onMutateResult, context);
      }
    },
  });
};
