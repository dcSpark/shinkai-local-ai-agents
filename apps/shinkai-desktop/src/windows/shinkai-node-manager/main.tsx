import './globals.css';

import { zodResolver } from '@hookform/resolvers/zod';
import { PlayIcon, StopIcon } from '@radix-ui/react-icons';
import { useTranslation } from '@shinkai_network/shinkai-i18n';
import { useSyncOllamaModels } from '@shinkai_network/shinkai-node-state/v2/mutations/syncOllamaModels/useSyncOllamaModels';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  Button,
  Form,
  FormField,
  ScrollArea,
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
  TextField,
  Toaster,
  Tooltip,
  TooltipContent,
  TooltipPortal,
  TooltipProvider,
  TooltipTrigger,
} from '@shinkai_network/shinkai-ui';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import { QueryClientProvider } from '@tanstack/react-query';
import { info } from '@tauri-apps/plugin-log';
import { Loader2, RefreshCcwIcon, Trash2 } from 'lucide-react';
import React, { useEffect, useState } from 'react';
import ReactDOM from 'react-dom/client';
import { useForm, useWatch } from 'react-hook-form';
import { z } from 'zod';

import logo from '../../../src-tauri/icons/128x128@2x.png';
import { OllamaModels } from '../../components/shinkai-node-manager/ollama-models';
import { ALLOWED_OLLAMA_MODELS } from '../../lib/shinkai-node-manager/ollama-models';
import {
  shinkaiNodeQueryClient,
  useShinkaiNodeGetOptionsQuery,
  useShinkaiNodeIsRunningQuery,
  useShinkaiNodeKillMutation,
  useShinkaiNodeRemoveStorageMutation,
  useShinkaiNodeSetDefaultOptionsMutation,
  useShinkaiNodeSetOptionsMutation,
  useShinkaiNodeSpawnMutation,
} from '../../lib/shinkai-node-manager/shinkai-node-manager-client';
import { type ShinkaiNodeOptions } from '../../lib/shinkai-node-manager/shinkai-node-manager-client-types';
import { useShinkaiNodeEventsToast } from '../../lib/shinkai-node-manager/shinkai-node-manager-hooks';
import {
  errorOllamaModelsSyncToast,
  errorRemovingShinkaiNodeStorageToast,
  shinkaiNodeStartedToast,
  shinkaiNodeStartErrorToast,
  shinkaiNodeStopErrorToast,
  shinkaiNodeStoppedToast,
  startingShinkaiNodeToast,
  stoppingShinkaiNodeToast,
  successOllamaModelsSyncToast,
  successRemovingShinkaiNodeStorageToast,
  successShinkaiNodeSetDefaultOptionsToast,
} from '../../lib/shinkai-node-manager/shinkai-node-manager-toasts-utils';
import { useAuth } from '../../store/auth';
import { useShinkaiNodeManager } from '../../store/shinkai-node-manager';
import { useSyncStorageSecondary } from '../../store/sync-utils';
import { Logs } from './components/logs';

