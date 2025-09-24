import {
  type MutationOptions,
  useMutation,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import {
  type SetEnableMcpServerInput,
  type SetEnableMcpServerOutput,
} from './types';
import { setEnableMcpServer } from './index';

type Options = MutationOptions<
  SetEnableMcpServerOutput,
  Error,
  SetEnableMcpServerInput
>;

export const useSetEnableMcpServer = (options?: Options) => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (input: SetEnableMcpServerInput) => setEnableMcpServer(input),
    ...options,
    onSuccess: async (...args) => {
      await queryClient.invalidateQueries({
        queryKey: [FunctionKeyV2.GET_MCP_SERVERS],
      });

      if (options?.onSuccess) {
        options.onSuccess(...args);
      }
    },
  });
};
