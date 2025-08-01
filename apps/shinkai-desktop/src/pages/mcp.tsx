import { useTranslation } from '@shinkai_network/shinkai-i18n';
import { useSetToolMcpEnabled } from '@shinkai_network/shinkai-node-state/v2/mutations/setToolMcpEnabled/useSetToolMcpEnabled';
import { useGetTools } from '@shinkai_network/shinkai-node-state/v2/queries/getToolsList/useGetToolsList';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  Badge,
  Button,
  buttonVariants,
  CopyToClipboardIcon,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
  SearchInput,
  Switch,
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
  ToggleGroup,
  ToggleGroupItem,
  Tooltip,
  TooltipContent,
  TooltipPortal,
  TooltipTrigger,
} from '@shinkai_network/shinkai-ui';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import { type TFunction } from 'i18next';
import { BoltIcon, MoveRightIcon, Plus } from 'lucide-react';
import { useMemo, useState } from 'react';
import { Link } from 'react-router';
import { toast } from 'sonner';

import { McpServers } from '../components/mcp-servers/mcp-servers';
import { handleConfigureClaude } from '../lib/external-clients/claude-desktop';
import { getDenoBinPath, ConfigError } from '../lib/external-clients/common';
import { handleConfigureCursor } from '../lib/external-clients/cursor';
import { useAuth } from '../store/auth';

export const MCP_SERVER_ID = 'shinkai-mcp-server';

type GetMCPCategory = 'all' | 'agent' | 'tool';

export const tabTriggerClassnames = cn(
  'relative flex size-full min-w-[120px] rounded-xs p-0 pt-0.5 text-sm',
  'data-[state=active]:bg-official-gray-950 data-[state=active]:text-white data-[state=active]:shadow-[0_2px_0_0_#1a1a1d]',
  'before:absolute before:top-0 before:right-0 before:left-0 before:h-0.5',
);

export const McpRegistryPage = () => {
  const [selectedTab, setSelectedTab] = useState<
    'mcp_servers' | 'expose_tools'
  >('mcp_servers');

  const { t } = useTranslation();
  return (
    <Tabs
      className="flex size-full flex-col"
      defaultValue="mcp_servers"
      onValueChange={(value) => {
        setSelectedTab(value as 'mcp_servers' | 'expose_tools');
      }}
    >
      <div className="container max-w-screen-lg">
        <div className="flex flex-col gap-5 pt-10 pb-6">
          <div className="flex justify-between gap-4">
            <div className="font-clash inline-flex items-center gap-5 text-3xl font-medium">
              <h1>{t('mcpServers.title')}</h1>
              <TabsList className="flex h-10 w-fit items-center gap-2 rounded-full bg-transparent px-1 py-1">
                <TabsTrigger
                  className={cn(
                    'flex items-center gap-4 rounded-full px-4 py-1.5 text-base font-medium transition-colors',
                    'data-[state=active]:bg-official-gray-800 data-[state=active]:text-white',
                    'data-[state=inactive]:text-text-secondary data-[state=inactive]:bg-transparent',
                    'focus-visible:outline-hidden',
                  )}
                  value="mcp_servers"
                >
                  {t('mcpServers.label')}
                  <span className="text-text-secondary inline-flex items-center gap-1 text-xs">
                    MCP <MoveRightIcon className="size-2.5" /> Shinkai
                  </span>
                </TabsTrigger>
                <TabsTrigger
                  className={cn(
                    'flex items-center gap-4 rounded-full px-4 py-1.5 text-base font-medium transition-colors',
                    'data-[state=active]:bg-official-gray-800 data-[state=active]:text-white',
                    'data-[state=inactive]:text-text-secondary data-[state=inactive]:bg-transparent',
                    'focus-visible:outline-hidden',
                  )}
                  value="expose_tools"
                >
                  {t('mcpPage.exposeToolsTab')}
                  <span className="text-text-secondary inline-flex items-center gap-1 text-xs">
                    Shinkai <MoveRightIcon className="size-2.5" /> MCP
                  </span>
                </TabsTrigger>
              </TabsList>
            </div>
          </div>
          <p className="text-text-secondary text-sm whitespace-pre-wrap">
            {selectedTab === 'mcp_servers'
              ? t('mcpPage.mcpServersDescription')
              : t('mcpPage.exposeToolsDescription')}
          </p>
          {selectedTab === 'mcp_servers' && (
            <p className="text-text-secondary text-sm">
              {t('mcpPage.helpfulLinks')}{' '}
              <a
                href="http://smithery.ai"
                target="_blank"
                rel="noopener noreferrer"
                className="underline"
              >
                smithery.ai
              </a>{' '}
              and{' '}
              <a
                href="http://composio.dev"
                target="_blank"
                rel="noopener noreferrer"
                className="underline"
              >
                composio.dev
              </a>
              .
            </p>
          )}
        </div>
        <TabsContent value="mcp_servers" className="mt-0">
          <McpServers />
        </TabsContent>
        <TabsContent value="expose_tools" className="mt-0">
          <ExposeToolsAsMcp />
        </TabsContent>
      </div>
    </Tabs>
  );
};

