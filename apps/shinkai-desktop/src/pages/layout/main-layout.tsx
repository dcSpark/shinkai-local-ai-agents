import { ExitIcon, GearIcon } from '@radix-ui/react-icons';
import { PopoverClose } from '@radix-ui/react-popover';
import { useTranslation } from '@shinkai_network/shinkai-i18n';
import { useImportAgentFromUrl } from '@shinkai_network/shinkai-node-state/v2/mutations/importAgentFromUrl/useImportAgentFromUrl';
import { useImportTool } from '@shinkai_network/shinkai-node-state/v2/mutations/importTool/useImportTool';
import { useGetHealth } from '@shinkai_network/shinkai-node-state/v2/queries/getHealth/useGetHealth';
import { useGetInboxesWithPagination } from '@shinkai_network/shinkai-node-state/v2/queries/getInboxes/useGetInboxesWithPagination';
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
  Popover,
  PopoverContent,
  PopoverTrigger,
  Tooltip,
  TooltipContent,
  TooltipPortal,
  TooltipProvider,
  TooltipTrigger,
} from '@shinkai_network/shinkai-ui';
import {
  AIAgentIcon,
  AISearchContentIcon,
  AisIcon,
  FilesIcon,
  HomeIcon,
  InboxIcon,
  MCPIcon,
  NetworkAgentIcon,
  ScheduledTasksIcon,
  ShinkaiCombinationMarkIcon,
  ToolsIcon,
} from '@shinkai_network/shinkai-ui/assets';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { AnimatePresence, motion } from 'framer-motion';
import {
  ArrowLeftToLine,
  ArrowRightToLine,
  Ellipsis,
  HelpCircleIcon,
} from 'lucide-react';
import React, { Fragment, useEffect, useState } from 'react';
import { Link, Outlet, useLocation, useMatch, useNavigate } from 'react-router';
import { toast } from 'sonner';

import { ResourcesBanner } from '../../components/hardware-capabilities/resources-banner';
import { UpdateBanner } from '../../components/hardware-capabilities/update-banner';
import OnboardingStepper from '../../components/onboarding-checklist/onboarding';
import { ResetConnectionDialog } from '../../components/reset-connection-dialog';
import config from '../../config';
import { useAuth } from '../../store/auth';
import { useSettings } from '../../store/settings';
import { useViewportStore } from '../../store/viewport';

type NavigationLink = {
  title: string;
  href: string;
  icon: React.ReactNode;
  onClick?: () => void;
  external?: boolean;
  disabled?: boolean;
};

export const showAnimation = {
  hidden: {
    width: 0,
    opacity: 0,
    transition: {
      duration: 0.3,
    },
  },
  show: {
    opacity: 1,
    width: 'auto',
    transition: {
      duration: 0.3,
    },
  },
};

const NavLink = ({
  href,
  external,
  onClick,
  icon,
  title,
  disabled,
  isPopover,
}: {
  href: string;
  external?: boolean;
  onClick?: () => void;
  icon: React.ReactNode;
  title: string;
  disabled?: boolean;
  isPopover?: boolean;
}) => {
  const { t } = useTranslation();
  const sidebarExpanded = useSettings((state) => state.sidebarExpanded);

  const isMatch = useMatch({
    path: href,
    end: true,
  });

  if (disabled) {
    return (
      <div
        className={cn(
          'flex w-full items-center gap-2 rounded-lg px-4 py-3 text-white transition-colors',
          'opacity-40',
        )}
      >
        <span>{icon}</span>
        {sidebarExpanded && <span className="sr-only">{title}</span>}
        <AnimatePresence initial={false}>
          {sidebarExpanded && (
            <motion.div
              animate="show"
              className="flex items-center gap-3 overflow-hidden text-xs whitespace-nowrap"
              exit="hidden"
              initial="hidden"
              variants={showAnimation}
            >
              <span className="max-w-[100px] truncate">{title} </span>
              <Badge className="text-[10px] uppercase" variant="inputAdornment">
                {t('common.soon')}
              </Badge>
            </motion.div>
          )}
        </AnimatePresence>
      </div>
    );
  }

  return (
    <Link
      className={cn(
        'flex w-full items-center gap-2.5 rounded-lg px-4 py-3 text-white transition-colors',
        isMatch
          ? 'bg-white/10 text-white shadow-xl'
          : 'opacity-60 hover:bg-white/10 hover:opacity-100',
      )}
      onClick={onClick}
      rel={external ? 'noreferrer' : ''}
      target={external ? '_blank' : ''}
      to={href}
    >
      <span>{icon}</span>
      {sidebarExpanded && <span className="sr-only">{title}</span>}
      <AnimatePresence initial={false}>
        {(sidebarExpanded || isPopover) && (
          <motion.div
            animate="show"
            className="overflow-hidden text-sm whitespace-nowrap"
            exit="hidden"
            initial="hidden"
            variants={showAnimation}
          >
            {title}
          </motion.div>
        )}
      </AnimatePresence>
    </Link>
  );
};

