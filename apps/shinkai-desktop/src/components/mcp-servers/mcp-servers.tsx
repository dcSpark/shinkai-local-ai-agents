import { useTranslation } from '@shinkai_network/shinkai-i18n';
import { McpServerType } from '@shinkai_network/shinkai-message-ts/api/mcp-servers/types';
import { type ImportMCPServerFromGithubURLOutput } from '@shinkai_network/shinkai-node-state/v2/mutations/importMCPServerFromGithubURL/types';
import { useSetEnableMcpServer } from '@shinkai_network/shinkai-node-state/v2/mutations/setEnableMcpServer/useSetEnableMcpServer';
import { useGetMcpServers } from '@shinkai_network/shinkai-node-state/v2/queries/getMcpServers/useGetMcpServers';
import {
  Button,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
  SearchInput,
  Skeleton,
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from '@shinkai_network/shinkai-ui';
import { Plus } from 'lucide-react';
import { useState } from 'react';
import { toast } from 'sonner';

import { useAuth } from '../../store/auth';
import { AddMcpServerModal } from './add-mcp-server-modal';
import { AddMcpServerWithGithubModal } from './add-mcp-server-with-github-modal';
import { ComposioMcpServers } from './composio-mcp-servers';
import { McpServerCard } from './mcp-server-card';

export const MCP_SERVER_ID = 'shinkai-mcp-server';

export const McpServers = () => {
  const { t } = useTranslation();
  const auth = useAuth((state) => state.auth);
  const [isAddMcpServerModalOpen, setIsAddMcpServerModalOpen] = useState(false);
  const [
    isAddMcpServerWithGithubModalOpen,
    setIsAddMcpServerWithGithubModalOpen,
  ] = useState(false);
  const [searchQuery, setSearchQuery] = useState('');
  const [initialDataForManualModal, setInitialDataForManualModal] = useState<
    ImportMCPServerFromGithubURLOutput | undefined
  >(undefined);

  const { data: mcpServers, isLoading } = useGetMcpServers({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
  });

  const { mutateAsync: setEnableMcpServer } = useSetEnableMcpServer({
    onSuccess: () => {
      toast.success(t('mcpServers.statusUpdated'));
    },
    onError: (error: Error) => {
      toast.error(t('mcpServers.statusUpdateFailed'), {
        description: error?.message,
      });
    },
  });

  const handleToggleEnabled = async (
    serverId: number,
    currentEnabled: boolean,
  ) => {
    if (!auth) return;

    try {
      await setEnableMcpServer({
        nodeAddress: auth.node_address,
        token: auth.api_v2_key,
        mcpServerId: serverId,
        isEnabled: !currentEnabled,
      });
    } catch (error) {
      toast.error(t('mcpServers.statusUpdateFailed'), {
        description: error instanceof Error ? error.message : 'Unknown error',
      });
    }
  };

  const filteredServers = (mcpServers ?? []).filter(
    (server) =>
      server.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
      server.type.toLowerCase().includes(searchQuery.toLowerCase()),
  );

  const installedMcpServersUrl = new Set(
    (mcpServers ?? [])
      .filter((server) => server.type === McpServerType.Sse)
      .map((server) => server.url),
  );

  return (
    <>
      <Tabs defaultValue="explore_composio">
        <TabsList className="border-divider flex h-auto justify-start gap-4 rounded-full bg-transparent px-0.5 py-1">
          <TabsTrigger
            className="data-[state=active]:bg-bg-quaternary text-text-tertiary border-divider data-[state=active]:text-text-default h-full gap-2 rounded-full border bg-transparent px-4 py-2 text-xs font-medium"
            value="explore_composio"
          >
            Explore Composio MCPs
          </TabsTrigger>
          <TabsTrigger
            className="data-[state=active]:bg-bg-quaternary text-text-tertiary border-divider data-[state=active]:text-text-default h-full gap-2 rounded-full border bg-transparent px-4 py-2 text-xs font-medium"
            value="my_mcp_servers"
          >
            My MCP Servers
          </TabsTrigger>
        </TabsList>
        <TabsContent value="my_mcp_servers">
          <div className="mx-auto flex flex-col">
            <div className="flex items-center justify-between gap-4">
              <SearchInput
                classNames={{ input: 'bg-transparent' }}
                onChange={(e) => {
                  setSearchQuery(e.target.value);
                }}
                placeholder={t('common.searchPlaceholder')}
                value={searchQuery}
              />

              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button className="min-w-[100px]" size="md">
                    <Plus className="h-4 w-4" />
                    <span>Add MCP Server</span>
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end" className="p-1.5 px-2">
                  <DropdownMenuItem
                    onClick={() => setIsAddMcpServerModalOpen(true)}
                  >
                    {t('mcpServers.manualSetup')}
                  </DropdownMenuItem>
                  <DropdownMenuItem
                    onClick={() => setIsAddMcpServerWithGithubModalOpen(true)}
                  >
                    Add from GitHub
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
            </div>

            {isLoading ? (
              <div className="flex items-center justify-center py-10">
                <div className="grid w-full animate-pulse grid-cols-[1fr_120px_40px_115px_36px] gap-5">
                  {[...Array(3)].map((_, i) => (
                    <div className="contents" key={i}>
                      <Skeleton className="h-12 rounded" />
                      <Skeleton className="h-12 rounded" />
                      <Skeleton className="h-12 rounded" />
                      <Skeleton className="h-12 rounded" />
                      <Skeleton className="h-12 rounded" />
                    </div>
                  ))}
                </div>
              </div>
            ) : filteredServers && filteredServers.length > 0 ? (
              <div className="divide-divider grid max-h-[calc(100vh-300px)] grid-cols-1 divide-y overflow-y-auto py-4">
                {filteredServers.map((server) => (
                  <McpServerCard
                    key={server.id}
                    onToggleEnabled={handleToggleEnabled}
                    server={server}
                  />
                ))}
              </div>
            ) : (
              <div className="flex flex-col items-center justify-center gap-2 py-8">
                <p className="text-text-secondary text-sm">
                  {t('mcpServers.noServersFound')}
                </p>
              </div>
            )}
          </div>
        </TabsContent>
        <TabsContent value="explore_composio">
          <ComposioMcpServers installedMcpServers={mcpServers ?? []} />
        </TabsContent>
      </Tabs>

      <AddMcpServerModal
        initialData={initialDataForManualModal}
        isOpen={isAddMcpServerModalOpen}
        mode="Create"
        onClose={() => {
          setIsAddMcpServerModalOpen(false);
          setInitialDataForManualModal(undefined);
        }}
        onSuccess={() => {
          setIsAddMcpServerModalOpen(false);
          setInitialDataForManualModal(undefined);
        }}
      />
      <AddMcpServerWithGithubModal
        isOpen={isAddMcpServerWithGithubModalOpen}
        onClose={() => setIsAddMcpServerWithGithubModalOpen(false)}
        onSuccess={(data) => {
          setIsAddMcpServerWithGithubModalOpen(false);
          setInitialDataForManualModal(data);
          setIsAddMcpServerModalOpen(true);
        }}
      />
    </>
  );
};