const ExposeToolsAsMcp = () => {
  const auth = useAuth((state) => state.auth);
  const [selectedMCCategory, setSelectedMCCategory] =
    useState<GetMCPCategory>('all');

  const [searchQuery, setSearchQuery] = useState('');

  const { data: toolsList, isPending } = useGetTools({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
    category: 'mcp_servers',
  });

  const { t } = useTranslation();

  const [customDialogOpen, setCustomDialogOpen] = useState(false);
  const [customSseUrl, setCustomSseUrl] = useState('');
  const [customCommand, setCustomCommand] = useState('');
  const [dialogOpen, setDialogOpen] = useState(false);
  const [dialogTitle, setDialogTitle] = useState('');
  const [dialogDescription, setDialogDescription] = useState('');
  const [dialogHelpText, setDialogHelpText] = useState('');
  const [jsonConfigToCopy, setJsonConfigToCopy] = useState('');

  const filteredToolsList = useMemo(() => {
    let filtered = toolsList ?? [];

    if (selectedMCCategory === 'agent') {
      filtered = filtered?.filter((tool) => tool.tool_type === 'Agent');
    } else if (selectedMCCategory === 'tool') {
      filtered = filtered?.filter((tool) => tool.tool_type !== 'Agent');
    }

    if (searchQuery) {
      filtered = filtered?.filter(
        (tool) =>
          tool.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
          tool.description?.toLowerCase().includes(searchQuery.toLowerCase()),
      );
    }

    return filtered;
  }, [toolsList, selectedMCCategory, searchQuery]);

  const handleConfigureClick = async (
    configureFn: (serverId: string, tFunc: TFunction) => Promise<void>,
    clientName: string,
  ) => {
    try {
      await configureFn(MCP_SERVER_ID, t);
    } catch (error) {
      if (error instanceof ConfigError) {
        const jsonMatch = error.helpText.match(/```json\s*([\s\S]*?)\s*```/);
        const extractedJson = jsonMatch ? jsonMatch[1].trim() : '';

        setDialogTitle(t('mcpClients.configFailTitle', { clientName }));
        setDialogDescription(
          t('mcpClients.configFailDescription', {
            errorMessage: error.message,
          }),
        );
        setDialogHelpText(error.helpText);
        setJsonConfigToCopy(extractedJson);
        setDialogOpen(true);
      } else if (error instanceof Error) {
        toast.error(`Failed to configure ${clientName}: ${error.message}`);
      } else {
        toast.error(
          `An unknown error occurred while configuring ${clientName}.`,
        );
      }
      console.error(`Configuration error for ${clientName}:`, error);
    }
  };

  const handleShowCustomInstructions = async () => {
    // Generate SSE URL (same logic as cursor)
    const nodeUrl = auth?.node_address || 'http://localhost:9550'; // Default or get from auth
    const sseUrl = `${nodeUrl}/mcp/sse`;
    setCustomSseUrl(sseUrl);

    // Generate Command (using deno as requested, referencing the SSE URL)
    const denoBinPath = await getDenoBinPath(); // Get deno path
    const command = `${denoBinPath} run -A npm:supergateway --sse ${sseUrl}`; // Use deno
    setCustomCommand(command);

    setCustomDialogOpen(true); // Open the dialog
  };

  return (
    <>
      <div className="flex justify-between gap-4">
        <div className="flex flex-1 items-center justify-between gap-4">
          <SearchInput
            classNames={{
              input: 'bg-transparent',
            }}
            onChange={(e) => {
              setSearchQuery(e.target.value);
            }}
            value={searchQuery}
          />
        </div>
        <div className="flex items-center justify-start gap-2">
          <ToggleGroup
            className="border-divider flex justify-start rounded-full border bg-transparent px-0.5 py-0.5"
            onValueChange={(value) => {
              if (!value) return;
              setSelectedMCCategory(value as GetMCPCategory);
            }}
            type="single"
            value={selectedMCCategory}
          >
            <ToggleGroupItem
              className="data-[state=on]:bg-official-gray-850 text-text-secondary rounded-full bg-transparent px-5 py-1.5 text-xs font-medium data-[state=on]:text-white"
              key="all"
              size="sm"
              value="all"
            >
              {t('common.all')}
            </ToggleGroupItem>
            {[
              {
                label: t('common.agent'),
                value: 'agent',
              },
              {
                label: t('common.tool'),
                value: 'tool',
              },
            ].map((tool) => (
              <ToggleGroupItem
                className="data-[state=on]:bg-official-gray-850 text-text-secondary rounded-full bg-transparent px-5 py-1.5 text-xs font-medium data-[state=on]:text-white"
                key={tool.value}
                size="sm"
                value={tool.value}
              >
                {tool.label}
              </ToggleGroupItem>
            ))}
          </ToggleGroup>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button size="md" variant="outline">
                {t('mcpClients.connectExternalClient')}
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="center" className="p-1 px-2">
              <DropdownMenuItem
                onClick={() =>
                  handleConfigureClick(
                    (serverId, tFunc) => handleConfigureClaude(serverId, tFunc),
                    'Claude Desktop',
                  )
                }
              >
                Claude Desktop
              </DropdownMenuItem>
              <DropdownMenuItem
                onClick={() =>
                  handleConfigureClick(
                    (serverId, tFunc) => handleConfigureCursor(serverId, tFunc),
                    'Cursor',
                  )
                }
              >
                Cursor
              </DropdownMenuItem>
              <DropdownMenuItem onClick={handleShowCustomInstructions}>
                Custom
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </div>
      <div className="mx-auto flex flex-col gap-2">
        <div className="divide-official-gray-780 grid grid-cols-1 divide-y py-4">
          {filteredToolsList?.length === 0 ? (
            <div className="flex flex-col items-center justify-center gap-2 py-8">
              <p className="text-text-secondary text-sm">
                {t('tools.noToolsInCategory')}
              </p>
            </div>
          ) : (
            filteredToolsList?.map((tool) => (
              <McpCard
                hideToolTypeBadge={selectedMCCategory !== 'all'}
                key={tool.tool_router_key}
                requiredConfig={(tool.config ?? []).some(
                  (config) =>
                    config.BasicConfig?.required &&
                    !config.BasicConfig?.key_value,
                )}
                toolAuthor={tool.author}
                toolDescription={tool.description}
                toolEnabled={tool.enabled}
                toolMcpEnabled={tool.mcp_enabled ?? false}
                toolName={tool.name}
                toolRouterKey={tool.tool_router_key}
                toolType={tool.tool_type === 'Agent' ? 'Agent' : 'Tool'}
              />
            ))
          )}
        </div>

        {isPending && (
          <div className="divide-official-gray-780 grid grid-cols-1 divide-y py-4">
            {Array.from({ length: 8 }).map((_, idx) => (
              <div
                className={cn(
                  'grid animate-pulse items-center gap-5 rounded-xs px-2 py-3 pr-4 text-left text-sm',
                )}
                key={idx}
              >
                <div className="flex w-full flex-1 flex-col gap-3">
                  <span className="bg-official-gray-800 h-4 w-36 rounded-xs" />
                  <div className="flex flex-col gap-1">
                    <span className="bg-official-gray-800 h-3 w-full rounded-xs" />
                    <span className="bg-official-gray-800 h-3 w-2/4 rounded-xs" />
                  </div>
                </div>
                <span className="bg-official-gray-800 h-5 w-[36px] rounded-full" />
              </div>
            ))}
          </div>
        )}

        <AlertDialog onOpenChange={setDialogOpen} open={dialogOpen}>
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>{dialogTitle}</AlertDialogTitle>
              <AlertDialogDescription>
                {dialogDescription}
              </AlertDialogDescription>
            </AlertDialogHeader>
            <div className="bg-official-gray-800 my-4 max-h-[60vh] overflow-y-auto rounded-sm p-3 text-sm">
              <pre>
                <code>{dialogHelpText}</code>
              </pre>
            </div>
            <AlertDialogFooter className="flex justify-between gap-4">
              <AlertDialogCancel>{t('oauth.close')}</AlertDialogCancel>
              <AlertDialogAction
                disabled={!jsonConfigToCopy}
                onClick={() => {
                  if (jsonConfigToCopy) {
                    void navigator.clipboard.writeText(jsonConfigToCopy);
                    toast.success(t('mcpClients.copyJsonSuccess'));
                    setDialogOpen(false);
                  }
                }}
              >
                {t('mcpClients.copyJsonButton')}
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>

        <AlertDialog onOpenChange={setCustomDialogOpen} open={customDialogOpen}>
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>{t('mcpClients.customTitle')}</AlertDialogTitle>
              <AlertDialogDescription>
                {t('mcpClients.customDescriptionPrimary')}
              </AlertDialogDescription>
            </AlertDialogHeader>
            <div className="bg-official-gray-800 relative my-2 rounded-sm p-3">
              <pre className="mt-1 text-sm whitespace-pre-wrap">
                <code>{customSseUrl}</code>
                <CopyToClipboardIcon
                  className={cn(
                    'text-text-secondary absolute top-2 right-2 h-7 w-7 border border-gray-200 bg-transparent hover:bg-gray-300 [&>svg]:h-3 [&>svg]:w-3',
                  )}
                  string={customSseUrl}
                />
              </pre>
            </div>
            <AlertDialogDescription className="mt-4">
              {t('mcpClients.customDescriptionSecondary')}
            </AlertDialogDescription>
            <div className="bg-official-gray-800 relative my-2 rounded-sm p-3">
              <pre className="mt-1 text-sm whitespace-pre-wrap">
                <code>{customCommand}</code>
                <CopyToClipboardIcon
                  className={cn(
                    'text-text-secondary absolute top-2 right-2 h-7 w-7 border border-gray-200 bg-transparent hover:bg-gray-300 [&>svg]:h-3 [&>svg]:w-3',
                  )}
                  string={customCommand}
                />
              </pre>
            </div>
            <AlertDialogFooter className="mt-4 flex justify-end gap-4">
              <AlertDialogCancel>{t('oauth.close')}</AlertDialogCancel>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
      </div>
    </>
  );
};

