import { DialogClose } from '@radix-ui/react-dialog';
import { DotsVerticalIcon } from '@radix-ui/react-icons';
import { useTranslation } from '@shinkai_network/shinkai-i18n';
import { type RecurringTask } from '@shinkai_network/shinkai-message-ts/api/recurring-tasks/types';
import { useExportAgent } from '@shinkai_network/shinkai-node-state/v2/mutations/exportAgent/useExportAgent';
import { useRemoveAgent } from '@shinkai_network/shinkai-node-state/v2/mutations/removeAgent/useRemoveAgent';
import { useGetAgents } from '@shinkai_network/shinkai-node-state/v2/queries/getAgents/useGetAgents';
import {
  Badge,
  Button,
  buttonVariants,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogTitle,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
  SearchInput,
  Tooltip,
  TooltipContent,
  TooltipPortal,
  TooltipProvider,
  TooltipTrigger,
  Tabs,
  TabsList,
  TabsTrigger,
  TabsContent,
  Card,
  CardTitle,
  CardDescription,
  AvatarFallback,
  Avatar,
} from '@shinkai_network/shinkai-ui';
import {
  AIAgentIcon,
  appIcon,
  CategoryIcon,
  CreateAIIcon,
  DownloadIcon,
  ScheduledTasksIcon,
} from '@shinkai_network/shinkai-ui/assets';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import { save } from '@tauri-apps/plugin-dialog';
import * as fs from '@tauri-apps/plugin-fs';
import { BaseDirectory } from '@tauri-apps/plugin-fs';
import cronstrue from 'cronstrue';
import { Edit, Plus, TrashIcon } from 'lucide-react';
import React, { useMemo, useState } from 'react';
import { Link, useNavigate } from 'react-router';
import { toast } from 'sonner';

import ImportAgentModal from '../components/agent/import-agent-modal';
import { useGetStoreAgents } from '../components/store/store-client';

import { useAuth } from '../store/auth';

function AgentsPage() {
  const auth = useAuth((state) => state.auth);
  const navigate = useNavigate();
  const { data: agents } = useGetAgents({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
  });

  const { t, Trans } = useTranslation();
  const [selectedTab, setSelectedTab] = useState<'my' | 'explore'>('my');

  const [searchQuery, setSearchQuery] = useState('');

  const filteredAgents = useMemo(() => {
    return agents?.filter((agent) =>
      agent.name.toLowerCase().includes(searchQuery.toLowerCase()),
    );
  }, [agents, searchQuery]);

  return (
    <div className="h-full">
      <Tabs
        className="flex h-full flex-col"
        value={selectedTab}
        onValueChange={(value) => setSelectedTab(value as 'my' | 'explore')}
      >
        <div className="container flex flex-col">
          <div className="flex flex-col gap-3 pt-10 pb-4">
            <div className="flex justify-between gap-4">
              <div className="font-clash inline-flex items-center gap-5 text-3xl font-medium">
                <h1>{t('agents.label')}</h1>
                <TabsList className="bg-official-gray-950/80 flex h-10 w-fit items-center gap-2 rounded-full px-1 py-1">
                  <TabsTrigger
                    className={cn(
                      'flex flex-col rounded-full px-4 py-1.5 text-base font-medium transition-colors',
                      'data-[state=active]:bg-official-gray-800 data-[state=active]:text-white',
                      'data-[state=inactive]:text-official-gray-400 data-[state=inactive]:bg-transparent',
                      'focus-visible:outline-hidden',
                    )}
                    value="my"
                  >
                    {t('agents.myAgents')}
                  </TabsTrigger>
                  <TabsTrigger
                    className={cn(
                      'flex flex-col rounded-full px-4 py-1.5 text-base font-medium transition-colors',
                      'data-[state=active]:bg-official-gray-800 data-[state=active]:text-white',
                      'data-[state=inactive]:text-official-gray-400 data-[state=inactive]:bg-transparent',
                      'focus-visible:outline-hidden',
                    )}
                    value="explore"
                  >
                    {t('agents.explore')}
                  </TabsTrigger>
                </TabsList>
              </div>
              <div className="flex gap-2">
                {selectedTab === 'my' && <ImportAgentModal />}
                <Button
                  className="min-w-[100px]"
                  onClick={() => {
                    void navigate('/add-agent');
                  }}
                  size="sm"
                >
                  <Plus className="h-4 w-4" />
                  <span>{t('agentsPage.addAgent')}</span>
                </Button>
              </div>
            </div>
            <p className="text-official-gray-400 text-sm">
              {selectedTab === 'my' ? (
                <>
                  <Trans
                    i18nKey="agentsPage.description"
                    components={{
                      br: <br />,
                    }}
                  />
                </>
              ) : (
                <>
                  <Trans
                    i18nKey="agentsPage.exploreDescription"
                    components={{
                      br: <br />,
                    }}
                  />
                </>
              )}
            </p>
          </div>

          <TabsContent value="my" className="space-y-6">
            <SearchInput
              classNames={{ input: 'bg-transparent' }}
              onChange={(e) => setSearchQuery(e.target.value)}
              value={searchQuery}
            />
            <div className="flex flex-1 flex-col space-y-3 pb-10">
              {!filteredAgents?.length ? (
                <div className="flex grow flex-col items-center gap-3 pt-20">
                  <div className="bg-official-gray-800 flex size-10 items-center justify-center rounded-lg p-2">
                    <AIAgentIcon className="size-full" name={''} />
                  </div>
                  <div className="flex flex-col items-center gap-1">
                    <p className="font-medium">
                      {t('agents.noAvailableAgents')}
                    </p>
                    <p className="text-official-gray-400 text-center text-sm font-medium">
                      {t('agents.createFirstAgent')}
                    </p>
                  </div>
                </div>
              ) : (
                <div className="flex flex-col gap-3">
                  {filteredAgents?.map((agent) => (
                    <AgentCard
                      key={agent.agent_id}
                      agentDescription={agent.ui_description}
                      agentId={agent.agent_id}
                      agentName={agent.name}
                      llmProviderId={agent.llm_provider_id}
                      scheduledTasks={agent.cron_tasks}
                    />
                  ))}
                </div>
              )}
            </div>
          </TabsContent>

          <TabsContent value="explore" className="space-y-6">
            <DownloadAgents />
          </TabsContent>
        </div>
      </Tabs>
    </div>
  );
}

