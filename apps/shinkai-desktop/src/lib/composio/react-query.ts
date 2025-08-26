import { McpServerType } from '@shinkai_network/shinkai-message-ts/api/mcp-servers/types';
import { useAddMcpServer } from '@shinkai_network/shinkai-node-state/v2/mutations/addMcpServer/useAddMcpServer';
import {
  useMutation,
  type UseMutationOptions,
  useQuery,
} from '@tanstack/react-query';

import { ComposioApi } from './composio-api';

const api = new ComposioApi();

// Query keys for caching and invalidation
export const composioKeys = {
  all: ['composio'] as const,
  apps: () => [...composioKeys.all, 'apps'] as const,
  app: (id: string) => [...composioKeys.apps(), id] as const,
};

// Hook to fetch all apps
export const useApps = () => {
  return useQuery({
    queryKey: composioKeys.apps(),
    queryFn: () => api.getApps(),
  });
};

// Hook to fetch a single app by ID
export const useApp = (appId: string) => {
  return useQuery({
    queryKey: composioKeys.app(appId),
    queryFn: () => api.getApp(appId),
    enabled: !!appId, // Only run the query if we have an appId
  });
};

type InstallAppParams = {
  appId: string;
  auth: {
    node_address: string;
    api_v2_key: string;
  };
};

// Mutation to get client ID and install app
export const useInstallApp = (
  options: UseMutationOptions<any, any, InstallAppParams, any>,
) => {
  const { mutateAsync: addMcpServer } = useAddMcpServer();

  return useMutation({
    ...options,
    mutationFn: async ({ appId, auth }: InstallAppParams) => {
      const app = await api.getApp(appId);
      const httpUrl = await api.generateHttpUrlForAppId(appId);
      if (!httpUrl) {
        throw new Error('Composio SSE url not found');
      }
      await addMcpServer({
        nodeAddress: auth.node_address,
        token: auth.api_v2_key,
        name: app.name,
        type: McpServerType.Http,
        url: httpUrl.toString(),
        is_enabled: true,
      });
      return httpUrl;
    },
  });
};
