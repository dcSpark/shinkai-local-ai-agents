import { useTranslation } from '@shinkai_network/shinkai-i18n';
import {
  type McpServer,
  McpServerType,
} from '@shinkai_network/shinkai-message-ts/api/mcp-servers/types';
import { buildInboxIdFromJobId } from '@shinkai_network/shinkai-message-ts/utils';
import { DEFAULT_CHAT_CONFIG } from '@shinkai_network/shinkai-node-state/v2/constants';
import { useCreateJob } from '@shinkai_network/shinkai-node-state/v2/mutations/createJob/useCreateJob';
import { useDeleteMcpServer } from '@shinkai_network/shinkai-node-state/v2/mutations/deleteMcpServer/useDeleteMcpServer';
import { Button, SearchInput, Skeleton } from '@shinkai_network/shinkai-ui';
import { PlusIcon, Trash } from 'lucide-react';
import { useMemo, useState } from 'react';
import { useNavigate } from 'react-router';
import { toast } from 'sonner';
import { type App } from '../../lib/composio/composio-api';
import { useApps, useInstallApp } from '../../lib/composio/react-query';
import { useAuth } from '../../store/auth';
import { useSettings } from '../../store/settings';
import { ComposioAppDetailsModal } from './composio-app-details-modal';

export const ComposioMcpServers = ({
  installedMcpServers,
}: {
  installedMcpServers: McpServer[];
}) => {
  const { t } = useTranslation();
  const { data: composioApps, isLoading: isLoadingComposio } = useApps();
  const [searchQuery, setSearchQuery] = useState('');
  const [selectedApp, setSelectedApp] = useState<string | null>(null);
  const [installingAppIds, setInstallingAppIds] = useState<Set<string>>(
    new Set(),
  );
  const auth = useAuth((state) => state.auth);

  const handleAppClick = async (app: App) => {
    setSelectedApp(app.id);
  };

  const navigate = useNavigate();
  const defaultAgentId = useSettings((state) => state.defaultAgentId);

  const { mutateAsync: createJob } = useCreateJob({
    onError: (error) => {
      toast.error('Failed to send message', {
        description: error.response?.data?.message ?? error.message,
      });
    },
    onSuccess: async (data) => {
      void navigate(
        `/inboxes/${encodeURIComponent(buildInboxIdFromJobId(data.jobId))}`,
      );
    },
  });

  const { mutateAsync: installApp } = useInstallApp({
    onSuccess: (data) => {
      toast.success(
        t('mcpServers.composio.installSuccess', {
          appName: data.name,
        }),
      );
    },
    onError: () => {
      toast.error(t('mcpServers.composio.installFailed'));
    },
    onSettled: (_, __, variables) => {
      setInstallingAppIds((prev) => {
        const newSet = new Set(prev);
        newSet.delete(variables.appId);
        return newSet;
      });
    },
  });
  const { mutateAsync: deleteMcpServer, isPending: isLoadingDeleteMcpServer } =
    useDeleteMcpServer({
      onSuccess: (data) => {
        toast.success(
          t('mcpServers.composio.uninstallSuccess', {
            appName: data.name,
          }),
        );
      },
      onError: () => {
        toast.error(t('mcpServers.composio.uninstallFailed'));
      },
    });

  const filteredApps = useMemo(() => {
    if (!searchQuery) {
      return composioApps;
    }
    return composioApps?.filter(
      (app) =>
        app.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
        app.description.toLowerCase().includes(searchQuery.toLowerCase()),
    );
  }, [composioApps, searchQuery]);

  const handleInstall = async (app: App) => {
    if (!auth) {
      toast.error(t('mcpServers.composio.loginRequired'));
      return;
    }
    setInstallingAppIds((prev) => new Set([...prev, app.id]));
    await installApp({
      appId: app.id,
      auth: {
        node_address: auth.node_address,
        api_v2_key: auth.api_v2_key,
      },
    });
    await createJob({
      nodeAddress: auth.node_address,
      token: auth.api_v2_key,
      llmProvider: defaultAgentId,
      content: `Initiate connection with ${app.id}`,
      isHidden: false,
      chatConfig: {
        stream: DEFAULT_CHAT_CONFIG.stream,
        custom_prompt: '',
        temperature: DEFAULT_CHAT_CONFIG.temperature,
        top_p: DEFAULT_CHAT_CONFIG.top_p,
        top_k: DEFAULT_CHAT_CONFIG.top_k,
        use_tools: true,
        thinking: DEFAULT_CHAT_CONFIG.thinking,
        reasoning_effort: DEFAULT_CHAT_CONFIG.reasoning_effort,
      },
    });
  };

  const handleUninstall = async (app: App) => {
    if (!auth) return;
    const mcpServerId = installMcpServersIndexedByComposioAppId.get(app.id)?.id;
    if (!mcpServerId) {
      return;
    }
    await deleteMcpServer({
      id: mcpServerId,
      nodeAddress: auth.node_address,
      token: auth.api_v2_key,
    });
  };

  const getComposioAppIdFromSseUrl = (url: string) => {
    let composioAppIdFromSseUrlRegexp =
      /https:\/\/mcp\.composio\.dev\/partner\/composio\/([a-z0-9]+)\?customerId/;
    let composioAppIdFromHttpUrlRegexp =
      /https:\/\/mcp\.composio\.dev\/partner\/composio\/([a-z0-9]+)\/mcp\?customerId/;
    let composioAppIdFromSseUrl = composioAppIdFromSseUrlRegexp.exec(url);
    let composioAppIdFromHttpUrl = composioAppIdFromHttpUrlRegexp.exec(url);
    if (composioAppIdFromSseUrl) {
      return composioAppIdFromSseUrl[1];
    } else if (composioAppIdFromHttpUrl) {
      return composioAppIdFromHttpUrl[1];
    }
    return null;
  };

  const installMcpServersIndexedByComposioAppId = useMemo(() => {
    const indexed = new Map<string, McpServer>();
    for (const server of installedMcpServers) {
      if (
        server.type === McpServerType.Sse ||
        server.type === McpServerType.Http
      ) {
        const key = getComposioAppIdFromSseUrl(server.url);

        if (key) {
          indexed.set(key, server);
        }
      }
    }
    return indexed;
  }, [installedMcpServers]);

  return (
    <div className="mx-auto flex flex-col">
      <SearchInput
        classNames={{
          input: 'bg-transparent',
        }}
        onChange={(e) => {
          setSearchQuery(e.target.value);
        }}
        placeholder={t('common.searchPlaceholder')}
        value={searchQuery}
      />
      <div className="grid grid-cols-1 gap-2.5 overflow-y-auto py-4 pr-2">
        {isLoadingComposio
          ? [...Array(3)].map((_, i) => (
              <Skeleton
                key={i}
                className="flex h-16 items-center justify-between rounded-lg border p-4"
              ></Skeleton>
            ))
          : filteredApps?.map((app) => (
              <div
                key={app.id}
                className="bg-bg-secondary border-divider group flex h-20 overflow-hidden rounded-2xl border p-3.5 transition-all"
                onClick={() => handleAppClick(app)}
                role="button"
              >
                <div className="flex flex-1 items-center gap-4">
                  <div className="relative shrink-0">
                    <img
                      src={app.icon}
                      alt={app.name}
                      className="size-10 rounded-lg object-cover"
                    />
                  </div>
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <h3 className="text-text-default truncate text-base font-semibold">
                        {app.name}
                      </h3>
                    </div>
                    <p className="text-text-secondary line-clamp-2 text-sm">
                      {app.description}
                    </p>
                  </div>
                </div>
                <div className="ml-4 flex flex-col items-end gap-2">
                  {!installMcpServersIndexedByComposioAppId.has(app.id) && (
                    <Button
                      onClick={(e) => {
                        e.stopPropagation();
                        void handleInstall(app);
                      }}
                      disabled={installingAppIds.has(app.id)}
                      size="sm"
                      variant="outline"
                      isLoading={installingAppIds.has(app.id)}
                    >
                      {installingAppIds.has(app.id) ? (
                        t('mcpServers.composio.adding')
                      ) : (
                        <>
                          <PlusIcon className="size-4" />
                          {t('mcpServers.composio.add')}
                        </>
                      )}
                    </Button>
                  )}

                  {installMcpServersIndexedByComposioAppId.has(app.id) && (
                    <Button
                      onClick={(e) => {
                        e.stopPropagation();
                        void handleUninstall(app);
                      }}
                      disabled={isLoadingDeleteMcpServer}
                      size="sm"
                      variant="outline"
                      isLoading={isLoadingDeleteMcpServer}
                    >
                      {isLoadingDeleteMcpServer ? (
                        t('mcpServers.composio.deleting')
                      ) : (
                        <>
                          <Trash className="h-4 w-4" />
                          {t('mcpServers.composio.delete')}
                        </>
                      )}
                    </Button>
                  )}
                </div>
              </div>
            ))}
      </div>

      {selectedApp && (
        <ComposioAppDetailsModal
          isOpen={!!selectedApp}
          onClose={() => setSelectedApp(null)}
          appId={selectedApp}
        />
      )}
    </div>
  );
};
