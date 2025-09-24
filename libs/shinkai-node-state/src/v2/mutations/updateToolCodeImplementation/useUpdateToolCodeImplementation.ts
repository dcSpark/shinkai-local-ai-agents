import {
  useMutation,
  type UseMutationOptions,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKey } from '../../../lib/constants';
import { type APIError } from '../../types';
import {
  type UpdateToolCodeImplementationInput,
  type UpdateToolCodeImplementationOutput,
} from './types';
import { updateToolCodeImplementation } from '.';

type Options = UseMutationOptions<
  UpdateToolCodeImplementationOutput,
  APIError,
  UpdateToolCodeImplementationInput
>;

export const useUpdateToolCodeImplementation = (options?: Options) => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: updateToolCodeImplementation,
    ...options,
    onSuccess: async (response, variables, onMutateResult, context) => {
      await queryClient.invalidateQueries({
        queryKey: [FunctionKey.GET_CHAT_CONVERSATION_PAGINATION],
      });
      if (options?.onSuccess) {
        options.onSuccess(response, variables, onMutateResult, context);
      }
    },
  });
};
