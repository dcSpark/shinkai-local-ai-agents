import { zodResolver } from '@hookform/resolvers/zod';
import {
  type LocaleMode,
  localeOptions,
  useTranslation,
} from '@shinkai_network/shinkai-i18n';
import { isShinkaiIdentityLocalhost } from '@shinkai_network/shinkai-message-ts/utils/inbox_name_handler';
import { useSetMaxChatIterations } from '@shinkai_network/shinkai-node-state/v2/mutations/setMaxChatIterations/useSetMaxChatIterations';
import { useStartEmbeddingMigration } from '@shinkai_network/shinkai-node-state/v2/mutations/startEmbeddingMigration/useStartEmbeddingMigration';
import { useUpdateNodeName } from '@shinkai_network/shinkai-node-state/v2/mutations/updateNodeName/useUpdateNodeName';
import { useGetEmbeddingMigrationStatus } from '@shinkai_network/shinkai-node-state/v2/queries/getEmbeddingMigrationStatus/useGetEmbeddingMigrationStatus';
import { useGetHealth } from '@shinkai_network/shinkai-node-state/v2/queries/getHealth/useGetHealth';
import { useGetLLMProviders } from '@shinkai_network/shinkai-node-state/v2/queries/getLLMProviders/useGetLLMProviders';
import { useGetPreferences } from '@shinkai_network/shinkai-node-state/v2/queries/getPreferences/useGetPreferences';
import { useGetShinkaiFreeModelQuota } from '@shinkai_network/shinkai-node-state/v2/queries/getShinkaiFreeModelQuota/useGetShinkaiFreeModelQuota';
import { useScanOllamaModels } from '@shinkai_network/shinkai-node-state/v2/queries/scanOllamaModels/useScanOllamaModels';
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
  Card,
  CardContent,
  CardFooter,
  Form,
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
  Progress,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Switch,
  TextField,
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@shinkai_network/shinkai-ui';
import { useDebounce } from '@shinkai_network/shinkai-ui/hooks';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import { getVersion } from '@tauri-apps/api/app';
import { formatDuration, intervalToDuration } from 'date-fns';
import { motion } from 'framer-motion';
import {
  ExternalLinkIcon,
  InfoIcon,
  ShieldCheck,
  RefreshCw,
  CheckCircle,
} from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { useForm, useWatch } from 'react-hook-form';
import { toast } from 'sonner';
import { z } from 'zod';

import { FeedbackModal } from '../components/feedback/feedback-modal';
import { OnboardingStep } from '../components/onboarding/constants';
import {
  useShinkaiNodeGetOllamaVersionQuery,
  useShinkaiNodeRespawnMutation,
} from '../lib/shinkai-node-manager/shinkai-node-manager-client';
import { isHostingShinkaiNode } from '../lib/shinkai-node-manager/shinkai-node-manager-windows-utils';
import {
  useCheckUpdateQuery,
  useDownloadUpdateMutation,
  useUpdateStateQuery,
} from '../lib/updater/updater-client';
import { type Auth, useAuth } from '../store/auth';
import { useSettings } from '../store/settings';
import { useShinkaiNodeManager } from '../store/shinkai-node-manager';
import { SimpleLayout } from './layout/simple-layout';

const formSchema = z.object({
  defaultAgentId: z.string(),
  displayActionButton: z.boolean(),
  nodeAddress: z.string(),
  shinkaiIdentity: z.string(),
  nodeVersion: z.string(),
  ollamaVersion: z.string(),
  optInAnalytics: z.boolean(),
  optInExperimental: z.boolean(),
  language: z.string(),
  maxChatIterations: z.number(),
  chatFontSize: z.enum(['xs', 'sm', 'base', 'lg']),
});

type FormSchemaType = z.infer<typeof formSchema>;

const MotionButton = motion(Button);

