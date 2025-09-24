import {
  useMutation,
  type UseMutationOptions,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import { type ForkJobMessagesInput, type ForkJobMessagesOutput } from './types';
import { forkJobMessages } from './index';

type Options = UseMutationOptions<
  ForkJobMessagesOutput,
  APIError,
  ForkJobMessagesInput
>;

export const useForkJobMessages = (options?: Options) => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: forkJobMessages,
    ...options,
    onSuccess: async (response, variables, onMutateResult, context) => {
      await queryClient.invalidateQueries({
        queryKey: [FunctionKeyV2.GET_INBOXES_WITH_PAGINATION],
      });

      if (options?.onSuccess) {
        options.onSuccess(response, variables, onMutateResult, context);
      }
    },
  });
};
