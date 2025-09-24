import {
  type UseMutationOptions,
  useQueryClient,
  useMutation,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import {
  type RestoreLocalWalletInput,
  type RestoreLocalWalletOutput,
} from './types';
import { restoreLocalWallet } from '.';

type Options = UseMutationOptions<
  RestoreLocalWalletOutput,
  APIError,
  RestoreLocalWalletInput
>;

export const useRestoreLocalWallet = (options?: Options) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: restoreLocalWallet,
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
