import { useTranslation } from '@shinkai_network/shinkai-i18n';
import { useAddNetworkTool } from '@shinkai_network/shinkai-node-state/v2/mutations/addNetworkTool/useAddNetworkTool';
import { useGetAgents } from '@shinkai_network/shinkai-node-state/v2/queries/getAgents/useGetAgents';
import { useGetInstalledNetworkTools } from '@shinkai_network/shinkai-node-state/v2/queries/getInstalledNetworkTools/useGetInstalledNetworkTools';
import { type FormattedNetworkAgent } from '@shinkai_network/shinkai-node-state/v2/queries/getNetworkAgents/types';
import { useGetNetworkAgents } from '@shinkai_network/shinkai-node-state/v2/queries/getNetworkAgents/useGetNetworkAgents';
import { useGetToolsWithOfferings } from '@shinkai_network/shinkai-node-state/v2/queries/getToolsWithOfferings/useGetToolsWithOfferings';
import { useGetWalletBalance } from '@shinkai_network/shinkai-node-state/v2/queries/getWalletBalance/useGetWalletBalance';
import { useGetWalletList } from '@shinkai_network/shinkai-node-state/v2/queries/getWalletList/useGetWalletList';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  Badge,
  Button,
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
  SearchInput,
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogTrigger,
  DialogFooter,
  Alert,
  AlertTitle,
  AlertDescription,
  buttonVariants,
  Popover,
  PopoverContent,
  PopoverTrigger,
  CardFooter,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@shinkai_network/shinkai-ui';
import {
  CryptoWalletIcon,
  EthereumIcon,
  USDCIcon,
} from '@shinkai_network/shinkai-ui/assets';
import { cn } from '@shinkai_network/shinkai-ui/utils';

import {
  User,
  CreditCard,
  Info,
  AlertCircle,
  CheckCircle2,
  ChevronDownIcon,
  PlusIcon,
  Sparkles,
  Plus,
  CheckCircle,
  Circle,
  X,
} from 'lucide-react';
import { useMemo, useState } from 'react';
import { Link, useNavigate } from 'react-router';
import { toast } from 'sonner';
import AddAgentFromIdModal from '../components/agent/add-agent-from-id-modal';
import ConfigureAgentDialog from '../components/agent/configure-agent-dialog';
import PublishAgentDialog from '../components/agent/publish-agent-dialog';
import {
  formatBalanceAmount,
  truncateAddress,
} from '../components/crypto-wallet/utils';
import RemoveNetworkAgentButton from '../components/network/remove-network-agent-button';
import RemoveToolOfferingButton from '../components/network/remove-tool-offering-button';
import { useAuth } from '../store/auth';
import { useSettings } from '../store/settings';

export const MCP_SERVER_ID = 'shinkai-mcp-server';

