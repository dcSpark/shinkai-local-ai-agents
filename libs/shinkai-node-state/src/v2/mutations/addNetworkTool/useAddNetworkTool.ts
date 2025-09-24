import {
  useMutation,
  type UseMutationOptions,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import { type AddNetworkToolInput } from './types';
import { addNetworkTool } from './index';

export const useAddNetworkTool = (
  options?: UseMutationOptions<any, APIError, AddNetworkToolInput>,
) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: addNetworkTool,
    ...options,
    onSuccess: async (...args) => {
      await queryClient.invalidateQueries({
        queryKey: [FunctionKeyV2.GET_INSTALLED_NETWORK_TOOLS],
      });
      if (options?.onSuccess) {
        await options.onSuccess(...args);
      }
    },
  });
};
