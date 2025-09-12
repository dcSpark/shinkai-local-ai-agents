import { setPreferences } from '@shinkai_network/shinkai-message-ts/api/general/index';
import {
  QueryClient,
  type QueryObserverOptions,
  useMutation,
  type UseMutationOptions,
  useQuery,
  useQueryClient,
  type UseQueryResult,
} from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import { relaunch } from '@tauri-apps/plugin-process';

import { type ShinkaiNodeOptions } from './shinkai-node-manager-client-types';

// Client

export const shinkaiNodeQueryClient = new QueryClient();

// Queries
export const useShinkaiNodeIsRunningQuery = (
  options?: Omit<QueryObserverOptions, 'queryKey'>,
): UseQueryResult<boolean, Error> => {
  const query = useQuery({
    queryKey: ['shinkai_node_is_running'],
    queryFn: (): Promise<boolean> => invoke('shinkai_node_is_running'),
    ...options,
  });
  return { ...query } as UseQueryResult<boolean, Error>;
};
export const useShinkaiNodeGetOptionsQuery = (
  options?: Omit<QueryObserverOptions, 'queryKey'>,
): UseQueryResult<ShinkaiNodeOptions, Error> => {
  const query = useQuery({
    queryKey: ['shinkai_node_get_options'],
    queryFn: (): Promise<ShinkaiNodeOptions> =>
      invoke('shinkai_node_get_options'),
    ...options,
  });
  return { ...query } as UseQueryResult<ShinkaiNodeOptions, Error>;
};
export const useShinkaiNodeGetOllamaApiUrlQuery = (
  options?: Omit<QueryObserverOptions, 'queryKey'>,
): UseQueryResult<string, Error> => {
  const query = useQuery({
    queryKey: ['shinkai_node_get_ollama_api_url'],
    queryFn: (): Promise<string> => invoke('shinkai_node_get_ollama_api_url'),
    ...options,
  });
  return { ...query } as UseQueryResult<string, Error>;
};
export const useShinkaiNodeGetDefaultModel = (
  options?: QueryObserverOptions,
): UseQueryResult<string, Error> => {
  const query = useQuery({
    queryKey: ['shinkai_node_get_default_model'],
    queryFn: (): Promise<string> => invoke('shinkai_node_get_default_model'),
    ...options,
  });
  return { ...query } as UseQueryResult<string, Error>;
};
export const useShinkaiNodeGetOllamaVersionQuery = (
  options?: Omit<QueryObserverOptions, 'queryKey'>,
): UseQueryResult<string, Error> => {
  const query = useQuery({
    queryKey: ['shinkai_node_get_ollama_version'],
    queryFn: (): Promise<string> => invoke('shinkai_node_get_ollama_version'),
    ...options,
  });
  return { ...query } as UseQueryResult<string, Error>;
};

export const useShinkaiNodeGetDefaultEmbeddingModelQuery = (
  options?: Omit<QueryObserverOptions, 'queryKey'>,
): UseQueryResult<string, Error> => {
  const query = useQuery({
    queryKey: ['shinkai_node_get_default_embedding_model'],
    queryFn: (): Promise<string> => invoke('shinkai_node_get_default_embedding_model'),
    staleTime: Infinity, // This is a static default value, never changes
    ...options,
  });
  return { ...query } as UseQueryResult<string, Error>;
};

// Mutations
export const useShinkaiNodeSpawnMutation = (options?: UseMutationOptions) => {
  const queryClient = useQueryClient();
  const response = useMutation({
    mutationFn: () => {
      return invoke('shinkai_node_spawn');
    },
    ...options,
    onSuccess: (...onSuccessParameters) => {
      void queryClient.invalidateQueries({
        queryKey: ['shinkai_node_is_running'],
      });
      if (options?.onSuccess) {
        options.onSuccess(...onSuccessParameters);
      }
    },
  });
  return { ...response };
};