export default AgentsPage;

function sanitizeFileName(name: string): string {
  let sanitized = name.replace(/[^a-zA-Z0-9_]/g, '_');
  sanitized = sanitized.replace(/_+/g, '_');
  sanitized = sanitized.replace(/^_+|_+$/g, '');

  return sanitized || 'untitled_agent';
}

const AgentCard = ({
  agentId,
  agentName,
  // llmProviderId,
  agentDescription,
  scheduledTasks,
}: {
  agentId: string;
  agentName: string;
  llmProviderId: string;
  agentDescription: string;
  scheduledTasks?: RecurringTask[];
}) => {
  const { t } = useTranslation();
  const [isDeleteAgentDrawerOpen, setIsDeleteAgentDrawerOpen] =
    React.useState(false);
  const auth = useAuth((state) => state.auth);
  const navigate = useNavigate();

  const { mutateAsync: exportAgent } = useExportAgent({
    onSuccess: async (response) => {
      const sanitizedAgentName = sanitizeFileName(agentName);
      const file = new Blob([response ?? ''], {
        type: 'application/octet-stream',
      });

      const arrayBuffer = await file.arrayBuffer();
      const content = new Uint8Array(arrayBuffer);

      const savePath = await save({
        defaultPath: `${sanitizedAgentName}.zip`,
        filters: [{ name: 'Zip File', extensions: ['zip'] }],
      });

      if (!savePath) {
        toast.info('File saving cancelled');
        return;
      }

      await fs.writeFile(savePath, content, {
        baseDir: BaseDirectory.Download,
      });

      toast.success('Agent exported successfully');
    },
    onError: (error) => {
      toast.error('Failed to export agent', {
        description: error.response?.data?.message ?? error.message,
      });
    },
  });

  const hasScheduledTasks =
    scheduledTasks?.length && scheduledTasks?.length > 0;

  return (
    <React.Fragment>
      <div className="border-official-gray-850 bg-official-gray-900 flex items-center justify-between gap-1 rounded-lg border p-3.5">
        <div className="flex items-start gap-3">
          <div className="flex size-8 items-center justify-center rounded-lg">
            <AIAgentIcon name={agentName} size="sm" />
          </div>
          <div className="flex flex-col gap-1.5">
            <span className="inline-flex w-full items-center gap-3 truncate text-start text-sm capitalize">
              {agentName}{' '}
              {scheduledTasks?.length && scheduledTasks?.length > 0 && (
                <Badge
                  className="border bg-emerald-900/40 px-1 py-0 text-xs font-medium text-emerald-400"
                  variant="secondary"
                >
                  Scheduled
                </Badge>
              )}
            </span>

            <span className="text-official-gray-400 text-sm">
              {agentDescription ?? 'No description'}
            </span>
            {hasScheduledTasks && (
              <div className="mt-2 inline-flex gap-2">
                {scheduledTasks.map((task) => (
                  <TooltipProvider delayDuration={0} key={task.task_id}>
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <Link
                          className="text-official-gray-200 bg-official-gray-850 flex items-center gap-2 rounded-md px-2 py-1 text-xs transition-colors hover:text-white"
                          key={task.task_id}
                          to={`/tasks/${task.task_id}`}
                        >
                          <ScheduledTasksIcon className="h-3 w-3" />
                          <span>
                            {cronstrue.toString(task.cron, {
                              throwExceptionOnParseError: false,
                            })}
                          </span>
                        </Link>
                      </TooltipTrigger>
                      <TooltipContent
                        align="center"
                        className="flex flex-col items-center gap-1"
                      >
                        {t('agentsPage.goToTaskDetails')}
                      </TooltipContent>
                    </Tooltip>
                  </TooltipProvider>
                ))}
              </div>
            )}
          </div>
        </div>
        <div className="flex items-center gap-4">
          <TooltipProvider delayDuration={0}>
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  onClick={() => {
                    void navigate(`/home`, { state: { agentName: agentId } });
                  }}
                  size="sm"
                  variant="outline"
                >
                  <CreateAIIcon className="size-4" />
                  <span className=""> Chat</span>
                </Button>
              </TooltipTrigger>

              <TooltipPortal>
                <TooltipContent
                  align="center"
                  className="flex flex-col items-center gap-1"
                  side="right"
                >
                  Create New Chat
                </TooltipContent>
              </TooltipPortal>
            </Tooltip>
          </TooltipProvider>
          <DropdownMenu modal={false}>
            <DropdownMenuTrigger asChild>
              <div
                className={cn(
                  buttonVariants({
                    variant: 'tertiary',
                    size: 'icon',
                  }),
                  'border-0 hover:bg-gray-500/40',
                )}
                onClick={(event) => {
                  event.stopPropagation();
                }}
                role="button"
                tabIndex={0}
              >
                <span className="sr-only">{t('common.moreOptions')}</span>
                <DotsVerticalIcon className="text-gray-100" />
              </div>
            </DropdownMenuTrigger>
            <DropdownMenuContent
              align="end"
              className="w-[160px] border bg-gray-500 px-2.5 py-2"
            >
              {[
                {
                  name: t('common.edit'),
                  icon: <Edit className="mr-3 h-4 w-4" />,
                  onClick: () => {
                    void navigate(`/agents/edit/${agentId}`);
                  },
                },
                {
                  name: 'Export',
                  icon: <DownloadIcon className="mr-3 h-4 w-4" />,
                  onClick: () => {
                    void exportAgent({
                      agentId,
                      nodeAddress: auth?.node_address ?? '',
                      token: auth?.api_v2_key ?? '',
                    });
                  },
                },
                {
                  name: t('common.delete'),
                  icon: <TrashIcon className="mr-3 h-4 w-4" />,
                  onClick: () => {
                    setIsDeleteAgentDrawerOpen(true);
                  },
                },
              ].map((option) => (
                <React.Fragment key={option.name}>
                  {option.name === 'Delete' && (
                    <DropdownMenuSeparator className="bg-gray-300" />
                  )}
                  <DropdownMenuItem
                    key={option.name}
                    onClick={(event) => {
                      event.stopPropagation();
                      option.onClick();
                    }}
                  >
                    {option.icon}
                    {option.name}
                  </DropdownMenuItem>
                </React.Fragment>
              ))}
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </div>
      <RemoveAgentDrawer
        agentId={agentId}
        onOpenChange={setIsDeleteAgentDrawerOpen}
        open={isDeleteAgentDrawerOpen}
      />
    </React.Fragment>
  );
};

