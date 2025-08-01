import { zodResolver } from '@hookform/resolvers/zod';
import {
  type LocaleMode,
  localeOptions,
  useTranslation,
} from '@shinkai_network/shinkai-i18n';
import { isShinkaiIdentityLocalhost } from '@shinkai_network/shinkai-message-ts/utils/inbox_name_handler';
import { useSetMaxChatIterations } from '@shinkai_network/shinkai-node-state/v2/mutations/setMaxChatIterations/useSetMaxChatIterations';
import { useUpdateNodeName } from '@shinkai_network/shinkai-node-state/v2/mutations/updateNodeName/useUpdateNodeName';
import { useGetHealth } from '@shinkai_network/shinkai-node-state/v2/queries/getHealth/useGetHealth';
import { useGetLLMProviders } from '@shinkai_network/shinkai-node-state/v2/queries/getLLMProviders/useGetLLMProviders';
import { useGetPreferences } from '@shinkai_network/shinkai-node-state/v2/queries/getPreferences/useGetPreferences';
import { useGetShinkaiFreeModelQuota } from '@shinkai_network/shinkai-node-state/v2/queries/getShinkaiFreeModelQuota/useGetShinkaiFreeModelQuota';
import {
  Button,
  buttonVariants,
  Card,
  CardContent,
  CardFooter,
  CardHeader,
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
import { ExternalLinkIcon, InfoIcon, ShieldCheck } from 'lucide-react';
import { useEffect, useState } from 'react';
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

  const setAuth = useAuth((authStore) => authStore.setAuth);

  const defaultAgentId = useSettings(
    (settingsStore) => settingsStore.defaultAgentId,
  );
  const setDefaultAgentId = useSettings(
    (settingsStore) => settingsStore.setDefaultAgentId,
  );

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

  const { data: shinkaiFreeModelQuota } = useGetShinkaiFreeModelQuota(
    { nodeAddress: auth?.node_address ?? '', token: auth?.api_v2_key ?? '' },
    { enabled: !!auth },
  );

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
    <SimpleLayout classname="max-w-xl" title={t('settings.layout.general')}>
      <div className="mb-6 flex items-center justify-between">
        <p>{t('settings.description')}</p>
        <FeedbackModal />
      </div>
      <div className="flex flex-col space-y-8 pr-2.5">
        <div className="flex flex-col space-y-8">
          {shinkaiFreeModelQuota && (
            <Card className="bg-bg-tertiary w-full">
              <CardHeader className="space-y-1">
                <h3 className="text-base font-semibold">
                  Free Shinkai AI Usage
                </h3>
              </CardHeader>

              <CardContent className="space-y-2">
                <div className="flex justify-between text-sm">
                  <span>Total tokens used</span>
                  <div className="flex items-center gap-1">
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
                          A token is a chunk of text — it can be a word, part of
                          a word, or even punctuation. AI processes text in
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
                  className="h-2 rounded-full"
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

              <CardFooter>
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
          )}

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
                            placeholder={t('settings.language.selectLanguage')}
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

              <FormField
                control={form.control}
                disabled
                name="nodeAddress"
                render={({ field }) => (
                  <TextField
                    field={field}
                    label={t('shinkaiNode.nodeAddress')}
                  />
                )}
              />
              <div className="space-y-1">
                <FormField
                  control={form.control}
                  name="shinkaiIdentity"
                  render={({ field }) => (
                    <TextField
                      field={{
                        ...field,
                        onKeyDown: (event) => {
                          if (currentShinkaiIdentity === auth?.shinkai_identity)
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
                                {t('settings.shinkaiIdentity.registerIdentity')}
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
                        variant: 'ghost',
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
              <FormField
                control={form.control}
                disabled
                name="nodeVersion"
                render={({ field }) => (
                  <TextField
                    field={field}
                    label={t('shinkaiNode.nodeVersion')}
                  />
                )}
              />
              <FormField
                control={form.control}
                disabled
                name="ollamaVersion"
                render={({ field }) => (
                  <TextField field={field} label={t('ollama.version')} />
                )}
              />
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
        <div>
          <p className="text-text-default text-right text-xs">
            {t('settings.shinkaiVersion')}{' '}
            <span className="font-bold">{appVersion}</span>
          </p>
        </div>
      </div>
    </SimpleLayout>
  );
};

export default SettingsPage;
