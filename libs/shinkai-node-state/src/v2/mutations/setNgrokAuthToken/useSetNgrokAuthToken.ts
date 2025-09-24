import {
  type UseMutationOptions,
  useMutation,
  useQueryClient,
} from '@tanstack/react-query';
import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import {
  type SetNgrokAuthTokenInput,
  type SetNgrokAuthTokenOutput,
} from './types';
import { setNgrokAuthToken } from '.';

type Options = UseMutationOptions<
  SetNgrokAuthTokenOutput,
  APIError,
  SetNgrokAuthTokenInput
>;

export const useSetNgrokAuthToken = (options?: Options) => {
  const queryClient = useQueryClient();
  const { onSuccess: onOptionsSuccess, ...restOptions } = options || {};
  return useMutation({
    mutationFn: setNgrokAuthToken,
    onSuccess: async (...args) => {
      await queryClient.invalidateQueries({
        queryKey: [FunctionKeyV2.GET_NGROK_STATUS],
      });
      if (onOptionsSuccess) {
        onOptionsSuccess(...args);
      }
    },
    ...restOptions,
  });
};