const RemoveAgentDrawer = ({
  open,
  onOpenChange,
  agentId,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  agentId: string;
}) => {
  const { t } = useTranslation();
  const auth = useAuth((state) => state.auth);
  const { mutateAsync: removeAgent, isPending } = useRemoveAgent({
    onSuccess: () => {
      onOpenChange(false);
      toast.success('Delete agent successfully');
    },
    onError: (error) => {
      toast.error('Failed delete agent', {
        description: error.response?.data?.message ?? error.message,
      });
    },
  });

  return (
    <Dialog onOpenChange={onOpenChange} open={open}>
      <DialogContent className="sm:max-w-[425px]">
        <DialogTitle className="pb-0">
          Delete Agent{' '}
          <span className="font-mono text-base break-all">{agentId}</span> ?
        </DialogTitle>
        <DialogDescription>
          The agent will be permanently deleted. This action cannot be undone.
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
              disabled={isPending}
              isLoading={isPending}
              onClick={async () => {
                await removeAgent({
                  nodeAddress: auth?.node_address ?? '',
                  agentId,
                  token: auth?.api_v2_key ?? '',
                });
              }}
              size="sm"
              variant="destructive"
            >
              {t('common.delete')}
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
};
const SHINKAI_DAPP_URL = 'https://shinkai-contracts.pages.dev';

export function AuthorAvatarLink({ author }: { author: string }) {
  const formattedAuthor = author
    .replace('@@', '')
    .replace('official.shinkai', 'official.sep-shinkai');

  return (
    <a
      href={`${SHINKAI_DAPP_URL}/identity/${formattedAuthor}`}
      className="text-gray-80 isolate flex items-center gap-2 text-sm hover:[&>span]:underline"
      target="_blank"
      rel="noopener noreferrer"
    >
      <Avatar className={cn('h-5 w-5')}>
        {author === '@@official.shinkai' ? (
          <img alt="Shinkai" src={appIcon} />
        ) : (
          <AvatarFallback className="bg-official-gray-800 text-official-gray-300">
            {formattedAuthor.charAt(0)}
          </AvatarFallback>
        )}
      </Avatar>
      <span className="truncate">{author}</span>
    </a>
  );
}

const DownloadAgents = () => {
  const { data: storeAgents, isPending: isStoreAgentsPending } =
    useGetStoreAgents();

  const [searchQuery, setSearchQuery] = useState('');

  const filtered = useMemo(() => {
    return (
      storeAgents?.filter(
        (a) =>
          a.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
          a.description.toLowerCase().includes(searchQuery.toLowerCase()),
      ) ?? []
    );
  }, [storeAgents, searchQuery]);

  return (
    <div className="space-y-6">
      <SearchInput
        placeholder="Search for agents"
        classNames={{ input: 'bg-transparent' }}
        value={searchQuery}
        onChange={(e) => setSearchQuery(e.target.value)}
      />
      <div className="grid grid-cols-1 gap-3 pb-10">
        {isStoreAgentsPending &&
          Array.from({ length: 4 }).map((_, idx) => (
            <Card
              key={idx}
              className="border-official-gray-850 bg-official-gray-900 h-24 animate-pulse"
            />
          ))}
        {filtered.map((agent) => (
          <Card
            key={agent.id}
            className="border-official-gray-850 bg-official-gray-900 flex flex-col rounded-xl border p-4"
          >
            <div className="flex items-center gap-2">
              <div className="bg-official-gray-900 flex h-12 min-h-12 w-12 min-w-12 items-center justify-center rounded-lg">
                {agent.iconUrl ? (
                  <img
                    src={agent.iconUrl}
                    alt={agent.name}
                    className="h-8 w-8 rounded object-cover"
                  />
                ) : (
                  <AIAgentIcon className="h-8 w-8" name={agent.name} />
                )}
              </div>
              <div className="flex min-w-0 flex-1 flex-col">
                <CardTitle className="truncate text-base leading-tight font-semibold text-white">
                  <Link
                    className="truncate text-base leading-tight font-semibold text-white hover:underline"
                    to={`https://store.shinkai.com/product/${agent.routerKey}`}
                    target="_blank"
                  >
                    {agent.name}
                  </Link>
                </CardTitle>
                <CardDescription className="text-official-gray-400 line-clamp-1 text-sm">
                  {agent.description}
                </CardDescription>
              </div>
            </div>
            <div className="mt-4 flex flex-wrap items-center gap-4">
              <div className="text-official-gray-400 flex items-center gap-6 pl-2 text-sm font-medium">
                <span className="flex items-center gap-2">
                  <CategoryIcon className="text-official-gray-400 size-4" />
                  {agent.category?.name ?? ''}
                </span>
                <span className="flex items-center gap-1">
                  <DownloadIcon className="text-official-gray-400 size-4" />
                  {agent.downloads ?? 0} Installs
                </span>
                <AuthorAvatarLink author={agent.author} />
              </div>
              <div className="ml-auto">
                <Link
                  className={cn(
                    buttonVariants({
                      variant: 'outline',
                      size: 'sm',
                    }),
                  )}
                  to={`https://store.shinkai.com/product/${agent.routerKey}`}
                  target="_blank"
                >
                  Get Agent
                </Link>
              </div>
            </div>
          </Card>
        ))}
      </div>
    </div>
  );
};
