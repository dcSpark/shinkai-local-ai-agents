import {
  type UseMutationOptions,
  useQueryClient,
  useMutation,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import {
  type RestoreCoinbaseMpcWalletInput,
  type RestoreCoinbaseMpcWalletOutput,
} from './types';
import { restoreCoinbaseMPCWallet } from '.';

type Options = UseMutationOptions<
  RestoreCoinbaseMpcWalletOutput,
  APIError,
  RestoreCoinbaseMpcWalletInput
>;

export const useRestoreCoinbaseMpcWallet = (options?: Options) => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: restoreCoinbaseMPCWallet,
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