const App = () => {
  const { t } = useTranslation();
  useEffect(() => {
    void info('initializing shinkai-node-manager');
  }, []);
  useSyncStorageSecondary();
  const auth = useAuth((auth) => auth.auth);
  const setLogout = useAuth((auth) => auth.setLogout);
  const { setShinkaiNodeOptions } = useShinkaiNodeManager();
  const [isConfirmResetDialogOpened, setIsConfirmResetDialogOpened] =
    useState<boolean>(false);
  const { data: shinkaiNodeIsRunning } = useShinkaiNodeIsRunningQuery({
    refetchInterval: 1000,
  });
  const { data: shinkaiNodeOptions } = useShinkaiNodeGetOptionsQuery({
    refetchInterval: 1000,
  });

  const {
    isPending: shinkaiNodeSpawnIsPending,
    mutateAsync: shinkaiNodeSpawn,
  } = useShinkaiNodeSpawnMutation({
    onMutate: () => {
      startingShinkaiNodeToast();
    },
    onSuccess: () => {
      shinkaiNodeStartedToast();
    },
    onError: () => {
      shinkaiNodeStartErrorToast();
    },
  });
  const { isPending: shinkaiNodeKillIsPending, mutateAsync: shinkaiNodeKill } =
    useShinkaiNodeKillMutation({
      onMutate: () => {
        stoppingShinkaiNodeToast();
      },
      onSuccess: () => {
        shinkaiNodeStoppedToast();
      },
      onError: () => {
        shinkaiNodeStopErrorToast();
      },
    });
  const {
    isPending: shinkaiNodeRemoveStorageIsPending,
    mutateAsync: shinkaiNodeRemoveStorage,
  } = useShinkaiNodeRemoveStorageMutation({
    onSuccess: async () => {
      successRemovingShinkaiNodeStorageToast();
      setShinkaiNodeOptions(null);
      setLogout();
    },
    onError: () => {
      errorRemovingShinkaiNodeStorageToast();
    },
  });
  const { mutateAsync: shinkaiNodeSetOptions } =
    useShinkaiNodeSetOptionsMutation({
      onSuccess: (options) => {
        setShinkaiNodeOptions(options);
      },
    });
  const { mutateAsync: shinkaiNodeSetDefaultOptions } =
    useShinkaiNodeSetDefaultOptionsMutation({
      onSuccess: (options) => {
        shinkaiNodeOptionsForm.reset(options);
        successShinkaiNodeSetDefaultOptionsToast();
      },
    });
  const shinkaiNodeOptionsForm = useForm<Partial<ShinkaiNodeOptions>>({
    resolver: zodResolver(z.any()),
  });
  const shinkaiNodeOptionsFormWatch = useWatch({
    control: shinkaiNodeOptionsForm.control,
  });
  const {
    mutateAsync: syncOllamaModels,
    isPending: syncOllamaModelsIsPending,
  } = useSyncOllamaModels({
    onSuccess: () => {
      successOllamaModelsSyncToast();
    },
    onError: () => {
      errorOllamaModelsSyncToast();
    },
  });

  useShinkaiNodeEventsToast();

  useEffect(() => {
    const options = {
      ...shinkaiNodeOptions,
      ...shinkaiNodeOptionsFormWatch,
    };
    void shinkaiNodeSetOptions(options as ShinkaiNodeOptions);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [shinkaiNodeOptionsFormWatch, shinkaiNodeSetOptions]);

  const handleReset = (): void => {
    setIsConfirmResetDialogOpened(false);
    void shinkaiNodeRemoveStorage({ preserveKeys: true });
  };

  const startSyncOllamaModels = async () => {
    await syncOllamaModels({
      nodeAddress: auth?.node_address ?? '',
      token: auth?.api_v2_key ?? '',
      allowedModels: ALLOWED_OLLAMA_MODELS,
    });
  };

  const [shinkaiNodeOptionsForUI, setShinkaiNodeOptionsForUI] =
    useState<Partial<ShinkaiNodeOptions>>();

  useEffect(() => {
    const filteredShinkaiNodeOptionsKeys: (keyof ShinkaiNodeOptions)[] = [
      'secret_desktop_installation_proof_key',
    ];
    setShinkaiNodeOptionsForUI(
      Object.fromEntries(
        Object.entries(shinkaiNodeOptions ?? {}).filter(
          ([key]) =>
            !filteredShinkaiNodeOptionsKeys.includes(
              key as keyof ShinkaiNodeOptions,
            ),
        ),
      ) as Partial<ShinkaiNodeOptions>,
    );
  }, [shinkaiNodeOptions]);

  return (
    <div className="flex h-screen w-full flex-col space-y-2">
      <div
        className="absolute top-0 z-50 h-6 w-full"
        data-tauri-drag-region={true}
      />
      <div className="flex flex-row items-center p-4">
        <img alt="shinkai logo" className="h-10 w-10" src={logo} />
        <div className="ml-4 flex flex-col">
          <span className="text-lg">Local Shinkai Node</span>
          <span className="text-text-secondary text-sm">{`API URL: http://${shinkaiNodeOptions?.node_api_ip}:${shinkaiNodeOptions?.node_api_port}`}</span>
        </div>
        <div className="flex grow flex-row items-center justify-end space-x-4">
          <Tooltip>
            <TooltipTrigger>
              <Button
                disabled={
                  shinkaiNodeSpawnIsPending ||
                  shinkaiNodeKillIsPending ||
                  shinkaiNodeIsRunning
                }
                onClick={() => {
                  console.log('spawning');
                  void shinkaiNodeSpawn();
                }}
                variant={'outline'}
                size={'icon'}
              >
                {shinkaiNodeSpawnIsPending || shinkaiNodeKillIsPending ? (
                  <Loader2 className="size-4 animate-spin" />
                ) : (
                  <PlayIcon className="size-4" />
                )}
              </Button>
            </TooltipTrigger>
            <TooltipPortal>
              <TooltipContent side="bottom">
                <p>Start Shinkai Node</p>
              </TooltipContent>
            </TooltipPortal>
          </Tooltip>

          <Tooltip>
            <TooltipTrigger>
              <Button
                disabled={
                  shinkaiNodeSpawnIsPending ||
                  shinkaiNodeKillIsPending ||
                  !shinkaiNodeIsRunning
                }
                onClick={() => shinkaiNodeKill()}
                variant={'outline'}
                size={'icon'}
              >
                {shinkaiNodeKillIsPending ? (
                  <Loader2 className="size-4 animate-spin" />
                ) : (
                  <StopIcon className="size-4" />
                )}
              </Button>
            </TooltipTrigger>
            <TooltipPortal>
              <TooltipContent side="bottom">
                <p>Stop Shinkai Node</p>
              </TooltipContent>
            </TooltipPortal>
          </Tooltip>

          <Tooltip>
            <TooltipTrigger>
              <Button
                disabled={shinkaiNodeIsRunning}
                onClick={() => setIsConfirmResetDialogOpened(true)}
                variant={'outline'}
                size={'icon'}
              >
                {shinkaiNodeRemoveStorageIsPending ? (
                  <Loader2 className="size-4 animate-spin" />
                ) : (
                  <Trash2 className="size-4" />
                )}
              </Button>
            </TooltipTrigger>
            <TooltipPortal>
              <TooltipContent side="bottom">
                <p>Reset Shinkai Node</p>
              </TooltipContent>
            </TooltipPortal>
          </Tooltip>

          <Tooltip>
            <TooltipTrigger>
              <Button
                disabled={!shinkaiNodeIsRunning}
                onClick={() => startSyncOllamaModels()}
                variant={'outline'}
                size={'icon'}
              >
                {syncOllamaModelsIsPending ? (
                  <Loader2 className="size-4 animate-spin" />
                ) : (
                  <RefreshCcwIcon className="size-4" />
                )}
              </Button>
            </TooltipTrigger>
            <TooltipPortal>
              <TooltipContent side="bottom">
                <p>Sync Ollama Models</p>
              </TooltipContent>
            </TooltipPortal>
          </Tooltip>
        </div>
      </div>

      <Tabs
        className="mt-4 flex h-full w-full flex-col overflow-hidden p-4"
        defaultValue="app-logs"
      >
        <TabsList className="flex h-10 w-fit items-center gap-2 rounded-full bg-transparent px-1 py-1">
          <TabsTrigger
            className={cn(
              'flex flex-col rounded-full px-4 py-1.5 text-base font-medium transition-colors',
              'data-[state=active]:bg-bg-quaternary data-[state=active]:text-text-default',
              'data-[state=inactive]:text-text-tertiary data-[state=inactive]:bg-transparent',
              'focus-visible:outline-hidden',
            )}
            value="app-logs"
          >
            App Logs
          </TabsTrigger>
          <TabsTrigger
            className={cn(
              'flex flex-col rounded-full px-4 py-1.5 text-base font-medium transition-colors',
              'data-[state=active]:bg-bg-quaternary data-[state=active]:text-text-default',
              'data-[state=inactive]:text-text-tertiary data-[state=inactive]:bg-transparent',
              'focus-visible:outline-hidden',
            )}
            value="options"
          >
            Options
          </TabsTrigger>
          <TabsTrigger
            className={cn(
              'flex flex-col rounded-full px-4 py-1.5 text-base font-medium transition-colors',
              'data-[state=active]:bg-bg-quaternary data-[state=active]:text-text-default',
              'data-[state=inactive]:text-text-tertiary data-[state=inactive]:bg-transparent',
              'focus-visible:outline-hidden',
            )}
            value="models"
          >
            Models
          </TabsTrigger>
        </TabsList>
        <TabsContent className="h-full overflow-hidden" value="app-logs">
          <Logs />
        </TabsContent>
        <TabsContent className="h-full overflow-hidden" value="options">
          <ScrollArea className="flex h-full flex-1 flex-col overflow-auto [&>div>div]:!block">
            <div className="flex flex-row justify-end pr-4">
              <Button
                className=""
                disabled={shinkaiNodeIsRunning}
                onClick={() => shinkaiNodeSetDefaultOptions()}
                variant={'outline'}
                size={'sm'}
              >
                Restore default
              </Button>
            </div>
            <div className="mt-2 h-full [&>div>div]:!block">
              <Form {...shinkaiNodeOptionsForm}>
                <form className="space-y-2 pr-4">
                  {shinkaiNodeOptionsForUI &&
                    Object.entries(shinkaiNodeOptionsForUI).map(
                      ([key, value]) => {
                        return (
                          <FormField
                            control={shinkaiNodeOptionsForm.control}
                            defaultValue={value}
                            disabled={shinkaiNodeIsRunning}
                            key={key}
                            name={key as keyof ShinkaiNodeOptions}
                            render={({ field }) => (
                              <TextField
                                field={field}
                                label={<span className="uppercase">{key}</span>}
                              />
                            )}
                          />
                        );
                      },
                    )}
                </form>
              </Form>
            </div>
          </ScrollArea>
        </TabsContent>

        <TabsContent className="h-full overflow-hidden pb-2" value="models">
          <OllamaModels />
        </TabsContent>
      </Tabs>
      <AlertDialog
        onOpenChange={setIsConfirmResetDialogOpened}
        open={isConfirmResetDialogOpened}
      >
        <AlertDialogContent className="w-[75%]">
          <AlertDialogHeader>
            <AlertDialogTitle>
              {t('shinkaiNode.resetConfirmDialog.title')}
            </AlertDialogTitle>
            <AlertDialogDescription>
              <div className="flex flex-col space-y-3 text-left text-white/70">
                <div className="flex flex-col space-y-1">
                  <span className="text-sm">
                    {t('shinkaiNode.resetConfirmDialog.description')}
                  </span>
                </div>
              </div>
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter className="mt-4 flex justify-end gap-1">
            <AlertDialogCancel
              className="mt-0 min-w-[120px]"
              onClick={() => {
                setIsConfirmResetDialogOpened(false);
              }}
            >
              {t('shinkaiNode.resetConfirmDialog.cancel')}
            </AlertDialogCancel>
            <AlertDialogAction
              className="min-w-[120px]"
              onClick={() => handleReset()}
            >
              {t('shinkaiNode.resetConfirmDialog.confirm')}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
};

ReactDOM.createRoot(document.querySelector('#root') as HTMLElement).render(
  <QueryClientProvider client={shinkaiNodeQueryClient}>
    <React.StrictMode>
      <TooltipProvider>
        <App />
        <Toaster />
      </TooltipProvider>
    </React.StrictMode>
  </QueryClientProvider>,
);