export function MainNav() {
  const { t, Trans } = useTranslation();
  const optInExperimental = useSettings((state) => state.optInExperimental);
  const auth = useAuth((state) => state.auth);
  const logout = useAuth((state) => state.setLogout);
  const resetSettings = useSettings((state) => state.resetSettings);
  const isGetStartedChecklistHidden = useSettings(
    (state) => state.isGetStartedChecklistHidden,
  );

  const [isConfirmLogoutDialogOpened, setIsConfirmLogoutDialogOpened] =
    useState(false);
  const [isApiV2KeyMissingDialogOpen, setIsApiV2KeyMissingDialogOpen] =
    useState(false);

  const sidebarExpanded = useSettings((state) => state.sidebarExpanded);
  const toggleSidebar = useSettings((state) => state.toggleSidebar);

  const confirmDisconnect = () => {
    setIsConfirmLogoutDialogOpened(true);
  };

  const { data: inboxesPagination } = useGetInboxesWithPagination({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
  });

  useEffect(() => {
    if (!auth?.api_v2_key) {
      setIsApiV2KeyMissingDialogOpen(true);
    }
  }, [auth?.api_v2_key]);

  const handleDisconnect = () => {
    logout();
    resetSettings();
  };

  const navigationLinks = [
    {
      title: 'Home',
      href: '/home',
      icon: <HomeIcon className="size-[18px]" />,
    },
    {
      title: t('layout.menuItems.chats'),
      href: `/inboxes/${
        inboxesPagination?.pages[0]?.inboxes
          ? encodeURIComponent(
              inboxesPagination?.pages[0]?.inboxes[0]?.inbox_id ?? '',
            )
          : ''
      }`,
      icon: <InboxIcon className="size-[18px]" />,
    },
    {
      title: t('layout.menuItems.agents'),
      href: '/agents',
      icon: <AIAgentIcon className="size-[18px]" name={''} />,
    },
    config.isDev &&
      optInExperimental && {
        title: t('layout.menuItems.vectorSearch'),
        href: '/vector-search',
        icon: <AISearchContentIcon className="size-[18px]" />,
      },
  ].filter(Boolean) as NavigationLink[];

  const secondaryNavigationLinks = [
    {
      title: t('layout.menuItems.tools'),
      href: '/tools',
      icon: <ToolsIcon className="size-[18px]" />,
    },
    {
      title: t('layout.menuItems.mcp'),
      href: '/mcp',
      icon: <MCPIcon className="size-[18px]" />,
    },

    {
      title: t('layout.menuItems.decentralizedAgents'),
      href: '/network-ai-agents',
      icon: <NetworkAgentIcon className="size-[18px]" />,
    },
    {
      title: t('layout.menuItems.vectorFs'),
      href: '/vector-fs',
      icon: <FilesIcon className="size-[18px]" />,
    },
    {
      title: t('layout.menuItems.scheduledTasks'),
      href: '/tasks',
      icon: <ScheduledTasksIcon className="size-[18px]" />,
    },
  ];
  const footerNavigationLinks = [
    {
      title: t('layout.menuItems.manageAis'),
      href: '/ais',
      icon: <AisIcon className="size-[18px]" />,
    },
    {
      title: t('layout.menuItems.helpAndSupport'),
      href: `https://docs.shinkai.com/`,
      icon: <HelpCircleIcon className="size-[18px]" />,
      external: true,
    },
    {
      title: t('layout.menuItems.settings'),
      href: '/settings',
      icon: <GearIcon className="size-[18px]" />,
    },
    config.isDev && {
      title: t('layout.menuItems.disconnect'),
      href: '#',
      icon: <ExitIcon className="size-[18px]" />,
      onClick: () => confirmDisconnect(),
    },
  ].filter(Boolean) as NavigationLink[];

  return (
    <motion.aside
      animate={{
        width: sidebarExpanded ? '230px' : '70px',
        opacity: 1,
      }}
      className="bg-bg-secondary border-border relative z-30 flex w-auto shrink-0 flex-col gap-2 overflow-x-hidden overflow-y-auto border-r px-2 py-6 pt-9 shadow-xl"
      exit={{ width: 0, opacity: 0 }}
      initial={{ width: 0, opacity: 0 }}
      transition={{ duration: 0.3 }}
    >
      <div
        className={cn(
          'text-text-secondary flex w-full items-center justify-between gap-2 py-2 pl-4',
          !sidebarExpanded && 'justify-center px-0',
        )}
      >
        {sidebarExpanded && (
          <ShinkaiCombinationMarkIcon className="text-official-gray-100 h-auto w-[90px]" />
        )}

        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              className={cn(
                'border-official-gray-780 text-text-secondary h-6 w-6 shrink-0 rounded-lg border bg-black/20 p-0 hover:bg-black/20 hover:text-white',
              )}
              onClick={toggleSidebar}
              size="auto"
              type="button"
              variant="ghost"
            >
              {sidebarExpanded ? (
                <ArrowLeftToLine className="h-3 w-3" />
              ) : (
                <ArrowRightToLine className="h-3 w-3" />
              )}
            </Button>
          </TooltipTrigger>
          <TooltipPortal>
            <TooltipContent align="center" side="right">
              {t('layout.sidebar.toggle')}
            </TooltipContent>
          </TooltipPortal>
        </Tooltip>
      </div>

      <div className="flex flex-1 flex-col justify-between">
        <div className="flex flex-col gap-1.5">
          {!isGetStartedChecklistHidden && <OnboardingStepper />}
          {navigationLinks.map((item) => {
            return (
              <Fragment key={item.title}>
                <TooltipProvider
                  delayDuration={
                    item.disabled ? 0 : !sidebarExpanded ? 0 : 10000
                  }
                  key={item.title}
                >
                  <Tooltip>
                    <TooltipTrigger className="flex items-center gap-1">
                      <NavLink
                        disabled={item.disabled}
                        external={item.external}
                        href={item.href}
                        icon={item.icon}
                        onClick={item.onClick}
                        title={item.title}
                      />
                    </TooltipTrigger>
                    <TooltipPortal>
                      <TooltipContent
                        align="center"
                        arrowPadding={2}
                        side="right"
                      >
                        {item.disabled ? (
                          <>
                            {item.title} <br />
                            <span className="text-text-secondary text-xs">
                              {t('common.comingSoon')}
                            </span>
                          </>
                        ) : (
                          <div className="flex flex-col gap-1">
                            {item.title}
                            {item.href === '/home' ? (
                              <div className="text-text-secondary flex items-center justify-center gap-2 text-center">
                                <span>⌘</span>
                                <span>N</span>
                              </div>
                            ) : null}
                          </div>
                        )}
                      </TooltipContent>
                    </TooltipPortal>
                  </Tooltip>
                </TooltipProvider>
              </Fragment>
            );
          })}
          <Popover>
            <TooltipProvider delayDuration={!sidebarExpanded ? 0 : 10000}>
              <Tooltip>
                <TooltipTrigger asChild>
                  <PopoverTrigger asChild>
                    <Button
                      className={cn(
                        'flex w-full items-center gap-2.5 rounded-lg bg-transparent px-4 py-2.5 text-white transition-colors',
                        'opacity-60 hover:bg-white/10 hover:opacity-100',
                      )}
                      size="auto"
                      style={{
                        justifyContent: sidebarExpanded
                          ? 'flex-start'
                          : 'center',
                      }}
                      variant="ghost"
                    >
                      <Ellipsis className="size-[18px]" />
                      <span
                        className={cn(
                          'text-sm font-normal',
                          !sidebarExpanded && 'sr-only',
                        )}
                      >
                        {t('common.more')}
                      </span>
                    </Button>
                  </PopoverTrigger>
                </TooltipTrigger>
                {!sidebarExpanded && (
                  <TooltipPortal>
                    <TooltipContent
                      align="center"
                      arrowPadding={2}
                      side="right"
                    >
                      <p>{t('common.more')}</p>
                    </TooltipContent>
                  </TooltipPortal>
                )}
              </Tooltip>
            </TooltipProvider>
            <PopoverContent
              align="start"
              className="w-[240px] p-2"
              sideOffset={8}
            >
              <div className="flex flex-col gap-1">
                {secondaryNavigationLinks.map((item) => {
                  return (
                    <PopoverClose asChild key={item.title}>
                      <NavLink
                        href={item.href}
                        icon={item.icon}
                        isPopover
                        title={item.title}
                      />
                    </PopoverClose>
                  );
                })}
              </div>
            </PopoverContent>
          </Popover>
        </div>
        <div className="flex flex-col gap-1">
          <ResourcesBanner isInSidebar />
          <UpdateBanner />
          {footerNavigationLinks.map((item) => {
            return (
              <React.Fragment key={item.title}>
                <TooltipProvider
                  delayDuration={!sidebarExpanded ? 0 : 10000}
                  key={item.title}
                >
                  <Tooltip>
                    <TooltipTrigger className="flex items-center gap-1">
                      <NavLink
                        external={item.external}
                        href={item.href}
                        icon={item.icon}
                        onClick={item.onClick}
                        title={item.title}
                      />
                    </TooltipTrigger>
                    <TooltipPortal>
                      <TooltipContent
                        align="center"
                        arrowPadding={2}
                        side="right"
                      >
                        <p>{item.title}</p>
                      </TooltipContent>
                    </TooltipPortal>
                  </Tooltip>
                </TooltipProvider>
              </React.Fragment>
            );
          })}
        </div>
      </div>

      <ResetConnectionDialog
        isOpen={isApiV2KeyMissingDialogOpen}
        onOpenChange={setIsApiV2KeyMissingDialogOpen}
      />
      <AlertDialog
        onOpenChange={setIsConfirmLogoutDialogOpened}
        open={isConfirmLogoutDialogOpened}
      >
        <AlertDialogContent className="w-[75%]">
          <AlertDialogHeader>
            <AlertDialogTitle>{t('disconnect.modalTitle')}</AlertDialogTitle>
            <AlertDialogDescription>
              <div className="flex flex-col space-y-3 text-left text-white/70">
                <div className="flex flex-col space-y-1">
                  <span className="text-sm">
                    {t('disconnect.modalDescription')}
                  </span>
                </div>
                <div className="text-sm">
                  <Trans
                    components={{
                      Link: (
                        <Link
                          className="mx-0.5 inline-block cursor-pointer text-white underline"
                          onClick={() => {
                            setIsConfirmLogoutDialogOpened(false);
                          }}
                          to={'/settings/export-connection'}
                        />
                      ),
                    }}
                    i18nKey="disconnect.exportConnection"
                  />
                </div>
              </div>
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter className="mt-4 flex justify-end gap-2">
            <AlertDialogCancel
              className="mt-0 min-w-[120px]"
              onClick={() => {
                setIsConfirmLogoutDialogOpened(false);
              }}
            >
              {t('common.cancel')}
            </AlertDialogCancel>
            <AlertDialogAction
              className="min-w-[120px]"
              onClick={handleDisconnect}
            >
              {t('common.disconnect')}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </motion.aside>
  );
}