export const useShinkaiNodeKillMutation = (options?: UseMutationOptions) => {
  const queryClient = useQueryClient();
  const response = useMutation({
    mutationFn: async (): Promise<void> => {
      return invoke('shinkai_node_kill');
    },
    ...options,
    onSuccess: (...onSuccessParameters) => {
      void queryClient.invalidateQueries({
        queryKey: ['shinkai_node_is_running'],
      });
      if (options?.onSuccess) {
        options.onSuccess(...onSuccessParameters);
      }
    },
  });
  return { ...response };
};

export type ShinkaiNodeRemoveStorageOptions = {
  preserveKeys: boolean;
};
export const useShinkaiNodeRemoveStorageMutation = (
  options?: UseMutationOptions<
    void,
    Error,
    Partial<ShinkaiNodeRemoveStorageOptions>
  >,
) => {
  const response = useMutation({
    mutationFn: async (
      options: Partial<ShinkaiNodeRemoveStorageOptions>,
    ): Promise<void> => {
      await invoke('shinkai_node_set_default_options');
      return invoke('shinkai_node_remove_storage', {
        preserveKeys: options?.preserveKeys,
      });
    },
    ...options,
  });
  return { ...response };
};

export const useShinkaiNodeSetOptionsMutation = (
  options?: UseMutationOptions<
    Partial<ShinkaiNodeOptions>,
    Error,
    ShinkaiNodeOptions
  >,
) => {
  const queryClient = useQueryClient();
  const response = useMutation({
    mutationFn: (
      shinkaiNodeOptions: Partial<ShinkaiNodeOptions>,
    ): Promise<ShinkaiNodeOptions> => {
      return invoke('shinkai_node_set_options', {
        options: shinkaiNodeOptions,
      });
    },
    ...options,
    onSuccess: (...onSuccessParameters) => {
      void queryClient.invalidateQueries({
        queryKey: ['shinkai_node_get_options'],
      });
      if (options?.onSuccess) {
        options.onSuccess(...onSuccessParameters);
      }
    },
  });
  return { ...response };
};

export const useShinkaiNodeSetDefaultOptionsMutation = (
  options?: UseMutationOptions<ShinkaiNodeOptions, Error, void>,
) => {
  const queryClient = useQueryClient();
  const response = useMutation({
    mutationFn: (): Promise<ShinkaiNodeOptions> => {
      return invoke('shinkai_node_set_default_options', {});
    },
    ...options,
    onSuccess: (...onSuccessParameters) => {
      void queryClient.invalidateQueries({
        queryKey: ['shinkai_node_set_default_options'],
      });
      if (options?.onSuccess) {
        options.onSuccess(...onSuccessParameters);
      }
    },
  });
  return { ...response };
};

export const useShinkaiNodeRespawnMutation = (options?: UseMutationOptions) => {
  const queryClient = useQueryClient();
  const response = useMutation({
    mutationFn: async () => {
      await invoke('shinkai_node_kill');
      await relaunch();
    },
    ...options,
    onSuccess: (...onSuccessParameters) => {
      void queryClient.invalidateQueries({
        queryKey: ['shinkai_node_is_running'],
      });
      if (options?.onSuccess) {
        options.onSuccess(...onSuccessParameters);
      }
    },
  });
  return { ...response };
};

export const useShinkaiNodeSetDefaultLlmProviderMutation = (
  options?: UseMutationOptions<void, Error, string>,
) => {
  const response = useMutation({
    mutationFn: async (defaultLlmProvider: string): Promise<void> => {
      if (!defaultLlmProvider) {
        throw new Error('Default LLM provider is required');
      }
      return Promise.resolve();
    },
    ...options,
  });
  return { ...response };
};

export const shinkaiNodeSetDefaultLlmProvider = async (
  defaultLlmProvider: string,
  nodeAddress: string,
  apiToken: string,
): Promise<void> => {
  if (!defaultLlmProvider || !nodeAddress || !apiToken) {
    throw new Error(
      'Default LLM provider, node address, and API token are required',
    );
  }

  await setPreferences(nodeAddress, apiToken, {
    default_llm_provider: defaultLlmProvider,
  });

  return Promise.resolve();
};
