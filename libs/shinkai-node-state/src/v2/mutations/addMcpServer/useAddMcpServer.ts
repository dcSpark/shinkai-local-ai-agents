import { type McpServer } from '@shinkai_network/shinkai-message-ts/api/mcp-servers/types';
import {
  useMutation,
  type UseMutationOptions,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type APIError } from '../../types';
import { type AddMcpServerInput } from './types';
import { addMcpServer } from './index';

export const useAddMcpServer = (
  options?: UseMutationOptions<McpServer, APIError, AddMcpServerInput>,
) => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (input: AddMcpServerInput) => addMcpServer(input),
    ...options,
    onSuccess: async (data, variables, onMutateResult, context) => {
      await queryClient.invalidateQueries({
        queryKey: [FunctionKeyV2.GET_MCP_SERVERS],
      });
      if (options?.onSuccess) {
        await options.onSuccess(data, variables, onMutateResult, context);
      }
    },
  });
};
