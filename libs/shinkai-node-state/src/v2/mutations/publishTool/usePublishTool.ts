import {
  useMutation,
  type UseMutationOptions,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import { type PublishToolInput, type PublishToolOutput } from './types';
import { publishTool } from './index';

type Options = UseMutationOptions<
  PublishToolOutput,
  APIError,
  PublishToolInput
>;

export const usePublishTool = (options?: Options) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: publishTool,
    ...options,
    onSuccess: async (response, variables, onMutateResult, context) => {
      await queryClient.invalidateQueries({
        queryKey: [FunctionKeyV2.GET_LIST_TOOLS],
      });

      if (options?.onSuccess) {
        options.onSuccess(response, variables, onMutateResult, context);
      }
    },
  });
};
