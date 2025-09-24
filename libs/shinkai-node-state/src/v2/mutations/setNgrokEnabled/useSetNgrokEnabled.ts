import {
  type UseMutationOptions,
  useMutation,
  useQueryClient,
} from '@tanstack/react-query';
import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import { type SetNgrokEnabledInput, type SetNgrokEnabledOutput } from './types';
import { setNgrokEnabled } from '.';

type Options = UseMutationOptions<
  SetNgrokEnabledOutput,
  APIError,
  SetNgrokEnabledInput
>;

export const useSetNgrokEnabled = (options?: Options) => {
  const queryClient = useQueryClient();
  const { onSuccess: onOptionsSuccess, ...restOptions } = options || {};
  return useMutation({
    mutationFn: setNgrokEnabled,
    onSuccess: async (data, variables, onMutateResult, context) => {
      await queryClient.invalidateQueries({
        queryKey: [FunctionKeyV2.GET_NGROK_STATUS],
      });
      if (onOptionsSuccess) {
        onOptionsSuccess(data, variables, onMutateResult, context);
      }
    },
    ...restOptions,
  });
};