const McpCard = ({
  toolRouterKey,
  toolName,
  toolAuthor,
  toolDescription,
  toolMcpEnabled,
  toolEnabled,
  toolType,
  hideToolTypeBadge,
  requiredConfig,
}: {
  toolRouterKey: string;
  toolName: string;
  toolAuthor: string;
  toolDescription: string;
  toolMcpEnabled: boolean;
  toolEnabled: boolean;
  toolType: string;
  hideToolTypeBadge?: boolean;
  requiredConfig?: boolean;
}) => {
  const auth = useAuth((state) => state.auth);
  const { t } = useTranslation();
  const { mutateAsync: setToolMcpEnabled } = useSetToolMcpEnabled({
    onError: (error) => {
      toast.error(t('mcpServers.updateFailed'), {
        description: error.response?.data?.message ?? error.message,
      });
    },
  });

  return (
    <div
      className={cn(
        'flex items-center justify-between gap-5 rounded-xs px-2 py-4 pr-4 text-left text-sm',
      )}
    >
      <div className="flex flex-col gap-2.5">
        <div className="flex items-center gap-2">
          <Link
            className="font-medium text-white hover:underline"
            to={`/tools/${toolRouterKey}`}
          >
            {toolName}
          </Link>
          {!hideToolTypeBadge && (
            <Badge
              className="text-official-gray-300 text-xs font-normal"
              variant="outline"
            >
              {toolType}
            </Badge>
          )}
        </div>
        <p className="text-text-secondary line-clamp-2 text-sm">
          {toolDescription}
        </p>
        <p className="text-text-secondary text-xs">{toolAuthor}</p>
      </div>
      <div className="text-text-secondary flex items-center gap-4 text-xs">
        {requiredConfig && (
          <Link
            className={cn(
              buttonVariants({
                variant: 'outline',
                size: 'sm',
              }),
              'h-auto min-h-auto rounded-md py-2',
            )}
            to={`/tools/${toolRouterKey}`}
          >
            <BoltIcon className="mr-1.5 h-4 w-4" />
            {t('common.configure')}
          </Link>
        )}
        {!requiredConfig && (
          <Tooltip>
            <TooltipTrigger asChild>
              <div>
                <Switch
                  checked={toolMcpEnabled}
                  disabled={!toolEnabled}
                  onCheckedChange={async () => {
                    if (!auth) return;
                    if (toolEnabled !== true) {
                      toast.error(t('mcpServers.toolMustBeEnabled'));
                      return;
                    }
                    await setToolMcpEnabled({
                      toolRouterKey: toolRouterKey,
                      mcpEnabled: !toolMcpEnabled,
                      nodeAddress: auth.node_address,
                      token: auth.api_v2_key,
                    });
                  }}
                />
              </div>
            </TooltipTrigger>
            <TooltipPortal>
              <TooltipContent align="center" side="top">
                {toolEnabled !== true
                  ? t('mcpServers.tooltipEnableFirst')
                  : `${t('mcpServers.label')} ${
                      toolMcpEnabled === true
                        ? t('common.enabled')
                        : t('common.disabled', 'Disabled')
                    }`}
              </TooltipContent>
            </TooltipPortal>
          </Tooltip>
        )}
      </div>
    </div>
  );
};