export const NetworkAgentPage = () => {
  const [selectedTab, setSelectedTab] = useState<'network' | 'published'>(
    'network',
  );

  const { t } = useTranslation();

  const auth = useAuth((state) => state.auth);
  const optInExperimental = useSettings((state) => state.optInExperimental);
  const { data: walletInfo } = useGetWalletList({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
  });

  const isWalletConnected =
    walletInfo?.payment_wallet || walletInfo?.receiving_wallet;

  const { data: walletBalance } = useGetWalletBalance(
    {
      nodeAddress: auth?.node_address ?? '',
      token: auth?.api_v2_key ?? '',
    },
    { enabled: !!isWalletConnected },
  );

  return (
    <Tabs
      className="flex size-full flex-col"
      defaultValue="network"
      onValueChange={(value) => {
        setSelectedTab(value as 'network' | 'published');
      }}
    >
      <div className="container max-w-screen-lg">
        <div
          className={cn(
            'flex flex-col gap-2 pt-10 pb-6',
            optInExperimental && 'gap-5',
          )}
        >
          <div className="flex items-center justify-between gap-4">
            <div
              className={cn(
                'font-clash inline-flex items-center gap-2 text-3xl font-medium',
              )}
            >
              <h1>
                {optInExperimental
                  ? t('networkAgentsPage.titleNetwork')
                  : t('networkAgentsPage.titleDecentralized')}
              </h1>
              {optInExperimental && (
                <TabsList className="bg-official-gray-950/80 flex h-10 w-fit items-center gap-2 rounded-full px-1 py-1">
                  <TabsTrigger
                    className={cn(
                      'flex flex-col rounded-full px-4 py-1.5 text-base font-medium transition-colors',
                      'data-[state=active]:bg-official-gray-800 data-[state=active]:text-white',
                      'data-[state=inactive]:text-official-gray-400 data-[state=inactive]:bg-transparent',
                      'focus-visible:outline-hidden',
                    )}
                    value="network"
                  >
                    {t('networkAgentsPage.agentsTab')}
                  </TabsTrigger>
                  {optInExperimental && (
                    <TabsTrigger
                      className={cn(
                        'flex flex-col rounded-full px-4 py-1.5 text-base font-medium transition-colors',
                        'data-[state=active]:bg-official-gray-800 data-[state=active]:text-white',
                        'data-[state=inactive]:text-official-gray-400 data-[state=inactive]:bg-transparent',
                        'focus-visible:outline-hidden',
                      )}
                      value="published"
                    >
                      {t('networkAgentsPage.publishedTab')}
                    </TabsTrigger>
                  )}
                </TabsList>
              )}
            </div>

            <div className="flex items-center gap-2">
              <AddAgentFromIdModal />

              {!isWalletConnected && (
                <Link
                  to="/settings/crypto-wallet"
                  className={cn(
                    buttonVariants({ variant: 'outline', size: 'sm' }),
                  )}
                >
                  <CryptoWalletIcon className="size-4" />
                  {t('networkAgentsPage.connectWallet')}
                </Link>
              )}

              {isWalletConnected && (
                <Popover>
                  <PopoverTrigger asChild>
                    <Button variant="outline" size="sm">
                      <CryptoWalletIcon className="size-4" />
                      {truncateAddress(
                        walletInfo?.payment_wallet?.data?.address?.address_id ??
                          '',
                      )}
                      <ChevronDownIcon className="size-4" />
                    </Button>
                  </PopoverTrigger>
                  <PopoverContent className="w-80" align="end">
                    <div className="space-y-4">
                      <div className="flex items-center justify-between">
                        <div className="text-sm font-medium">
                          {t('networkAgentsPage.walletBalance')}
                        </div>
                        <div className="text-official-gray-400 text-xs">
                          {walletInfo?.payment_wallet?.data?.network}
                        </div>
                      </div>
                      <div className="space-y-2">
                        <div className="flex items-center justify-between">
                          <div className="flex items-center gap-2">
                            <div className="flex size-6 items-center justify-center rounded-full border bg-black">
                              <EthereumIcon />
                            </div>
                            <span className="text-sm">ETH</span>
                          </div>
                          <div className="text-sm font-medium">
                            {formatBalanceAmount(
                              walletBalance?.ETH.amount ?? '0',
                              walletBalance?.ETH.decimals ?? 0,
                            )}{' '}
                            ETH
                          </div>
                        </div>
                        <div className="flex items-center justify-between">
                          <div className="flex items-center gap-2">
                            <div className="flex size-6 items-center justify-center rounded-full border bg-black">
                              <USDCIcon />
                            </div>
                            <span className="text-sm">USDC</span>
                          </div>
                          <div className="text-sm font-medium">
                            {formatBalanceAmount(
                              walletBalance?.USDC.amount ?? '0',
                              walletBalance?.USDC.decimals ?? 0,
                            )}{' '}
                            USDC
                          </div>
                        </div>
                      </div>
                      <Link
                        to="/settings/crypto-wallet"
                        className={cn(
                          buttonVariants({ variant: 'outline', size: 'sm' }),
                          'w-full',
                        )}
                      >
                        {t('networkAgentsPage.manageWallet')}
                      </Link>
                    </div>
                  </PopoverContent>
                </Popover>
              )}
            </div>
          </div>
          <p className="text-official-gray-400 text-sm whitespace-pre-wrap">
            {selectedTab === 'network'
              ? t('networkAgentsPage.descriptionNetwork')
              : t('networkAgentsPage.descriptionPublished')}
          </p>
        </div>

        {!isWalletConnected && (
          <SetupGuide isWalletConnected={!!isWalletConnected} />
        )}

        <Disclaimer />

        <TabsContent value="network">
          <DiscoverNetworkAgents isWalletConnected={!!isWalletConnected} />
        </TabsContent>
        {optInExperimental && (
          <TabsContent value="published" className="space-y-4">
            <div className="flex justify-end">
              <PublishAgentDialog />
            </div>
            <PublishedAgents />
          </TabsContent>
        )}
      </div>
    </Tabs>
  );
};

