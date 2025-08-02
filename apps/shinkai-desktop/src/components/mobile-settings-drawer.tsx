import { useTranslation } from '@shinkai_network/shinkai-i18n';
import {
  Drawer,
  DrawerContent,
  DrawerHeader,
  DrawerTitle,
  DrawerDescription,
  DrawerFooter,
  DrawerClose,
  Button,
} from '@shinkai_network/shinkai-ui';
import {
  ExportIcon,
  PromptLibraryIcon,
  ShortcutsIcon,
} from '@shinkai_network/shinkai-ui/assets';
import { AnimatePresence, motion } from 'framer-motion';
import {
  ArrowLeft,
  BarChart2,
  ChevronRight,
  CodesandboxIcon,
  LaptopMinimal,
  PaintbrushIcon,
  SettingsIcon,
  WalletMinimal,
  XIcon,
} from 'lucide-react';
import React, { useState, useRef, useMemo } from 'react';

import galxeIcon from '../assets/galxe-icon.png';
import { openShinkaiNodeManagerWindow } from '../lib/shinkai-node-manager/shinkai-node-manager-windows-utils';
import AnalyticsSettingsPage from '../pages/analytics-settings';
import AppearancePage from '../pages/appearance';
import CryptoWalletPage from '../pages/crypto-wallet';
import { ExportConnection } from '../pages/export-connection';
import { PromptLibrary } from '../pages/prompt-library';
import { PublicKeys } from '../pages/public-keys';
import SettingsPage from '../pages/settings';
import ShortcutsPage from '../pages/shortcuts';
import { useShinkaiNodeManager } from '../store/shinkai-node-manager';
import { WalletsProvider } from './crypto-wallet/context/wallets-context';

type SettingsView =
  | 'overview'
  | 'general'
  | 'appearance'
  | 'shortcuts'
  | 'prompt-library'
  | 'analytics-settings'
  | 'public-keys'
  | 'export-connection'
  | 'remote-access'
  | 'crypto-wallet'
  | 'galxe-validation'
  | 'shinkai-node';

interface MobileSettingsDrawerProps {
  children: React.ReactNode;
}

