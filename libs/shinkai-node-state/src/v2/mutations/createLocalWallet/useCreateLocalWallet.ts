import {
  type UseMutationOptions,
  useQueryClient,
  useMutation,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import {
  type CreateLocalWalletInput,
  type CreateLocalWalletOutput,
} from './types';
import { createLocalWallet } from '.';

type Options = UseMutationOptions<
  CreateLocalWalletOutput,
  APIError,
  CreateLocalWalletInput
>;

export const useCreateLocalWallet = (options?: Options) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: createLocalWallet,
    ...options,
    onSuccess: async (response, variables, onMutateResult, context) => {
      await queryClient.invalidateQueries({
        queryKey: [
          FunctionKeyV2.GET_WALLET_LIST,
          {
            token: variables.token,
            nodeAddress: variables.nodeAddress,
          },
        ],
      });
      if (options?.onSuccess) {
        options.onSuccess(response, variables, onMutateResult, context);
      }
    },
  });
};