const DiscoverNetworkAgents = ({
  isWalletConnected,
}: {
  isWalletConnected: boolean;
}) => {
  const [searchQuery, setSearchQuery] = useState('');
  const [pricingFilter, setPricingFilter] = useState<'all' | 'free' | 'paid'>(
    'all',
  );
  const [providerFilter, setProviderFilter] = useState<
    'all' | 'shinkai' | 'community'
  >('all');
  const { t } = useTranslation();
  const auth = useAuth((state) => state.auth);

  const {
    data: networkAgents,
    isPending: isNetworkAgentsPending,
    isSuccess: isNetworkAgentsSuccess,
  } = useGetNetworkAgents();

  const { data: agents } = useGetAgents({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
  });

  const {
    data: installedNetworkTools,
    isPending: isInstalledNetworkToolsPending,
    isSuccess: isInstalledNetworkToolsSuccess,
  } = useGetInstalledNetworkTools({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
  });

  const toolAgentMap = useMemo(() => {
    const map = new Map<string, string>();
    agents?.forEach((agent) => {
      agent.tools.forEach((tool) => {
        if (!map.has(tool)) {
          map.set(tool, agent.agent_id);
        }
      });
    });
    return map;
  }, [agents]);

  const filteredAgents = (networkAgents ?? [])?.filter((agent) => {
    const matchesSearch =
      agent.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
      agent.description.toLowerCase().includes(searchQuery.toLowerCase());

    const isFreePricing =
      agent.price.toLowerCase().includes('free') || agent.price === '$0.00';
    const matchesPricing =
      pricingFilter === 'all' ||
      (pricingFilter === 'free' && isFreePricing) ||
      (pricingFilter === 'paid' && !isFreePricing);

    const isShinkaiProvider = agent.provider
      ?.toLowerCase()
      .includes('@@official.sep-shinkai');

    const matchesProvider =
      providerFilter === 'all' ||
      (providerFilter === 'shinkai' && isShinkaiProvider) ||
      (providerFilter === 'community' && !isShinkaiProvider);

    return matchesSearch && matchesPricing && matchesProvider;
  });

  return (
    <div className="space-y-8">
      <div className="flex items-center gap-2">
        <SearchInput
          placeholder={t('networkAgentsPage.searchPlaceholder')}
          classNames={{ input: 'bg-transparent' }}
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
        />

        <div className="flex items-center gap-2">
          <Select
            value={pricingFilter}
            onValueChange={(value: 'all' | 'free' | 'paid') =>
              setPricingFilter(value)
            }
          >
            <SelectTrigger className="!h-[auto] w-32 rounded-full bg-transparent py-2 [&>svg]:top-[12px]">
              <SelectValue placeholder="Pricing" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">Free + Paid</SelectItem>
              <SelectItem value="free">Free</SelectItem>
              <SelectItem value="paid">Paid</SelectItem>
            </SelectContent>
          </Select>

          <Select
            value={providerFilter}
            onValueChange={(value: 'all' | 'shinkai' | 'community') =>
              setProviderFilter(value)
            }
          >
            <SelectTrigger className="!h-[auto] w-36 rounded-full bg-transparent py-2 [&>svg]:top-[12px]">
              <SelectValue placeholder="Provider" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">All Providers</SelectItem>
              <SelectItem value="shinkai">Shinkai</SelectItem>
              <SelectItem value="community">Community</SelectItem>
            </SelectContent>
          </Select>
        </div>
      </div>
      <div className="grid grid-cols-1 gap-6 pb-10 md:grid-cols-2">
        {(isNetworkAgentsPending || isInstalledNetworkToolsPending) &&
          Array.from({ length: 4 }).map((_, index) => (
            <Card
              key={index}
              className="border-official-gray-850 bg-official-gray-900 flex flex-col border"
            >
              <CardHeader className="pb-4">
                <div className="bg-official-gray-800 mb-1 h-6 w-3/4 animate-pulse rounded" />
                <div className="bg-official-gray-800 mb-3 h-4 w-1/2 animate-pulse rounded" />
                <div className="space-y-2">
                  <div className="bg-official-gray-800 h-4 w-full animate-pulse rounded" />
                  <div className="bg-official-gray-800 h-4 w-2/3 animate-pulse rounded" />
                </div>
              </CardHeader>
              <CardContent className="flex-1">
                <div className="flex items-center justify-between">
                  <div className="bg-official-gray-800 h-4 w-1/4 animate-pulse rounded" />
                  <div className="bg-official-gray-800 h-4 w-1/4 animate-pulse rounded" />
                </div>
              </CardContent>
              <CardFooter className="flex gap-3">
                <div className="bg-official-gray-800 h-9 w-full animate-pulse rounded-full" />
                <div className="bg-official-gray-800 h-9 w-full animate-pulse rounded-full" />
              </CardFooter>
            </Card>
          ))}

        {isNetworkAgentsSuccess &&
          isInstalledNetworkToolsSuccess &&
          filteredAgents.map((agent) => (
            <AgentCard
              key={agent.id}
              agent={agent}
              type="discover"
              isInstalled={
                (Array.isArray(installedNetworkTools) &&
                  installedNetworkTools?.some(
                    (tool) => tool.tool_router_key === agent.toolRouterKey,
                  )) ??
                false
              }
              associatedAgentId={toolAgentMap.get(agent.toolRouterKey)}
              isWalletConnected={isWalletConnected}
              nodeStatus={agent.node?.online ? 'online' : 'offline'}
            />
          ))}
      </div>
    </div>
  );
};

