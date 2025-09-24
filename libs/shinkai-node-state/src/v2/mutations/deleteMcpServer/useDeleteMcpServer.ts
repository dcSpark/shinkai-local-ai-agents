import { type McpServer } from '@shinkai_network/shinkai-message-ts/api/mcp-servers/types';
import {
  useMutation,
  type UseMutationOptions,
  useQueryClient,
} from '@tanstack/react-query';

import { FunctionKeyV2 } from '../../constants';
import { type DeleteMcpServerInput } from './types';
import { deleteMcpServer } from './index';

export const useDeleteMcpServer = (
  options?: UseMutationOptions<McpServer, Error, DeleteMcpServerInput>,
) => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (input: DeleteMcpServerInput) => {
      const { deleted_mcp_server } = await deleteMcpServer(input);
      return deleted_mcp_server;
    },
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
