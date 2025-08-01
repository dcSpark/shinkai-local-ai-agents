import { DialogClose } from '@radix-ui/react-dialog';
import { useTranslation } from '@shinkai_network/shinkai-i18n';
import { type McpServer } from '@shinkai_network/shinkai-message-ts/api/mcp-servers/types';
import { useDeleteMcpServer } from '@shinkai_network/shinkai-node-state/v2/mutations/deleteMcpServer/useDeleteMcpServer';
import { useGetMCPServerTools } from '@shinkai_network/shinkai-node-state/v2/queries/getMCPServerTools/useGetMCPServerTool';
import {
  Badge,
  Button,
  buttonVariants,
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
  MarkdownText,
  Switch,
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@shinkai_network/shinkai-ui';
import { ToolsIcon } from '@shinkai_network/shinkai-ui/assets';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import { BoltIcon, ChevronDown, MoreVertical, Trash2 } from 'lucide-react';
import { useState } from 'react';
import { Link } from 'react-router';
import { toast } from 'sonner';

import { useAuth } from '../../store/auth';
import { AddMcpServerModal } from './add-mcp-server-modal';

const getToolDisplayName = (name: string) => {
  const index = name.indexOf(' - ');
  return index !== -1 ? name.slice(index + 3) : name;
};

interface McpServerCardProps {
  server: McpServer;
  onToggleEnabled: (serverId: number, currentEnabled: boolean) => Promise<void>;
}

export const McpServerCard = ({
  server,
  onToggleEnabled,
}: McpServerCardProps) => {
  const { t } = useTranslation();
  const auth = useAuth((state) => state.auth);
  const [isDeleteDialogOpen, setIsDeleteDialogOpen] = useState(false);
  const [isToolsDialogOpen, setIsToolsDialogOpen] = useState(false);
  const [isConfigureModalOpen, setIsConfigureModalOpen] = useState(false);
  const { data: mcpServerTools } = useGetMCPServerTools({
    nodeAddress: auth?.node_address || '',
    token: auth?.api_v2_key || '',
    mcpServerId: server.id,
  });

  const { mutateAsync: deleteMcpServer, isPending: isDeleting } =
    useDeleteMcpServer({
      onSuccess: () => {
        toast.success(t('mcpServers.deleteSuccess'));
        setIsDeleteDialogOpen(false);
      },
      onError: (error: Error) => {
        toast.error(t('mcpServers.deleteFailed'), {
          description: error?.message,
        });
      },
    });

  const handleDelete = async () => {
    if (!auth) return;

    await deleteMcpServer({
      nodeAddress: auth.node_address,
      token: auth.api_v2_key,
      id: server.id,
    });
  };

  return (
    <>
      <div
        className={cn(
          'grid grid-cols-[1fr_120px_40px_auto] items-center gap-5 rounded-xs px-2 py-4 pr-4 text-left text-sm',
        )}
      >
        <div className="flex flex-col gap-2.5">
          <div className="flex items-center gap-2">
            <span className="text-text-default text-sm font-medium">
              {server.name}{' '}
            </span>
            <Badge variant="outline">{server.type}</Badge>
          </div>
          <div className="text-text-secondary text-xs">
            {server.type === 'Command' ? server.command : server.url}
          </div>
        </div>
        {mcpServerTools && mcpServerTools.length > 0 ? (
          <div className="flex items-center justify-center">
            <Dialog
              onOpenChange={setIsToolsDialogOpen}
              open={isToolsDialogOpen}
            >
              <Tooltip>
                <TooltipTrigger asChild>
                  <DialogTrigger asChild>
                    <Badge
                      className={cn(
                        buttonVariants({
                          variant: 'outline',
                          size: 'xs',
                        }),
                        'cursor-pointer',
                      )}
                      onClick={() => setIsToolsDialogOpen(true)}
                    >
                      <ToolsIcon className="mr-2 size-4 text-white" />
                      {mcpServerTools && mcpServerTools.length > 99
                        ? '99+'
                        : mcpServerTools?.length || '0'}{' '}
                      {t('mcpServers.tools')}
                    </Badge>
                  </DialogTrigger>
                </TooltipTrigger>
                <TooltipContent align="center" side="top">
                  {t('mcpServers.viewAvailableTools')}
                </TooltipContent>
              </Tooltip>
              <DialogContent className="sm:max-w-3xl" showCloseButton>
                <DialogHeader>
                  <DialogTitle>
                    {t('mcpServers.toolsFor', { name: server.name })}
                  </DialogTitle>
                  <DialogDescription>
                    {t('mcpServers.listOfToolsAvailableFromThisMcpServer')}
                  </DialogDescription>
                </DialogHeader>
                <div className="max-h-[60vh] overflow-y-auto py-1">
                  {mcpServerTools && mcpServerTools.length > 0 ? (
                    <ul className="space-y-3">
                      {mcpServerTools.map((tool) => (
                        <li
                          className="border-divider rounded-lg border p-3 py-2.5 text-sm"
                          key={tool.id}
                          style={{
                            fontSize: '14px',
                          }}
                        >
                          <div className="flex flex-col gap-2">
                            <div className="flex items-center gap-2 text-base font-medium">
                              <ToolsIcon className="h-4 w-4" />

                              {tool.tool_router_key ? (
                                <Link
                                  className="text-white hover:underline"
                                  onClick={() => setIsToolsDialogOpen(false)}
                                  to={`/tools/${tool.tool_router_key}`}
                                >
                                  {getToolDisplayName(tool.name)}
                                </Link>
                              ) : (
                                <span className="text-white">
                                  {getToolDisplayName(tool.name)}
                                </span>
                              )}
                            </div>
                            {tool.description && (
                              <MarkdownText
                                className="text-text-secondary"
                                content={tool.description}
                              />
                            )}
                          </div>
                          {Object.keys((tool.input_args || {}).properties || {})
                            .length > 0 && (
                            <Collapsible>
                              <CollapsibleTrigger className="text-text-secondary bg-official-gray-900 mt-2 flex w-full cursor-pointer items-center justify-between rounded-lg p-2 text-left hover:text-white [&[data-state=open]]:rounded-b-none [&[data-state=open]>svg]:rotate-180">
                                <span className="text-sm">
                                  View Input Parameters
                                </span>
                                <ChevronDown className="ml-auto size-4" />
                              </CollapsibleTrigger>
                              <CollapsibleContent className="bg-official-gray-900 rounded-b-lg p-1 pt-0">
                                <ul className="grid list-disc gap-2 rounded-lg px-1 py-1 pl-6">
                                  {Object.keys(
                                    (tool.input_args || {}).properties || {},
                                  ).map((key) => (
                                    <li className="" key={key}>
                                      <span className="mr-2 text-sm text-white">
                                        {key}
                                      </span>
                                      <span className="text-text-secondary text-sm">
                                        {
                                          tool.input_args?.properties?.[key]
                                            ?.description
                                        }
                                      </span>
                                    </li>
                                  ))}
                                </ul>
                              </CollapsibleContent>
                            </Collapsible>
                          )}
                        </li>
                      ))}
                    </ul>
                  ) : (
                    <p className="text-sm text-gray-400">
                      {t('mcpServers.noToolsAvailableForThisServer')}
                    </p>
                  )}
                </div>
                <DialogFooter>
                  <DialogClose asChild>
                    <Button size="md" type="button" variant="outline">
                      Close
                    </Button>
                  </DialogClose>
                </DialogFooter>
              </DialogContent>
            </Dialog>
          </div>
        ) : (
          <div />
        )}

        <Tooltip>
          <TooltipTrigger>
            <Switch
              checked={server.is_enabled}
              onCheckedChange={() =>
                onToggleEnabled(server.id, server.is_enabled)
              }
            />
          </TooltipTrigger>
          <TooltipContent>
            {server.is_enabled ? 'MCP Server Enabled' : 'MCP Server Disabled'}
          </TooltipContent>
        </Tooltip>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              className="size-8 p-2"
              rounded="lg"
              size="auto"
              variant="outline"
            >
              <MoreVertical className="size-full" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent
            align="end"
            className="bg-official-gray-950 border-divider border p-2.5"
          >
            <DropdownMenuItem
              className="flex items-center gap-2.5 text-xs"
              onClick={() => setIsConfigureModalOpen(true)}
            >
              <BoltIcon className="h-4 w-4" />
              Configure
            </DropdownMenuItem>
            <DropdownMenuItem
              className="flex items-center gap-2 text-xs"
              onClick={() => setIsDeleteDialogOpen(true)}
            >
              <Trash2 className="h-4 w-4" />
              Delete
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
      <Dialog onOpenChange={setIsDeleteDialogOpen} open={isDeleteDialogOpen}>
        <DialogContent className="sm:max-w-[425px]">
          <DialogTitle className="pb-0">{t('mcpServers.delete')}</DialogTitle>
          <DialogDescription>
            {t('mcpServers.deleteDescription', { name: server.name })}
          </DialogDescription>
          <DialogFooter>
            <div className="flex gap-2 pt-4">
              <DialogClose asChild className="flex-1">
                <Button
                  className="min-w-[100px] flex-1"
                  size="sm"
                  type="button"
                  variant="ghost"
                >
                  {t('common.cancel')}
                </Button>
              </DialogClose>
              <Button
                className="min-w-[100px] flex-1"
                isLoading={isDeleting}
                onClick={handleDelete}
                size="sm"
                variant="destructive"
              >
                {t('common.delete')}
              </Button>
            </div>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      <AddMcpServerModal
        initialData={server}
        isOpen={isConfigureModalOpen}
        mode="Update"
        onClose={() => setIsConfigureModalOpen(false)}
        onSuccess={() => {
          setIsConfigureModalOpen(false);
        }}
      />
    </>
  );
};