const PublishedAgents = () => {
  const { t } = useTranslation();
  const auth = useAuth((state) => state.auth);
  const { data } = useGetToolsWithOfferings({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
  });

  const publishedAgents = useMemo(() => {
    if (!data) return [];
    return data.map((item) => {
      const payment =
        item.tool_offering.usage_type?.PerUse &&
        typeof item.tool_offering.usage_type?.PerUse === 'object' &&
        'Payment' in item.tool_offering.usage_type?.PerUse
          ? item.tool_offering.usage_type?.PerUse?.Payment?.[0]
          : undefined;
      const price = payment
        ? `${payment.maxAmountRequired} ${payment.extra?.name ?? ''}`
        : 'Free';
      return {
        id: item.tool_offering.tool_key,
        name: item.network_tool.name,
        description: item.network_tool.description,
        price,
        category: item.network_tool.toolkit_name ?? '',
        provider: item.network_tool.provider,
        toolRouterKey: item.network_tool.tool_router_key,
        apiData: item,
        node: item.node,
      };
    });
  }, [data]);

  if (publishedAgents.length === 0) {
    return (
      <div className="flex flex-col items-center gap-3 pt-20">
        <p className="text-center text-sm font-medium">
          {t('networkAgentsPage.noPublishedAgents')}
        </p>
        <PublishAgentDialog />
      </div>
    );
  }

  return (
    <div className="grid grid-cols-1 gap-6 md:grid-cols-2">
      {publishedAgents.map((agent) => (
        <AgentCard
          key={agent.id}
          agent={agent}
          type="exposed"
          isWalletConnected={true}
        />
      ))}
    </div>
  );
};

interface AgentCardProps {
  agent: FormattedNetworkAgent;
  type: 'discover' | 'exposed';
  isInstalled?: boolean;
  isWalletConnected: boolean;
  associatedAgentId?: string;
  nodeStatus?: 'online' | 'offline';
}