const MainLayout = () => {
  const { t } = useTranslation();
  const location = useLocation();
  const auth = useAuth((state) => state.auth);
  const navigate = useNavigate();
  const [needsResetApp, setNeedsResetApp] = useState(false);
  const { nodeInfo, isSuccess, isFetching } = useGetHealth(
    { nodeAddress: auth?.node_address ?? '' },
    { refetchInterval: 35000, enabled: !!auth },
  );

  const { mutateAsync: importTool } = useImportTool({
    onSuccess: async (data) => {
      toast.success('Installation successful', {
        description:
          'Congratulations! Your tool is now installed and ready to use it in the app.',
        action: {
          label: 'View',
          onClick: () => {
            void navigate(`/tools/${data.tool_key}`);
          },
        },
      });
      await navigate('/tools');
    },
    onError: (error) => {
      toast.error('Failed to install tool', {
        description: error.response?.data?.message ?? error.message,
      });
    },
  });

  const { mutateAsync: importAgentFromUrl } = useImportAgentFromUrl({
    onSuccess: async () => {
      await navigate('/agents');
      toast.success('Agent imported successfully', {
        description: 'Your agent is now ready to use in the app.',
      });
    },
    onError: (error) => {
      toast.error('Failed to import agent', {
        description: error.response?.data?.message ?? error.message,
      });
    },
  });

  useEffect(() => {
    const unlisten = listen('store-deep-link', async (event) => {
      if (!auth) return;
      const payload = event.payload as { tool_type: string; tool_url: string };
      if (payload.tool_type.toLowerCase() === 'tool') {
        await importTool({
          nodeAddress: auth?.node_address ?? '',
          token: auth?.api_v2_key ?? '',
          url: payload.tool_url,
        });
      } else if (payload.tool_type.toLowerCase() === 'agent') {
        try {
          await importAgentFromUrl({
            nodeAddress: auth?.node_address ?? '',
            token: auth?.api_v2_key ?? '',
            url: payload.tool_url,
          });
        } catch (error) {
          console.error('Error processing agent deep link:', error);
          toast.error('Failed to process agent from store', {
            description:
              error instanceof Error
                ? error.message
                : 'An unexpected error occurred.',
          });
        }
      }
    });
    void getCurrentWindow().emit('shinkai-app-ready');
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [importTool, importAgentFromUrl, auth]);

  useEffect(() => {
    if (isSuccess && nodeInfo?.status !== 'ok') {
      toast.error(t('errors.nodeUnavailable.title'), {
        description: t('errors.nodeUnavailable.description'),
        id: 'node-unavailable',
        duration: 20000,
      });
    }
    if (isSuccess && nodeInfo?.status === 'ok') {
      toast.dismiss('node-unavailable');
    }
    if (isSuccess && nodeInfo?.update_requires_reset) {
      setNeedsResetApp(true);
    } else {
      setNeedsResetApp(false);
    }
  }, [isSuccess, nodeInfo, isFetching, t]);

  const disabledSidebarRoutes = [
    '/terms-conditions',
    '/analytics',
    '/ai-provider-selection',
    '/quick-connection',
    '/restore',
    '/connect-qr',
  ];

  const displaySidebar =
    !!auth && !disabledSidebarRoutes.includes(location.pathname);

  const mainLayoutContainerRef = useViewportStore(
    (state) => state.mainLayoutContainerRef,
  );

  return (
    <div className="bg-bg-default text-text-default relative flex h-screen min-h-full flex-col overflow-hidden">
      <div
        className="absolute top-0 z-50 h-6 w-full"
        data-tauri-drag-region={true}
      />
      <div className={cn('flex h-full flex-1')}>
        <AnimatePresence initial={false}>
          {displaySidebar && <MainNav />}
        </AnimatePresence>

        <div
          className={cn('min-h-full flex-1 overflow-y-scroll')}
          ref={mainLayoutContainerRef}
        >
          <Outlet />
        </div>
      </div>
      <ResetConnectionDialog
        isOpen={needsResetApp}
        onOpenChange={setNeedsResetApp}
      />
    </div>
  );
};
export default MainLayout;