const MobileSettingsDrawer: React.FC<MobileSettingsDrawerProps> = ({
  children,
}) => {
  const { t } = useTranslation();
  const [currentView, setCurrentView] = useState<SettingsView>('overview');
  const [isOpen, setIsOpen] = useState(false);
  const [direction, setDirection] = useState(0);
  const previousViewRef = useRef<SettingsView>('overview');
  const isLocalShinkaiNodeInUse = useShinkaiNodeManager(
    (state) => state.isInUse,
  );

  const settingsSections = [
    {
      title: 'Preferences',
      items: [
        {
          title: t('settings.layout.general'),
          description: 'Language, AI models, and preferences',
          view: 'general' as SettingsView,
          icon: <SettingsIcon className="text-text-secondary h-5 w-5" />,
        },
        {
          title: t('settings.layout.appearance'),
          description: 'Theme, fonts, and display options',
          view: 'appearance' as SettingsView,
          icon: <PaintbrushIcon className="text-text-secondary h-5 w-5" />,
        },
        {
          title: 'Shortcuts',
          description: 'Keyboard shortcuts and hotkeys',
          view: 'shortcuts' as SettingsView,
          icon: <ShortcutsIcon className="text-text-secondary h-5 w-5" />,
        },
      ],
    },
    {
      title: 'AI & Content',
      items: [
        {
          title: t('settings.layout.promptLibrary'),
          description: 'Manage your prompt templates',
          view: 'prompt-library' as SettingsView,
          icon: <PromptLibraryIcon className="text-text-secondary h-5 w-5" />,
        },
        {
          title: t('settings.layout.analytics'),
          description: 'Usage tracking and privacy settings',
          view: 'analytics-settings' as SettingsView,
          icon: <BarChart2 className="text-text-secondary h-5 w-5" />,
        },
      ],
    },
    {
      title: 'Security & Connection',
      items: [
        {
          title: t('settings.layout.publicKeys'),
          description: 'View and manage encryption keys',
          view: 'public-keys' as SettingsView,
          icon: (
            <svg
              className="text-text-secondary h-5 w-5"
              fill="currentColor"
              stroke="currentColor"
              strokeWidth="0"
              viewBox="0 0 512 512"
            >
              <path d="M261.1 24.8c-6.3 0-12.7.43-19.2 1.18-34.6 4.01-64.8 17.59-86.1 37.06-21.4 19.48-34.2 45.56-31 73.16 2.8 24.6 17.8 45.2 39.1 59.4 2.6-6.2 5.9-11.9 9.2-16.5-17.6-11.6-28.4-27.3-30.4-45-2.3-19.7 6.7-39.58 24.8-56.14 18.2-16.57 45.3-29.06 76.6-32.68 31.3-3.63 60.6 2.33 82.1 14.3 21.4 11.98 34.7 29.31 37 48.92 2.2 19.3-6.2 38.8-23.4 55a69.91 69.91 0 0 0-35.4-10.6h-2.2c-5.1.1-10.1.7-15.3 1.8-37.5 8.7-60.8 45.5-52.2 82.7 5.3 23 21.6 40.6 42.2 48.5l39.7 172.2 47 29.1 29.5-46.7-23.5-14.5 14.8-23.4-23.5-14.6 14.7-23.3-23.5-14.6 14.8-23.4-13.5-58.4c15.1-16.1 22-39.1 16.7-62.2-2.7-11.7-8.2-22-15.8-30.4 18.9-19 29.8-43.5 26.8-69.2-3.2-27.55-21.6-50.04-46.9-64.11-20.5-11.45-45.8-17.77-73.1-17.59zm-20.2 135.5c-25.9 1.1-49.9 16.8-60.4 42.2-9.1 21.9-6 45.7 6.2 64.2l-67.8 163 21.3 51 51.2-20.9-10.7-25.5 25.6-10.4-10.6-25.5 25.6-10.4-10.7-25.5 25.6-10.5 22.8-54.8c-20.5-11.5-36.2-31.2-41.9-55.8-6.9-30.3 3.1-60.6 23.8-81.1zm58 7.2c8.9-.1 17.3 3.5 23.4 9.4-5.5 3.5-11.6 6.6-18 9.4-1.6-.6-3.3-.8-5.1-.8-.6 0-1.1 0-1.6.1-7 .8-12.2 6.1-13.1 12.7-.2 1-.2 2-.2 2.9.1.3.1.7.1 1 1 8.4 8.3 14.2 16.7 13.2 6.8-.8 12-5.9 13-12.3 6.2-2.8 12-5.9 17.5-9.4.2 1 .4 2 .5 3 2.1 18-11 34.5-29 36.6-17.9 2.1-34.5-11-36.5-29-2.1-18 11-34.5 29-36.6 1.1-.1 2.2-.2 3.3-.2z" />
            </svg>
          ),
        },
        {
          title: t('settings.layout.exportConnection'),
          description: 'Backup and restore connections',
          view: 'export-connection' as SettingsView,
          icon: <ExportIcon className="text-text-secondary h-5 w-5" />,
        },
        {
          title: t('settings.layout.remoteAccess'),
          description: 'Internet access and remote settings',
          view: 'remote-access' as SettingsView,
          icon: <LaptopMinimal className="text-text-secondary h-5 w-5" />,
        },
      ],
    },
    {
      title: 'Integrations',
      items: [
        {
          title: t('settings.layout.cryptoWallet'),
          description: 'Manage your crypto wallet',
          view: 'crypto-wallet' as SettingsView,
          icon: <WalletMinimal className="text-text-secondary h-5 w-5" />,
        },
        {
          title: t('settings.layout.galxe'),
          description: 'Galxe validation and rewards',
          view: 'galxe-validation' as SettingsView,
          icon: (
            <div className="text-text-secondary">
              <img alt="galxe icon" className="h-5 w-5" src={galxeIcon} />
            </div>
          ),
        },
      ],
    },
    ...(isLocalShinkaiNodeInUse
      ? [
          {
            title: 'Advanced',
            items: [
              {
                title: t('settings.layout.shinkaiNode'),
                description: 'Node management and configuration',
                view: 'shinkai-node' as SettingsView,
                onClick: () => {
                  void openShinkaiNodeManagerWindow();
                },
                icon: (
                  <CodesandboxIcon className="text-text-secondary h-5 w-5" />
                ),
              },
            ],
          },
        ]
      : []),
  ];

  const getViewTitle = (view: SettingsView) => {
    const allItems = settingsSections.flatMap((section) => section.items);
    const item = allItems.find((item) => item.view === view);
    return item?.title || 'Settings';
  };

  const handleOpenChange = (open: boolean) => {
    setIsOpen(open);
    if (!open) {
      setCurrentView('overview');
    }
  };

  const variants = {
    initial: (direction: number) => {
      return { x: `${110 * direction}%`, opacity: 0 };
    },
    active: { x: '0%', opacity: 1 },
    exit: (direction: number) => {
      return { x: `${-110 * direction}%`, opacity: 0 };
    },
  };

  const content = useMemo(() => {
    if (currentView === 'overview') {
      return (
        <div className="flex min-h-full flex-col">
          <div className="flex-1 overflow-y-auto pb-6">
            {settingsSections.map((section) => (
              <div key={section.title} className="py-6">
                <div className="px-4 pb-3">
                  <h2 className="text-text-tertiary text-sm font-medium tracking-wider uppercase">
                    {section.title}
                  </h2>
                </div>

                <div className="space-y-2">
                  {section.items.map((item) => (
                    <div key={item.title}>
                      <Button
                        variant="tertiary"
                        onClick={() => {
                          const newView = item.view;
                          if (newView !== currentView) {
                            setDirection(1);
                            previousViewRef.current = currentView;
                            setCurrentView(newView);
                          }
                        }}
                        className="flex w-full items-center justify-between rounded-sm px-4 py-4 transition-colors"
                      >
                        <div className="flex items-center gap-3">
                          <div className="flex-shrink-0">{item.icon}</div>
                          <div className="flex flex-col items-start">
                            <span className="text-text-default text-base font-medium">
                              {item.title}
                            </span>
                            <span className="text-text-secondary text-sm font-normal">
                              {item.description}
                            </span>
                          </div>
                        </div>
                        <ChevronRight className="text-text-tertiary h-5 w-5" />
                      </Button>
                    </div>
                  ))}
                </div>
              </div>
            ))}
          </div>
        </div>
      );
    }
    return (
      <div className="h-full w-full overflow-y-auto">
        {currentView === 'general' && <SettingsPage />}
        {currentView === 'appearance' && <AppearancePage />}
        {currentView === 'shortcuts' && <ShortcutsPage />}
        {currentView === 'prompt-library' && <PromptLibrary />}
        {currentView === 'analytics-settings' && <AnalyticsSettingsPage />}
        {currentView === 'public-keys' && <PublicKeys />}
        {currentView === 'export-connection' && <ExportConnection />}
        {currentView === 'crypto-wallet' && <CryptoWalletPage />}
        {![
          'general',
          'appearance',
          'shortcuts',
          'prompt-library',
          'analytics-settings',
          'public-keys',
          'export-connection',
          'crypto-wallet',
        ].includes(currentView) && (
          <div className="flex h-full items-center justify-center">
            <p className="text-text-secondary">Coming soon...</p>
          </div>
        )}
      </div>
    );
  }, [currentView, settingsSections]);

  const renderSettingsContent = () => {
    return (
      <WalletsProvider>
        <AnimatePresence custom={direction} initial={false} mode="popLayout">
          <motion.div
            animate="active"
            className="h-full w-full"
            custom={direction}
            exit="exit"
            initial="initial"
            key={currentView}
            transition={{ duration: 0.5, type: 'spring', bounce: 0 }}
            variants={variants}
          >
            {content}
          </motion.div>
        </AnimatePresence>
      </WalletsProvider>
    );
  };

  return (
    <div className="md:hidden">
      <Drawer open={isOpen} onOpenChange={handleOpenChange}>
        <div onClick={() => setIsOpen(true)}>{children}</div>
        <DrawerContent className="h-[90vh]">
          <DrawerHeader>
            <div className="flex items-center gap-3">
              {currentView !== 'overview' ? (
                <Button
                  variant="tertiary"
                  size="icon"
                  onClick={() => {
                    setDirection(-1);
                    previousViewRef.current = currentView;
                    setCurrentView('overview');
                  }}
                  className="hover:bg-bg-dark p-2 text-white"
                >
                  <ArrowLeft className="h-5 w-5" />
                  <span className="sr-only">Go back</span>
                </Button>
              ) : (
                <div className="size-[30px]" />
              )}
              <DrawerTitle className="text-text-default w-full text-center">
                {currentView === 'overview'
                  ? 'Settings'
                  : getViewTitle(currentView)}
              </DrawerTitle>
              <Button
                variant="tertiary"
                size="icon"
                onClick={() => setIsOpen(false)}
              >
                <XIcon className="h-5 w-5" />
              </Button>
            </div>
          </DrawerHeader>

          <div className="flex-1 overflow-y-auto px-4">
            {renderSettingsContent()}
          </div>
        </DrawerContent>
      </Drawer>
    </div>
  );
};

export default MobileSettingsDrawer;