const AgentCard = ({
  agent,
  type,
  isInstalled,
  isWalletConnected,
  associatedAgentId,
  nodeStatus,
}: AgentCardProps) => {
  const [showInstallModal, setShowInstallModal] = useState(false);
  const [showDetailsModal, setShowDetailsModal] = useState(false);

  const { t } = useTranslation();
  const isFreePricing =
    agent.price.toLowerCase().includes('free') || agent.price === '$0.00';

  const amount =
    agent.apiData.tool_offering.usage_type?.PerUse &&
    typeof agent.apiData.tool_offering.usage_type?.PerUse === 'object' &&
    'Payment' in agent.apiData.tool_offering.usage_type?.PerUse
      ? agent.apiData.tool_offering.usage_type?.PerUse?.Payment?.[0]
          ?.maxAmountRequired
      : undefined;

  const ticker =
    agent.apiData.tool_offering.usage_type?.PerUse &&
    typeof agent.apiData.tool_offering.usage_type?.PerUse === 'object' &&
    'Payment' in agent.apiData.tool_offering.usage_type?.PerUse
      ? agent.apiData.tool_offering.usage_type?.PerUse?.Payment?.[0]?.extra
          ?.name
      : undefined;

  const allowInstall =
    (!isInstalled && isFreePricing) ||
    (!isInstalled && !isFreePricing && isWalletConnected);

  return (
    <Card className="border-official-gray-850 bg-official-gray-900 flex flex-col border">
      <CardHeader className="pb-4">
        <CardTitle className="mb-1 inline-flex items-center justify-between gap-2 text-lg leading-tight font-bold text-white">
          <div className="inline-flex items-center gap-2">
            {agent.name}
            {isInstalled && (
              <Badge variant="tags" className="gap-1">
                <CheckCircle2 className="h-3 w-3" />
                {t('common.added')}
              </Badge>
            )}
          </div>
          <div className="flex items-center gap-1">
            <Circle
              className={cn(
                'h-3 w-3',
                nodeStatus === 'online' && 'fill-green-500 text-green-500',
                nodeStatus !== 'online' && 'fill-red-500 text-red-500',
              )}
            />
            <span
              className={cn(
                'text-xs',
                nodeStatus === 'online' && 'text-green-600',
                nodeStatus !== 'online' && 'text-red-600',
              )}
            >
              {nodeStatus === 'online' ? 'Online' : 'Offline'}
            </span>
          </div>
        </CardTitle>

        {agent.provider && type === 'discover' && (
          <div className="text-official-gray-400 mb-3 flex items-center gap-2 text-sm">
            <User className="h-3 w-3" />
            <span>{agent.provider}</span>
          </div>
        )}
        <CardDescription className="text-official-gray-400 line-clamp-2 h-[44px] text-sm leading-relaxed">
          {agent.description}
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-1 flex-col space-y-4 pt-3">
        <div className="flex w-full items-center justify-between gap-2">
          {type === 'discover' && (
            <div className="inline-flex items-center gap-2 text-right">
              <div className="text-lg font-semibold">
                {isFreePricing ? 'Free' : formatBalanceAmount(amount ?? '0', 6)}
                {!isFreePricing && (
                  <span className="text-official-gray-200 text-sm font-medium">
                    {' '}
                    {ticker}
                  </span>
                )}
              </div>
              {!isFreePricing && (
                <div className="text-official-gray-400 flex items-center gap-1 text-sm">
                  <span>per use</span>
                </div>
              )}
            </div>
          )}

          {type === 'discover' && (
            <div className="flex items-center gap-2">
              {isInstalled && associatedAgentId && (
                <Link
                  to={`/home`}
                  state={{ agentName: associatedAgentId }}
                  className={cn(
                    buttonVariants({ variant: 'outline', size: 'sm' }),
                  )}
                >
                  {t('common.chat')}
                </Link>
              )}
              <Dialog
                open={showDetailsModal}
                onOpenChange={setShowDetailsModal}
              >
                <DialogTrigger asChild>
                  <Button variant="outline" size="sm" className="px-4">
                    {t('common.details')}
                  </Button>
                </DialogTrigger>
                <DialogContent
                  showCloseButton
                  className="max-h-[80vh] max-w-xl overflow-y-auto"
                >
                  <DialogHeader>
                    <DialogTitle className="text-xl">{agent?.name}</DialogTitle>
                    <DialogDescription className="flex items-center gap-2">
                      <Badge variant="outline" className="font-normal">
                        v{agent?.apiData?.network_tool?.version}
                      </Badge>
                      <span className="text-sm">
                        by {agent?.apiData?.network_tool?.author}
                      </span>
                    </DialogDescription>
                  </DialogHeader>
                  <div className="space-y-6">
                    <p>{agent?.apiData?.network_tool?.description}</p>

                    {!isFreePricing && !isWalletConnected && (
                      <SetupGuide isWalletConnected={!!isWalletConnected} />
                    )}
                    <div className="flex justify-between py-2">
                      <span className="text-official-gray-400 text-sm">
                        {t('networkAgentsPage.toolRouterKey')}
                      </span>
                      <span className="font-mono text-xs break-all">
                        {agent?.toolRouterKey}
                      </span>
                    </div>

                    <div className="bg-official-gray-850 flex items-center justify-between rounded-md p-3">
                      <div className="flex items-center gap-3">
                        <CreditCard className="h-5 w-5" />
                        <div>
                          <p className="font-medium">
                            {t('networkAgentsPage.costPerUse')}
                          </p>
                          <p className="text-official-gray-400 text-sm">
                            {t('networkAgentsPage.costPerUseDescription')}
                          </p>
                        </div>
                      </div>
                      {isFreePricing ? (
                        <div className="inline-flex gap-1 py-1.5 text-lg">
                          Free
                        </div>
                      ) : (
                        <div className="inline-flex items-center gap-1 py-1.5 text-lg">
                          {formatBalanceAmount(amount ?? '0', 6)}{' '}
                          <span className="text-official-gray-200 text-sm font-medium">
                            {ticker}
                          </span>
                        </div>
                      )}
                    </div>

                    <div className="rounded-md border border-cyan-800 bg-cyan-900/10 p-3">
                      <div className="flex gap-2">
                        <Info className="mt-0.5 h-4 w-4 shrink-0 text-cyan-400" />
                        <div>
                          <p className="text-sm font-medium text-cyan-400">
                            {t('networkAgentsPage.howPaymentsWork')}
                          </p>
                          <p className="text-official-gray-400 text-sm">
                            {t('networkAgentsPage.howPaymentsWorkDescription', {
                              network:
                                agent?.apiData?.tool_offering?.usage_type
                                  ?.PerUse &&
                                typeof agent?.apiData?.tool_offering?.usage_type
                                  ?.PerUse === 'object' &&
                                'Payment' in
                                  agent?.apiData?.tool_offering?.usage_type
                                    ?.PerUse
                                  ? agent?.apiData?.tool_offering?.usage_type
                                      ?.PerUse?.Payment?.[0]?.network
                                  : undefined,
                            })}
                          </p>
                        </div>
                      </div>
                    </div>
                  </div>

                  {allowInstall && (
                    <DialogFooter className="ml-auto w-full max-w-[300px] flex-row gap-1">
                      <Button
                        variant="outline"
                        size="md"
                        className="flex-1 px-4"
                        onClick={() => setShowDetailsModal(false)}
                      >
                        {t('common.cancel')}
                      </Button>
                      <Button
                        size="md"
                        className="flex-1"
                        onClick={() => {
                          setShowInstallModal(true);
                          setShowDetailsModal(false);
                        }}
                      >
                        {t('networkAgentsPage.nextAddAgent')}
                      </Button>
                    </DialogFooter>
                  )}
                  {isInstalled && (
                    <DialogFooter>
                      <RemoveNetworkAgentButton
                        toolRouterKey={agent.toolRouterKey}
                      />
                    </DialogFooter>
                  )}
                </DialogContent>
              </Dialog>
              {!isInstalled && (
                <Button
                  variant="outline"
                  onClick={() => setShowInstallModal(true)}
                  size="sm"
                >
                  <PlusIcon className="h-4 w-4" />
                  {t('agentsPage.addAgent')}
                </Button>
              )}

              {isInstalled && (
                <RemoveNetworkAgentButton toolRouterKey={agent.toolRouterKey} />
              )}
            </div>
          )}
          {type === 'exposed' && (
            <div className="ml-auto flex w-full max-w-[300px] items-center justify-end gap-3">
              <ConfigureAgentDialog agent={agent} />
              <RemoveToolOfferingButton toolKey={agent.toolRouterKey} />
            </div>
          )}
        </div>
      </CardContent>
      <InstallAgentModal
        isOpen={showInstallModal}
        onClose={() => setShowInstallModal(false)}
        agent={agent}
        isInstalled={!!isInstalled}
        allowInstall={!!allowInstall}
      />
    </Card>
  );
};