const SettingsPage = () => {
  const { t } = useTranslation();
  const auth = useAuth((authStore) => authStore.auth);
  const isLocalShinkaiNodeInUse = useShinkaiNodeManager(
    (state) => state.isInUse,
  );
  const userLanguage = useSettings((state) => state.userLanguage);
  const setUserLanguage = useSettings((state) => state.setUserLanguage);
  const optInAnalytics = useSettings((state) =>
    state.getStepChoice(OnboardingStep.ANALYTICS),
  );
  const optInExperimental = useSettings((state) => state.optInExperimental);
  const setOptInExperimental = useSettings(
    (state) => state.setOptInExperimental,
  );
  const setEmbeddingModelMismatchPromptDismissed = useSettings(
    (state) => state.setEmbeddingModelMismatchPromptDismissed,
  );

  const setAuth = useAuth((authStore) => authStore.setAuth);

  const defaultAgentId = useSettings(
    (settingsStore) => settingsStore.defaultAgentId,
  );
  const setDefaultAgentId = useSettings(
    (settingsStore) => settingsStore.setDefaultAgentId,
  );

  const [selectedEmbeddingModel, setSelectedEmbeddingModel] = useState('');
  const [showMigrationConfirmation, setShowMigrationConfirmation] = useState(false);
  const [pendingEmbeddingModel, setPendingEmbeddingModel] = useState('');

  const { nodeInfo, isSuccess: isNodeInfoSuccess } = useGetHealth({
    nodeAddress: auth?.node_address ?? '',
  });

  const { mutateAsync: setMaxChatIterationsMutation } = useSetMaxChatIterations(
    {
      onSuccess: (_data) => {
        toast.success(t('settings.maxChatIterations.success'));
      },
      onError: (error) => {
        toast.error(t('settings.maxChatIterations.error'), {
          description: error?.message,
        });
        form.setValue('maxChatIterations', preferences?.max_iterations ?? 10);
      },
    },
  );
  const { data: preferences } = useGetPreferences({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
  });
  const [appVersion, setAppVersion] = useState('');

  const form = useForm<FormSchemaType>({
    resolver: zodResolver(formSchema),
    defaultValues: {
      defaultAgentId: defaultAgentId,
      nodeAddress: auth?.node_address,
      shinkaiIdentity: auth?.shinkai_identity,
      ollamaVersion: '',
      optInAnalytics: !!optInAnalytics,
      optInExperimental,
      language: userLanguage,
    },
  });

  const currentDefaultAgentId = useWatch({
    control: form.control,
    name: 'defaultAgentId',
  });

  const currentOptInExperimental = useWatch({
    control: form.control,
    name: 'optInExperimental',
  });
  const currentLanguage = useWatch({
    control: form.control,
    name: 'language',
  });

  const currentMaxChatIterations = useWatch({
    control: form.control,
    name: 'maxChatIterations',
  });

  const debouncedMaxChatIterations = useDebounce(
    currentMaxChatIterations?.toString() ?? '',
    1000,
  );

  useEffect(() => {
    void (async () => {
      setAppVersion(await getVersion());
    })();
  }, []);

  useEffect(() => {
    setUserLanguage(currentLanguage as LocaleMode);
  }, [currentLanguage, setUserLanguage]);

  useEffect(() => {
    setOptInExperimental(currentOptInExperimental);
  }, [currentOptInExperimental, setOptInExperimental]);

  useEffect(() => {
    if (preferences) {
      form.setValue('maxChatIterations', preferences.max_iterations ?? 10);
    }
  }, [preferences, form]);

  useEffect(() => {
    if (!preferences) return;
    const currentFormValue = form.getValues('maxChatIterations');
    const currentBackendValue = preferences.max_iterations ?? 10;
    if (currentFormValue === currentBackendValue) return; // Avoid unnecessary update at startup
    const newMaxIterations = parseInt(debouncedMaxChatIterations, 10);
    if (!debouncedMaxChatIterations || isNaN(newMaxIterations)) return;

    if (newMaxIterations !== currentBackendValue) {
      void setMaxChatIterationsMutation({
        nodeAddress: auth?.node_address ?? '',
        token: auth?.api_v2_key ?? '',
        maxIterations: newMaxIterations,
      });
    }
  }, [
    debouncedMaxChatIterations,
    preferences,
    auth,
    setMaxChatIterationsMutation,
    form,
  ]);

  const { llmProviders } = useGetLLMProviders({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
  });

  const { data: ollamaVersion } = useShinkaiNodeGetOllamaVersionQuery();
  useEffect(() => {
    form.setValue('ollamaVersion', ollamaVersion ?? '');
  }, [ollamaVersion, form]);

  const { data: updateState } = useUpdateStateQuery();
  const { refetch: checkForUpdates, isFetching: isCheckingUpdates } =
    useCheckUpdateQuery({
      enabled: false,
    });
  const { mutateAsync: downloadUpdate, isPending: isDownloadingUpdate } =
    useDownloadUpdateMutation({
      onSuccess: () => {
        toast.success(
          'Update downloaded successfully! The app will restart now.',
        );
      },
      onError: (error) => {
        toast.error('Failed to download update', {
          description: error?.message,
        });
      },
    });

  const { data: shinkaiFreeModelQuota } = useGetShinkaiFreeModelQuota(
    { nodeAddress: auth?.node_address ?? '', token: auth?.api_v2_key ?? '' },
    { enabled: !!auth },
  );

  // Supported embedding models - memoized to prevent re-renders
  const supportedEmbeddingModels = useMemo(() => [
    'snowflake-arctic-embed:xs',
    'embeddinggemma:300m',
    'jina/jina-embeddings-v2-base-es:latest'
  ], []);

  const { data: availableOllamaModels } = useScanOllamaModels(
    { nodeAddress: auth?.node_address ?? '', token: auth?.api_v2_key ?? '' },
    { enabled: !!auth }
  );

  const { data: embeddingMigrationStatus, refetch: refetchMigrationStatus } = useGetEmbeddingMigrationStatus(
    { nodeAddress: auth?.node_address ?? '', token: auth?.api_v2_key ?? '' },
    { 
      enabled: !!auth,
      // Don't auto-refetch in settings - let the global hook handle polling
      refetchInterval: false
    }
  );

  const { mutateAsync: startEmbeddingMigration, isPending: isMigratingEmbedding } = useStartEmbeddingMigration({
    onSuccess: () => {
      // Toast will be managed by the global hook
    },
    onError: (error) => {
      toast.error('Failed to start embedding migration', {
        description: error.message,
      });
    },
  });

  // Filter available models to only show supported embedding models
  const availableEmbeddingModels = useMemo(() => {
    if (!availableOllamaModels) return [];
    
    return availableOllamaModels
      .filter(model => supportedEmbeddingModels.includes(model.model))
      .map(model => ({
        value: model.model,
        label: model.model
      }));
  }, [availableOllamaModels, supportedEmbeddingModels]);

  // Set initial embedding model from status
  useEffect(() => {
    if (embeddingMigrationStatus?.current_embedding_model && !selectedEmbeddingModel) {
      setSelectedEmbeddingModel(embeddingMigrationStatus.current_embedding_model);
    }
  }, [embeddingMigrationStatus, selectedEmbeddingModel]);

  // Handle embedding model change - show confirmation first
  const handleEmbeddingModelChange = (newModel: string) => {
    // Don't proceed if model is empty/invalid or same as current
    if (!newModel || newModel === selectedEmbeddingModel) return;
    
    // Don't proceed if auth is not available
    if (!auth?.node_address || !auth?.api_v2_key) return;

    // Store the pending model and show confirmation
    setPendingEmbeddingModel(newModel);
    setShowMigrationConfirmation(true);
  };

  // Actually perform the migration after confirmation
  const confirmMigration = async () => {
    try {
      await startEmbeddingMigration({
        nodeAddress: auth!.node_address,
        token: auth!.api_v2_key,
        force: true,
        embedding_model: pendingEmbeddingModel,
      });
      
      // Update the selected model
      setSelectedEmbeddingModel(pendingEmbeddingModel);
      
      // Mark the prompt as dismissed since user took action
      setEmbeddingModelMismatchPromptDismissed(true);
      
      // Trigger immediate status refetch to ensure global hook detects the migration
      setTimeout(() => {
        void refetchMigrationStatus();
      }, 500); // Small delay to allow backend to update
      
      setShowMigrationConfirmation(false);
      setPendingEmbeddingModel('');
    } catch (error) {
      console.error('Failed to change embedding model:', error);
      setShowMigrationConfirmation(false);
      setPendingEmbeddingModel('');
    }
  };

  // Cancel migration
  const cancelMigration = () => {
    setShowMigrationConfirmation(false);
    setPendingEmbeddingModel('');
  };

  const { mutateAsync: respawnShinkaiNode } = useShinkaiNodeRespawnMutation();
  const { mutateAsync: updateNodeName, isPending: isUpdateNodeNamePending } =
    useUpdateNodeName({
      onSuccess: async () => {
        toast.success(t('settings.shinkaiIdentity.success'));
        if (!auth) return;
        const newAuth: Auth = { ...auth };
        setAuth({
          ...newAuth,
          shinkai_identity: currentShinkaiIdentity,
        });
        if (isLocalShinkaiNodeInUse) {
          await respawnShinkaiNode();
        } else if (!isHostingShinkaiNode(auth.node_address)) {
          toast.info(t('shinkaiNode.restartNode'));
        }
      },
      onError: (error) => {
        toast.error(t('settings.shinkaiIdentity.error'), {
          description: error?.response?.data?.error
            ? error?.response?.data?.error +
              ': ' +
              error?.response?.data?.message
            : error.message,
        });
      },
    });

  useEffect(() => {
    if (isNodeInfoSuccess) {
      form.setValue('nodeVersion', nodeInfo?.version ?? '');
      form.setValue('shinkaiIdentity', nodeInfo?.node_name ?? '');
    }
  }, [form, isNodeInfoSuccess, nodeInfo?.node_name, nodeInfo?.version]);

  const currentShinkaiIdentity = useWatch({
    control: form.control,
    name: 'shinkaiIdentity',
  });
  const handleUpdateNodeName = async () => {
    if (!auth) return;
    await updateNodeName({
      nodeAddress: auth?.node_address ?? '',
      token: auth?.api_v2_key ?? '',
      newNodeName: form.getValues().shinkaiIdentity,
    });
  };

  useEffect(() => {
    setDefaultAgentId(currentDefaultAgentId);
  }, [currentDefaultAgentId, setDefaultAgentId]);

  const isIdentityLocalhost = isShinkaiIdentityLocalhost(
    auth?.shinkai_identity ?? '',
  );

  return (
    <SimpleLayout classname="max-w-2xl" title={t('settings.layout.general')}>
      <div className="mb-6 flex items-center justify-between">
        <p>{t('settings.description')}</p>
        <FeedbackModal />
      </div>
      <div className="flex flex-col space-y-8 pr-2.5">
        <div className="flex flex-col space-y-8">
          {shinkaiFreeModelQuota && (
            <div className="bg-bg-secondary space-y-4 rounded-lg p-4">
              <div>
                <h2 className="text-text-default text-base font-semibold">
                  Usage
                </h2>
                <p className="text-text-secondary text-sm">
                  Monitor your AI usage
                </p>
              </div>
              <Card className="w-full border-none p-0">
                <CardContent className="space-y-2 p-0 py-2">
                  <div className="flex justify-between text-sm">
                    <h3 className="text-base font-semibold">
                      Free Shinkai AI Usage
                    </h3>

                    <div className="flex items-center gap-1">
                      <span>Total tokens used: </span>
                      <span className="text-text-default text-xs">
                        {shinkaiFreeModelQuota.usedTokens.toLocaleString()}{' '}
                        <span className="text-text-tertiary text-xs">
                          / {shinkaiFreeModelQuota.tokensQuota.toLocaleString()}
                        </span>
                      </span>
                      <Tooltip>
                        <TooltipTrigger>
                          <InfoIcon className="size-3 text-current" />
                        </TooltipTrigger>
                        <TooltipContent>
                          <p>
                            A token is a chunk of text — it can be a word, part
                            of a word, or even punctuation. AI processes text in
                            tokens, and usage is measured by how many tokens are
                            used. <br /> <br /> Based on your current usage, you
                            have approximately{' '}
                            {Math.floor(
                              (shinkaiFreeModelQuota.tokensQuota -
                                shinkaiFreeModelQuota.usedTokens) /
                                2,
                            )}{' '}
                            messages remaining.
                          </p>
                        </TooltipContent>
                      </Tooltip>
                    </div>
                  </div>
                  <Progress
                    className="h-2 rounded-full bg-cyan-900 [&>div]:bg-cyan-400"
                    max={100}
                    value={
                      shinkaiFreeModelQuota?.tokensQuota
                        ? Math.min(
                            100,
                            (shinkaiFreeModelQuota.usedTokens /
                              shinkaiFreeModelQuota.tokensQuota) *
                              100,
                          )
                        : 0
                    }
                  />
                </CardContent>

                <CardFooter className="p-0">
                  <span className="text-text-default text-xs">
                    Your free limit resets in{' '}
                    {formatDuration(
                      intervalToDuration({
                        start: 0,
                        end: shinkaiFreeModelQuota?.resetTime * 60 * 1000,
                      }),
                    )}
                  </span>
                </CardFooter>
              </Card>
            </div>
          )}

          <div className="bg-bg-secondary space-y-4 rounded-lg p-4">
            <div>
              <h2 className="text-text-default text-base font-semibold">
                Preferences
              </h2>
              <p className="text-text-secondary text-sm">
                Customize language, AI models, and application behavior
              </p>
            </div>
            <Form {...form}>
              <form className="flex grow flex-col justify-between space-y-6 overflow-hidden">
                <FormField
                  control={form.control}
                  name="language"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>{t('settings.language.label')}</FormLabel>
                      <Select
                        defaultValue={field.value}
                        onValueChange={field.onChange}
                      >
                        <FormControl>
                          <SelectTrigger>
                            <SelectValue
                              placeholder={t(
                                'settings.language.selectLanguage',
                              )}
                            />
                          </SelectTrigger>
                        </FormControl>
                        <SelectContent>
                          {[
                            {
                              label: 'Automatic',
                              value: 'auto',
                            },
                            ...localeOptions,
                          ].map((locale) => (
                            <SelectItem key={locale.value} value={locale.value}>
                              {locale.label}
                            </SelectItem>
                          ))}
                        </SelectContent>
                      </Select>
                    </FormItem>
                  )}
                />
                <FormItem>
                  <Select
                    defaultValue={defaultAgentId}
                    name="defaultAgentId"
                    onValueChange={(value) => {
                      form.setValue('defaultAgentId', value);
                    }}
                    value={
                      llmProviders?.find((agent) => agent.id === defaultAgentId)
                        ?.id
                    }
                  >
                    <FormControl>
                      <SelectTrigger>
                        <SelectValue />
                      </SelectTrigger>
                    </FormControl>
                    <FormLabel>{t('settings.defaultAgent')}</FormLabel>
                    <SelectContent>
                      {llmProviders?.map((llmProvider) => (
                        <SelectItem key={llmProvider.id} value={llmProvider.id}>
                          {llmProvider.name || llmProvider.id}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>

                  <FormMessage />
                </FormItem>
                <FormItem>
                  <FormLabel>Embeddings Model</FormLabel>
                  <Select
                    disabled={isMigratingEmbedding || embeddingMigrationStatus?.migration_in_progress}
                    onValueChange={handleEmbeddingModelChange}
                    value={selectedEmbeddingModel}
                  >
                    <FormControl>
                      <SelectTrigger>
                        <SelectValue placeholder="Select embedding model" />
                      </SelectTrigger>
                    </FormControl>
                    <SelectContent>
                      {availableEmbeddingModels.map((model) => (
                        <SelectItem key={model.value} value={model.value}>
                          {model.label}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                  <FormMessage />
                </FormItem>
              </form>
            </Form>
          </div>

          <div className="bg-bg-secondary space-y-4 rounded-lg p-4">
            <div>
              <h2 className="text-text-default text-base font-semibold">
                Shinkai Node Configuration
              </h2>
              <p className="text-text-secondary text-sm">
                Configure your Shinkai node connection and identity settings
              </p>
            </div>
            <Form {...form}>
              <form className="flex grow flex-col justify-between space-y-6 overflow-hidden">
                <div className="divide-divider flex flex-col divide-y">
                  {[
                    {
                      label: t('shinkaiNode.nodeAddress'),
                      value: auth?.node_address,
                    },
                    {
                      label: t('shinkaiNode.nodeVersion'),
                      value: nodeInfo?.version,
                    },
                    {
                      label: t('ollama.version'),
                      value: ollamaVersion ?? '-',
                    },
                  ].map((item) => (
                    <div
                      key={item.label}
                      className="flex items-center justify-between gap-1 py-3"
                    >
                      <span className="text-text-secondary text-sm">
                        {item.label}
                      </span>
                      <span className="text-text-default font-mono text-sm">
                        {item.value}
                      </span>
                    </div>
                  ))}
                </div>

                <div className="space-y-1">
                  <FormField
                    control={form.control}
                    name="shinkaiIdentity"
                    render={({ field }) => (
                      <TextField
                        field={{
                          ...field,
                          onKeyDown: (event) => {
                            if (
                              currentShinkaiIdentity === auth?.shinkai_identity
                            )
                              return;
                            if (event.key === 'Enter') {
                              void handleUpdateNodeName();
                            }
                          },
                        }}
                        helperMessage={
                          <span className="flex items-center justify-start gap-3">
                            <span className="text-text-secondary hover:text-text-default inline-flex items-center gap-1 px-1 py-2.5">
                              {isIdentityLocalhost ? (
                                <a
                                  className={cn(
                                    buttonVariants({
                                      size: 'auto',
                                      variant: 'link',
                                    }),
                                    'rounded-lg p-0 text-xs text-inherit underline',
                                  )}
                                  href={`https://shinkai-contracts.pages.dev?encryption_pk=${auth?.encryption_pk}&signature_pk=${auth?.identity_pk}&node_address=${auth?.node_address}`}
                                  rel="noreferrer"
                                  target="_blank"
                                >
                                  {t(
                                    'settings.shinkaiIdentity.registerIdentity',
                                  )}
                                </a>
                              ) : (
                                <a
                                  className={cn(
                                    buttonVariants({
                                      size: 'auto',
                                      variant: 'link',
                                    }),
                                    'rounded-lg p-0 text-xs text-inherit underline',
                                  )}
                                  href={`https://shinkai-contracts.pages.dev/identity/${auth?.shinkai_identity?.replace(
                                    '@@',
                                    '',
                                  )}`}
                                  rel="noreferrer"
                                  target="_blank"
                                >
                                  {t(
                                    'settings.shinkaiIdentity.goToShinkaiIdentity',
                                  )}
                                </a>
                              )}
                              <ExternalLinkIcon className="h-4 w-4" />
                            </span>
                            <a
                              className={cn(
                                buttonVariants({
                                  size: 'auto',
                                  variant: 'link',
                                }),
                                'text-text-secondary hover:text-text-default rounded-lg p-0 text-xs underline',
                              )}
                              href="https://docs.shinkai.com/advanced/shinkai-identity-troubleshooting"
                              rel="noreferrer"
                              target="_blank"
                            >
                              {t(
                                'settings.shinkaiIdentity.troubleRegisterIdentity',
                              )}
                            </a>
                          </span>
                        }
                        label={t('settings.shinkaiIdentity.label')}
                      />
                    )}
                  />
                  {currentShinkaiIdentity !== auth?.shinkai_identity && (
                    <div className="space-y-1.5">
                      <p className="text-text-tertiary flex items-center gap-1 text-xs">
                        <InfoIcon className="size-3" />
                        {t('settings.shinkaiIdentity.saveWillRestartApp')}
                      </p>
                      <div className="flex items-center gap-3">
                        <MotionButton
                          className="h-10 min-w-[100px] rounded-lg text-sm"
                          isLoading={isUpdateNodeNamePending}
                          layout
                          onClick={handleUpdateNodeName}
                          size="auto"
                          type="button"
                        >
                          {t('common.save')}
                        </MotionButton>
                        <Button
                          className="h-10 min-w-10 rounded-lg text-sm"
                          onClick={() => {
                            form.setValue(
                              'shinkaiIdentity',
                              auth?.shinkai_identity ?? '',
                            );
                          }}
                          type="button"
                          variant="outline"
                        >
                          {t('common.cancel')}
                        </Button>
                      </div>
                    </div>
                  )}

                  {!isIdentityLocalhost && (
                    <a
                      className={cn(
                        buttonVariants({
                          size: 'auto',
                          variant: 'tertiary',
                        }),
                        'flex cursor-pointer items-start justify-start gap-2 rounded-lg text-xs',
                      )}
                      href={`https://shinkai-contracts.pages.dev/identity/${auth?.shinkai_identity?.replace(
                        '@@',
                        '',
                      )}?encryption_pk=${auth?.encryption_pk}&signature_pk=${auth?.identity_pk}`}
                      rel="noreferrer"
                      target="_blank"
                    >
                      <ShieldCheck className="h-5 w-5" />
                      <span className="flex flex-col gap-0.5">
                        <span className="capitalize">
                          {t('settings.shinkaiIdentity.checkIdentityInSync')}
                        </span>
                        <span className="text-text-tertiary">
                          {t(
                            'settings.shinkaiIdentity.checkIdentityInSyncDescription',
                          )}
                        </span>
                      </span>
                    </a>
                  )}
                </div>
              </form>
            </Form>
          </div>

          <div className="bg-bg-secondary space-y-4 rounded-lg p-4">
            <div>
              <h2 className="text-text-default text-base font-semibold">
                Advanced Settings
              </h2>
              <p className="text-text-secondary text-sm">
                Configure advanced features and experimental options
              </p>
            </div>
            <Form {...form}>
              <form className="flex grow flex-col justify-between space-y-6 overflow-hidden">
                <FormField
                  control={form.control}
                  name="maxChatIterations"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>
                        {t('settings.maxChatIterations.label')}
                      </FormLabel>
                      <FormControl>
                        <TextField
                          field={{ ...field, value: field.value ?? '' }}
                          helperMessage={t(
                            'settings.maxChatIterations.description',
                          )}
                          label={t('settings.maxChatIterations.label')}
                          max={100}
                          min={1}
                          type="number"
                        />
                      </FormControl>
                      <FormMessage />
                    </FormItem>
                  )}
                />
                <FormField
                  control={form.control}
                  name="optInExperimental"
                  render={({ field }) => (
                    <FormItem className="flex gap-2.5">
                      <FormControl>
                        <Switch
                          aria-readonly
                          checked={field.value}
                          onCheckedChange={field.onChange}
                          className="data-[state=unchecked]:bg-gray-400"
                        />
                      </FormControl>
                      <div className="space-y-1 leading-none">
                        <FormLabel className="text-text-default static space-y-1.5 text-sm">
                          {t('settings.experimentalFeature.label')}
                        </FormLabel>
                      </div>
                    </FormItem>
                  )}
                />
              </form>
            </Form>
          </div>

          <div className="bg-bg-secondary mb-10 space-y-4 rounded-lg p-4">
            <div className="flex items-center justify-between gap-3">
              <div className="flex flex-col gap-1">
                <h2 className="text-text-default flex items-center gap-2 text-base font-semibold">
                  Updates
                </h2>

                <p className="text-text-secondary text-sm">
                  Manage application updates and version information.
                </p>
              </div>
              <Button
                disabled={isCheckingUpdates}
                onClick={() => checkForUpdates()}
                size="sm"
                variant="outline"
                className="w-full sm:w-auto"
              >
                {isCheckingUpdates ? (
                  <>
                    <RefreshCw className="mr-2 h-4 w-4 animate-spin" />
                    Checking...
                  </>
                ) : (
                  <>
                    <RefreshCw className="mr-2 h-4 w-4" />
                    Check for Updates
                  </>
                )}
              </Button>
            </div>

            <div className="divide-divider flex flex-col gap-1 divide-y text-sm">
              <div className="flex items-center justify-between gap-1 py-3">
                <span className="text-text-secondary text-sm">
                  Current Shinkai App Version
                </span>
                <span className="text-text-default font-mono text-sm">
                  {appVersion}
                </span>
              </div>

              <div className="space-y-5 p-2">
                {updateState?.update?.available && (
                  <div className="flex items-center justify-between gap-2">
                    {updateState?.update?.available && (
                      <Badge className="flex items-center gap-1 rounded-full bg-gray-900 px-2 py-0.5 text-sm font-semibold text-cyan-400">
                        <CheckCircle className="h-3.5 w-3.5" />
                        New version available: {updateState.update.version}
                      </Badge>
                    )}
                    <Button
                      disabled={
                        isDownloadingUpdate ||
                        updateState.state === 'downloading'
                      }
                      onClick={() => downloadUpdate()}
                      size="sm"
                      variant="outline"
                      className="w-full sm:w-auto"
                    >
                      {isDownloadingUpdate ||
                      updateState.state === 'downloading' ? (
                        <>Downloading...</>
                      ) : (
                        <>Install Update</>
                      )}
                    </Button>
                  </div>
                )}

                {updateState?.state === 'downloading' &&
                  updateState.downloadState && (
                    <div className="space-y-2">
                      <div className="flex items-center justify-between text-sm">
                        <span className="flex items-center gap-1">
                          Downloading update...
                        </span>
                        <span>
                          {updateState.downloadState.data
                            ?.downloadProgressPercent || 0}
                          %
                        </span>
                      </div>
                      <Progress
                        className="h-2 w-full rounded-lg bg-cyan-900 [&>div]:bg-cyan-400"
                        value={
                          updateState.downloadState.data
                            ?.downloadProgressPercent || 0
                        }
                      />
                    </div>
                  )}
              </div>
            </div>
          </div>
        </div>
      </div>

      {/* Embedding Migration Confirmation Dialog */}
      <AlertDialog open={showMigrationConfirmation} onOpenChange={setShowMigrationConfirmation}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Confirm Embedding Model Migration</AlertDialogTitle>
            <AlertDialogDescription>
              Are you sure you want to migrate to <strong>{pendingEmbeddingModel}</strong>?
              <br />
              <br />
              This process will:
              <ul className="mt-2 list-disc pl-5 space-y-1">
                <li>Update your embedding model configuration</li>
                <li>Re-process existing vectorized data (this may take some time)</li>
                <li>Temporarily affect search functionality during migration</li>
              </ul>
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter className="gap-3">
            <AlertDialogCancel onClick={cancelMigration}>
              Cancel
            </AlertDialogCancel>
            <AlertDialogAction onClick={confirmMigration}>
              Yes, Migrate Model
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </SimpleLayout>
  );
};

export default SettingsPage;
