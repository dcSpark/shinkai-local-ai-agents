import {
  useMutation,
  type UseMutationOptions,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import {
  type StopGeneratingLLMInput,
  type StopGeneratingLLMOutput,
} from './types';
import { stopGeneratingLLM } from './index';

type Options = UseMutationOptions<
  StopGeneratingLLMOutput,
  Error,
  StopGeneratingLLMInput
>;

export const useStopGeneratingLLM = (options?: Options) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: stopGeneratingLLM,
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