export default AgentCard;

interface InstallAgentModalProps {
  isOpen: boolean;
  onClose: () => void;
  agent: FormattedNetworkAgent;
  isInstalled?: boolean;
  allowInstall?: boolean;
}

export const InstallAgentModal = ({
  isOpen,
  onClose,
  agent,

  allowInstall,
}: InstallAgentModalProps) => {
  const { t } = useTranslation();
  const [step, setStep] = useState<1 | 2>(1); // 1: confirm, 2: success

  const auth = useAuth((state) => state.auth);
  const { data: walletInfo } = useGetWalletList({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
  });

  const isWalletConnected =
    walletInfo?.payment_wallet || walletInfo?.receiving_wallet;

  const [addedAgentId, setAddedAgentId] = useState<string | undefined>();

  const { mutateAsync: addNetworkTool, isPending: isAddingAgent } =
    useAddNetworkTool({
      onError: (error) => {
        toast.error('Failed to add network agent', {
          description: error.response?.data?.message ?? error.message,
        });
      },
      onSuccess: (data) => {
        setAddedAgentId(data?.agent_id);
        setStep(2);
      },
    });

  const navigate = useNavigate();

  if (!agent) return null;

  const handleAddAgent = async () => {
    if (!auth) return;

    await addNetworkTool({
      nodeAddress: auth.node_address,
      token: auth.api_v2_key,
      networkTool: agent.apiData.network_tool,
    });
  };

  const handleClose = () => {
    setStep(1);
    onClose();
  };

  const isFreePricing =
    agent.price.toLowerCase().includes('free') || agent.price === '$0.00';

  const amount =
    agent.apiData.tool_offering.usage_type?.PerUse &&
    typeof agent.apiData.tool_offering.usage_type?.PerUse === 'object' &&
    'Payment' in agent.apiData.tool_offering.usage_type?.PerUse
      ? agent.apiData.tool_offering.usage_type?.PerUse?.Payment?.[0]
          ?.maxAmountRequired
      : undefined;

  const ticker =
    agent.apiData.tool_offering.usage_type?.PerUse &&
    typeof agent.apiData.tool_offering.usage_type?.PerUse === 'object' &&
    'Payment' in agent.apiData.tool_offering.usage_type?.PerUse
      ? agent.apiData.tool_offering.usage_type?.PerUse?.Payment?.[0]?.extra
          ?.name
      : undefined;

  return (
    <Dialog open={isOpen} onOpenChange={handleClose}>
      <DialogContent className="max-w-xl text-white">
        <DialogHeader>
          <DialogTitle className="flex items-center space-x-3 text-xl">
            {/* <div className="flex h-10 w-10 items-center justify-center rounded-xl bg-gradient-to-br from-blue-500 to-purple-500 text-xl">
              {agent.icon}
            </div> */}
            <span>Add {agent.name}</span>
          </DialogTitle>
          <DialogDescription className="text-official-gray-400">
            {step === 1 && t('networkAgentsPage.addAgent')}
            {step === 2 && t('networkAgentsPage.addedSuccess')}
          </DialogDescription>
        </DialogHeader>

        {!isFreePricing && !isWalletConnected && (
          <SetupGuide isWalletConnected={!!isWalletConnected} />
        )}
        {step === 1 && (
          <div className="space-y-6">
            <div className="rounded-lg border border-cyan-800 bg-cyan-900/10 p-4">
              <div className="flex flex-col gap-2.5">
                <div className="flex items-center gap-2">
                  <Sparkles className="h-4 w-4 text-cyan-400" />
                  <p className="text-sm font-medium text-cyan-400">
                    {t('networkAgentsPage.howPaymentsWork')}
                  </p>
                </div>
                <div>
                  <div className="text-official-gray-100 space-y-2 text-sm">
                    <p>
                      <strong>Free to add</strong> - No cost to add agents to
                      your collection
                    </p>
                    <p>
                      <strong>Pay per use</strong> -{' '}
                      {isFreePricing ? (
                        'Free to use in chat.'
                      ) : (
                        <>
                          Only pay{' '}
                          <strong className="text-white">
                            {`${formatBalanceAmount(amount ?? '0', 6)} ${ticker}`}
                          </strong>{' '}
                          when you use it in chat. You'll see a payment
                          confirmation before any charges.
                        </>
                      )}
                    </p>
                  </div>
                </div>
              </div>
            </div>

            <div className="bg-official-gray-900 space-y-3 rounded-lg p-4">
              <div className="flex items-start justify-between">
                <div>
                  <h3 className="text-lg font-semibold text-white">
                    {agent.name}
                  </h3>
                  <p className="text-official-gray-400 text-sm">
                    by {agent.provider}
                  </p>
                </div>
              </div>
              <p className="text-official-gray-400 text-sm leading-relaxed">
                {agent.description}
              </p>

              <div className="border-official-gray-780 border-t pt-2">
                <div className="flex items-center justify-between text-sm">
                  <span className="text-white">
                    {t('networkAgentsPage.costPerUse')}:
                  </span>
                  <span className="font-semibold text-white">
                    {isFreePricing
                      ? 'FREE'
                      : `${formatBalanceAmount(amount ?? '0', 6)} ${ticker}`}
                  </span>
                </div>
              </div>
            </div>

            {allowInstall && (
              <div className="ml-auto flex max-w-[300px] items-center gap-2">
                <Button
                  variant="outline"
                  onClick={handleClose}
                  className="flex-1"
                  size="md"
                >
                  Cancel
                </Button>
                <Button
                  onClick={handleAddAgent}
                  size="md"
                  className="flex-1"
                  isLoading={isAddingAgent}
                >
                  {isAddingAgent ? null : <Plus className="h-4 w-4" />}
                  {t('agentsPage.addAgent')}
                </Button>
              </div>
            )}
          </div>
        )}

        {step === 2 && (
          <div className="py-8 text-center">
            <CheckCircle className="mx-auto mb-4 h-12 w-12 text-green-400" />
            <h3 className="mb-2 text-base font-semibold text-white">
              {t('networkAgentsPage.addedSuccess')}
            </h3>
            <p className="text-official-gray-400 mb-6 text-sm">
              {t('networkAgentsPage.addedDescription', { name: agent.name })}
            </p>
            <div className="mx-auto flex w-full max-w-sm gap-3">
              <Button
                variant="outline"
                onClick={handleClose}
                className="flex-1"
                size="md"
              >
                {t('networkAgentsPage.browseMore')}
              </Button>
              <Button
                onClick={async () => {
                  handleClose();
                  await navigate('/home', {
                    state: {
                      agentName: addedAgentId ?? agent.id,
                    },
                  });
                }}
                size="md"
                className="flex-1"
              >
                {t('networkAgentsPage.startChat')}
              </Button>
            </div>
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
};

interface SetupGuideProps {
  isWalletConnected: boolean;
}
function SetupGuide({ isWalletConnected }: SetupGuideProps) {
  const { t } = useTranslation();

  return (
    <Alert variant="warning" className="mb-6 border-0 bg-yellow-300/5 p-6">
      <div className="flex flex-col gap-4">
        <AlertTitle className="text-base font-semibold text-white">
          {t('networkAgentsPage.setupRequired')}
        </AlertTitle>
        <AlertDescription>
          <div className="space-y-5">
            <div className="flex items-center justify-between gap-3">
              <div className="flex items-start gap-3">
                {isWalletConnected ? (
                  <CheckCircle2 className="mt-0.5 size-4 shrink-0 text-green-500" />
                ) : (
                  <AlertCircle className="mt-0.5 size-4 shrink-0 text-yellow-400" />
                )}
                <div>
                  <p className="text-sm font-medium text-white">
                    {t('networkAgentsPage.connectWallet')}
                  </p>
                  <p className="text-official-gray-200 text-sm">
                    {t('networkAgentsPage.connectWalletDescription')}
                  </p>
                </div>
              </div>
              {!isWalletConnected && (
                <Link
                  to="/settings/crypto-wallet"
                  className={cn(
                    buttonVariants({ variant: 'outline', size: 'md' }),
                    'shrink-0 bg-yellow-400 text-black hover:bg-yellow-500 hover:text-black',
                  )}
                >
                  {t('networkAgentsPage.connectWallet')}
                </Link>
              )}
            </div>
          </div>
        </AlertDescription>
      </div>
    </Alert>
  );
}

function Disclaimer() {
  const dismissedCommunityAgentsDisclaimer = useSettings(
    (state) => state.dismissedCommunityAgentsDisclaimer,
  );
  const setDismissedCommunityAgentsDisclaimer = useSettings(
    (state) => state.setDismissedCommunityAgentsDisclaimer,
  );

  if (dismissedCommunityAgentsDisclaimer) return null;

  return (
    <Alert variant="warning" className="mb-6 border-0 bg-orange-300/5 p-6">
      <button
        className="ring-offset-background focus:ring-ring data-[state=open]:bg-accent data-[state=open]:text-muted-foreground absolute top-4 right-4 rounded-xs opacity-70 transition-opacity hover:opacity-100 focus:ring-2 focus:ring-offset-2 focus:outline-hidden disabled:pointer-events-none"
        onClick={() => setDismissedCommunityAgentsDisclaimer(true)}
      >
        <X className="h-4 w-4" />
        <span className="sr-only">Close</span>
      </button>
      <div className="flex items-start gap-3">
        <AlertCircle className="mt-0.5 size-4 shrink-0 text-orange-400" />
        <div>
          <p className="text-sm font-medium text-white">Community Agents</p>
          <p className="text-official-gray-200 text-sm">
            Community agents are not verified by Shinkai. We may remove agents
            that violate our terms. Use at your own risk.
          </p>
        </div>
      </div>
    </Alert>
  );
}
