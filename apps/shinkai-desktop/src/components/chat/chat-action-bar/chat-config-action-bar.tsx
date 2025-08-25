import { zodResolver } from '@hookform/resolvers/zod';
import { PopoverClose } from '@radix-ui/react-popover';
import { useTranslation } from '@shinkai_network/shinkai-i18n';
import { extractJobIdFromInbox } from '@shinkai_network/shinkai-message-ts/utils/inbox_name_handler';
import { useUpdateChatConfig } from '@shinkai_network/shinkai-node-state/v2/mutations/updateChatConfig/useUpdateChatConfig';
import { useGetChatConfig } from '@shinkai_network/shinkai-node-state/v2/queries/getChatConfig/useGetChatConfig';
import { useGetLLMProviders } from '@shinkai_network/shinkai-node-state/v2/queries/getLLMProviders/useGetLLMProviders';
import { useGetProviderFromJob } from '@shinkai_network/shinkai-node-state/v2/queries/getProviderFromJob/useGetProviderFromJob';
import {
  Button,
  Form,
  Popover,
  PopoverContent,
  PopoverTrigger,
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  HoverCard,
  HoverCardContent,
  HoverCardTrigger,
  Label,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Slider,
  Switch,
  Textarea,
} from '@shinkai_network/shinkai-ui';
import { ChatSettingsIcon } from '@shinkai_network/shinkai-ui/assets';

import { cn } from '@shinkai_network/shinkai-ui/utils';
import { memo, useEffect, useMemo } from 'react';
import { useForm, type UseFormReturn } from 'react-hook-form';
import { useParams } from 'react-router';
import { toast } from 'sonner';
import { z } from 'zod';

import { useAuth } from '../../../store/auth';
import { useSettings } from '../../../store/settings';
import {
  getThinkingConfig,
  type ThinkingConfig,
} from '../../../utils/thinking-config';
import { ARTIFACTS_SYSTEM_PROMPT } from '../constants';
import { actionButtonClassnames } from '../conversation-footer';

export const chatConfigFormSchema = z.object({
  stream: z.boolean(),
  useTools: z.boolean(),
  thinking: z.boolean(),
  reasoningEffort: z.enum(['low', 'medium', 'high']).optional(),
  webSearchEnabled: z.boolean().optional(),
  customPrompt: z.string().optional(),
  temperature: z.number(),
  topP: z.number(),
  topK: z.number(),
});

export type ChatConfigFormSchemaType = z.infer<typeof chatConfigFormSchema>;

interface ChatConfigFormProps {
  form: UseFormReturn<ChatConfigFormSchemaType>;
  thinkingConfig?: ThinkingConfig;
}

function ChatConfigForm({ form, thinkingConfig }: ChatConfigFormProps) {
  const optInExperimental = useSettings((state) => state.optInExperimental);

  // Check if thinking is enabled (either forced or manually enabled)
  const isThinkingEnabled =
    thinkingConfig?.forceEnabled || form.watch('thinking');
  const shouldDisableSliders =
    thinkingConfig?.supportsThinking && isThinkingEnabled;

  return (
    <div className="space-y-6">
      <FormField
        control={form.control}
        name="stream"
        render={({ field }) => (
          <FormItem className="flex w-full flex-col gap-3">
            <div className="flex gap-3">
              <FormControl>
                <Switch
                  checked={field.value}
                  onCheckedChange={field.onChange}
                />
              </FormControl>
              <div className="space-y-1 leading-none">
                <FormLabel className="static space-y-1.5 text-xs text-white">
                  Enable Stream
                </FormLabel>
              </div>
            </div>
          </FormItem>
        )}
      />
      <FormField
        control={form.control}
        name="webSearchEnabled"
        render={({ field }) => (
          <FormItem className="flex w-full flex-col gap-3">
            <div className="flex gap-3">
              <FormControl>
                <Switch
                  checked={field.value}
                  onCheckedChange={field.onChange}
                />
              </FormControl>
              <div className="space-y-1 leading-none">
                <FormLabel className="static space-y-1.5 text-xs text-white">
                  Enable Web Search
                </FormLabel>
              </div>
            </div>
          </FormItem>
        )}
      />
      <FormField
        control={form.control}
        name="temperature"
        render={({ field }) => (
          <FormItem className="flex gap-2.5">
            <FormControl>
              <HoverCard openDelay={200}>
                <HoverCardTrigger asChild>
                  <div className="grid w-full gap-1.5">
                    <div className="flex items-center justify-between">
                      <Label
                        className={`text-xs ${shouldDisableSliders ? 'opacity-60' : ''}`}
                        htmlFor="temperature"
                      >
                        Temperature
                        {shouldDisableSliders && (
                          <span className="ml-1 text-xs text-white">
                            (Disabled for thinking models)
                          </span>
                        )}
                      </Label>
                      <span className="text-text-secondary hover:border-border w-12 rounded-md border border-transparent px-2 py-0.5 text-right text-xs">
                        {field.value}
                      </span>
                    </div>
                    <Slider
                      aria-label="Temperature"
                      disabled={shouldDisableSliders}
                      id="temperature"
                      max={1}
                      onValueChange={(vals) => {
                        if (!shouldDisableSliders) {
                          field.onChange(vals[0]);
                        }
                      }}
                      step={0.1}
                      value={[field.value]}
                    />
                  </div>
                </HoverCardTrigger>
                <HoverCardContent
                  align="start"
                  className="w-[300px] bg-gray-600 px-2 py-3 text-xs"
                  side="left"
                >
                  Temperature is a parameter that affects the randomness of AI
                  outputs. Higher temp = more unexpected, lower temp = more
                  predictable.
                </HoverCardContent>
              </HoverCard>
            </FormControl>
          </FormItem>
        )}
      />
      <FormField
        control={form.control}
        name="topP"
        render={({ field }) => (
          <FormItem className="flex gap-2.5">
            <FormControl>
              <HoverCard openDelay={200}>
                <HoverCardTrigger asChild>
                  <div className="grid w-full gap-1.5">
                    <div className="flex items-center justify-between">
                      <Label
                        className={`text-xs ${shouldDisableSliders ? 'opacity-60' : ''}`}
                        htmlFor="topP"
                      >
                        Top P
                        {shouldDisableSliders && (
                          <span className="ml-1 text-xs text-white">
                            (Disabled for thinking models)
                          </span>
                        )}
                      </Label>
                      <span className="text-text-secondary hover:border-border w-12 rounded-md border border-transparent px-2 py-0.5 text-right text-xs">
                        {field.value}
                      </span>
                    </div>
                    <Slider
                      aria-label="Top P"
                      disabled={shouldDisableSliders}
                      id="topP"
                      max={1}
                      min={0}
                      onValueChange={(vals) => {
                        if (!shouldDisableSliders) {
                          field.onChange(vals[0]);
                        }
                      }}
                      step={0.1}
                      value={[field.value]}
                    />
                  </div>
                </HoverCardTrigger>
                <HoverCardContent
                  align="start"
                  className="w-[300px] bg-gray-600 px-2 py-3 text-xs"
                  side="left"
                >
                  Adjust the probability threshold to increase the relevance of
                  results. For example, a threshold of 0.9 could be optimal for
                  targeted, specific applications, whereas a threshold of 0.95
                  or 0.97 might be preferred for tasks that require broader,
                  more creative responses.
                </HoverCardContent>
              </HoverCard>
            </FormControl>
          </FormItem>
        )}
      />
      <FormField
        control={form.control}
        name="topK"
        render={({ field }) => (
          <FormItem className="flex gap-2.5">
            <FormControl>
              <HoverCard openDelay={200}>
                <HoverCardTrigger asChild>
                  <div className="grid w-full gap-1.5">
                    <div className="flex items-center justify-between">
                      <Label
                        className={`text-xs ${shouldDisableSliders ? 'opacity-60' : ''}`}
                        htmlFor="topK"
                      >
                        Top K
                        {shouldDisableSliders && (
                          <span className="ml-1 text-xs text-white">
                            (Disabled for thinking models)
                          </span>
                        )}
                      </Label>
                      <span className="text-text-secondary hover:border-border w-12 rounded-md border border-transparent px-2 py-0.5 text-right text-xs">
                        {field.value}
                      </span>
                    </div>
                    <Slider
                      aria-label="Top K"
                      disabled={shouldDisableSliders}
                      id="topK"
                      max={100}
                      onValueChange={(vals) => {
                        if (!shouldDisableSliders) {
                          field.onChange(vals[0]);
                        }
                      }}
                      step={1}
                      value={[field.value]}
                    />
                  </div>
                </HoverCardTrigger>
                <HoverCardContent
                  align="start"
                  className="w-[300px] bg-gray-600 px-2 py-3 text-xs"
                  side="left"
                >
                  Adjust the count of key words for creating sequences. This
                  parameter governs the extent of the generated passage,
                  forestalling too much repetition. Selecting a higher figure
                  yields longer narratives, whereas a smaller figure keeps the
                  text brief.
                </HoverCardContent>
              </HoverCard>
            </FormControl>
          </FormItem>
        )}
      />
      <FormField
        control={form.control}
        name="customPrompt"
        render={({ field }) => (
          <FormItem>
            <FormLabel>System Prompt</FormLabel>
            <FormControl>
              <Textarea
                className="!min-h-[130px] resize-none text-xs"
                spellCheck={false}
                {...field}
              />
            </FormControl>
          </FormItem>
        )}
      />
      {thinkingConfig?.reasoningLevel && isThinkingEnabled && (
        <FormField
          control={form.control}
          name="reasoningEffort"
          render={({ field }) => (
            <FormItem>
              <FormLabel className="text-xs text-white">
                Reasoning Effort
              </FormLabel>
              <FormControl>
                <Select value={field.value} onValueChange={field.onChange}>
                  <SelectTrigger className="w-full">
                    <SelectValue placeholder="Select reasoning effort" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="low">Low</SelectItem>
                    <SelectItem value="medium">Medium</SelectItem>
                    <SelectItem value="high">High</SelectItem>
                  </SelectContent>
                </Select>
              </FormControl>
            </FormItem>
          )}
        />
      )}
      {optInExperimental && (
        <div className="flex w-full flex-col gap-3">
          <div className="flex gap-3">
            <Switch
              checked={form.watch('customPrompt') === ARTIFACTS_SYSTEM_PROMPT}
              onCheckedChange={(checked) => {
                form.setValue(
                  'customPrompt',
                  checked ? ARTIFACTS_SYSTEM_PROMPT : '',
                );
              }}
            />
            <div className="space-y-1 leading-none">
              <FormLabel className="static space-y-1.5 text-xs text-white">
                Enable UI Artifacts
              </FormLabel>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

export function UpdateChatConfigActionBarBase() {
  const auth = useAuth((state) => state.auth);
  const { inboxId: encodedInboxId = '' } = useParams();
  const inboxId = decodeURIComponent(encodedInboxId);
  const { data: chatConfig } = useGetChatConfig(
    {
      nodeAddress: auth?.node_address ?? '',
      token: auth?.api_v2_key ?? '',
      jobId: inboxId ? extractJobIdFromInbox(inboxId) : '',
    },
    { enabled: !!inboxId },
  );

  const { data: provider } = useGetProviderFromJob({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
    jobId: inboxId ? extractJobIdFromInbox(inboxId) : '',
  });

  const { t } = useTranslation();

  const thinkingConfig = useMemo(() => {
    const modelName = provider?.agent?.model ?? provider?.agent?.id;
    return getThinkingConfig(modelName);
  }, [provider?.agent?.id, provider?.agent?.model]);

  const form = useForm<ChatConfigFormSchemaType>({
    resolver: zodResolver(chatConfigFormSchema),
    defaultValues: {
      stream: chatConfig?.stream,
      customPrompt: chatConfig?.custom_prompt ?? '',
      temperature: chatConfig?.temperature,
      topP: chatConfig?.top_p,
      topK: chatConfig?.top_k,
      useTools: chatConfig?.use_tools,
      thinking: chatConfig?.thinking,
      reasoningEffort: chatConfig?.reasoning_effort,
      webSearchEnabled: chatConfig?.web_search_enabled,
    },
  });

  const { mutateAsync: updateChatConfig } = useUpdateChatConfig({
    onSuccess: () => {
      toast.success('Chat settings updated successfully');
    },
    onError: (error) => {
      toast.error('Chat settings update failed', {
        description: error.response?.data?.message ?? error.message,
      });
    },
  });

  // Auto-enable thinking if force enabled for the model
  useEffect(() => {
    if (thinkingConfig.forceEnabled && chatConfig) {
      form.setValue('thinking', true);
    }
  }, [thinkingConfig.forceEnabled, chatConfig, form]);

  useEffect(() => {
    if (chatConfig) {
      form.reset({
        stream: chatConfig.stream,
        customPrompt: chatConfig.custom_prompt ?? '',
        temperature: chatConfig.temperature,
        topP: chatConfig.top_p,
        topK: chatConfig.top_k,
        useTools: chatConfig.use_tools,
        thinking: chatConfig.thinking,
        reasoningEffort: chatConfig.reasoning_effort,
        webSearchEnabled: chatConfig.web_search_enabled,
      });
    }
  }, [chatConfig, form]);

  const onSubmit = async (data: ChatConfigFormSchemaType) => {
    if (!inboxId) return;
    await updateChatConfig({
      nodeAddress: auth?.node_address ?? '',
      token: auth?.api_v2_key ?? '',
      jobId: extractJobIdFromInbox(inboxId),
      jobConfig: {
        stream: data.stream,
        custom_prompt: data.customPrompt ?? '',
        temperature: data.temperature,
        top_p: data.topP,
        top_k: data.topK,
        use_tools: data.useTools,
        thinking: data.thinking,
        reasoning_effort: data.reasoningEffort,
        web_search_enabled: data.webSearchEnabled,
      },
    });
  };

  return (
    <div className="flex items-center gap-2">
      {/* <ToolsDisabledAlert isToolsDisabled={!form.watch('useTools')} /> */}
      <Popover
        onOpenChange={(open) => {
          if (open) {
            form.reset({
              stream: chatConfig?.stream,
              customPrompt: chatConfig?.custom_prompt ?? '',
              temperature: chatConfig?.temperature,
              topP: chatConfig?.top_p,
              topK: chatConfig?.top_k,
              useTools: chatConfig?.use_tools,
              thinking: chatConfig?.thinking,
              reasoningEffort: chatConfig?.reasoning_effort,
              webSearchEnabled: chatConfig?.web_search_enabled,
            });
          }
        }}
      >
        <TooltipProvider delayDuration={0}>
          <Tooltip>
            <PopoverTrigger asChild>
              <TooltipTrigger asChild>
                <button
                  className={cn(actionButtonClassnames, 'p-2')}
                  type="button"
                >
                  <ChatSettingsIcon className="h-full w-full" />
                </button>
              </TooltipTrigger>
            </PopoverTrigger>
            <PopoverContent
              align="end"
              className="min-w-[380px] px-6 py-7 text-xs"
              side="top"
            >
              <h2 className="text-text-secondary mb-5 text-xs leading-1 uppercase">
                Chat Settings
              </h2>

              <Form {...form}>
                <form
                  className="flex w-full flex-col justify-between gap-10 overflow-hidden"
                  onSubmit={form.handleSubmit(onSubmit)}
                >
                  <ChatConfigForm form={form} thinkingConfig={thinkingConfig} />
                  <div className="flex items-center justify-end gap-2">
                    <PopoverClose asChild>
                      <Button
                        className="min-w-[100px]"
                        rounded="lg"
                        size="xs"
                        variant="outline"
                      >
                        <span>{t('common.cancel')}</span>
                      </Button>
                    </PopoverClose>
                    <PopoverClose asChild>
                      <Button
                        className="min-w-[100px]"
                        rounded="lg"
                        size="xs"
                        type={'submit'}
                      >
                        <span>{t('common.save')}</span>
                      </Button>
                    </PopoverClose>
                  </div>
                </form>
              </Form>
            </PopoverContent>
            <TooltipContent>Chat Settings</TooltipContent>
          </Tooltip>
        </TooltipProvider>
      </Popover>
    </div>
  );
}

export const UpdateChatConfigActionBar = memo(UpdateChatConfigActionBarBase);

export function CreateChatConfigActionBar({
  form,
  currentAI,
}: {
  form: UseFormReturn<ChatConfigFormSchemaType>;
  currentAI?: string;
}) {
  const { t } = useTranslation();
  const auth = useAuth((state) => state.auth);

  const { data: llmProviders } = useGetLLMProviders({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
  });

  const thinkingConfig = useMemo(() => {
    if (!currentAI || !llmProviders) {
      return {
        supportsThinking: false,
        forceEnabled: false,
        reasoningLevel: false,
      };
    }

    const selectedProvider = llmProviders.find(
      (provider) => provider.id === currentAI,
    );
    const modelName = selectedProvider?.model;
    return getThinkingConfig(modelName);
  }, [currentAI, llmProviders]);

  // Auto-enable thinking if force enabled for the model
  useEffect(() => {
    if (thinkingConfig.forceEnabled) {
      form.setValue('thinking', true);
    }
  }, [thinkingConfig.forceEnabled, form]);

  return (
    <div className="flex items-center gap-2">
      <Popover>
        <TooltipProvider delayDuration={0}>
          <Tooltip>
            <PopoverTrigger asChild>
              <TooltipTrigger asChild>
                <button
                  className={cn(actionButtonClassnames, 'p-2')}
                  type="button"
                >
                  <ChatSettingsIcon className="h-full w-full" />
                </button>
              </TooltipTrigger>
            </PopoverTrigger>
            <PopoverContent
              className="max-h-[50vh] min-w-[380px] overflow-auto px-6 py-7 text-xs"
              side="bottom"
            >
              <h2 className="text-text-secondary mb-5 text-xs leading-1 uppercase">
                Chat Settings
              </h2>

              <Form {...form}>
                <form
                  className="flex w-full flex-col justify-between gap-10 overflow-hidden"
                  // onSubmit={form.handleSubmit(onSubmit)}
                >
                  <ChatConfigForm form={form} thinkingConfig={thinkingConfig} />
                  <div className="flex items-center justify-end gap-2">
                    <PopoverClose asChild>
                      <Button
                        className="h-9 min-w-[100px] gap-2 rounded-xl"
                        onClick={() => {
                          form.reset();
                        }}
                        size="sm"
                        variant="outline"
                      >
                        <span>Reset to defaults</span>
                      </Button>
                    </PopoverClose>
                    <PopoverClose asChild>
                      <Button className="min-w-[100px]" rounded="lg" size="xs">
                        <span>{t('common.save')}</span>
                      </Button>
                    </PopoverClose>
                  </div>
                </form>
              </Form>
            </PopoverContent>
            <TooltipContent>Chat Settings</TooltipContent>
          </Tooltip>
        </TooltipProvider>
      </Popover>
    </div>
  );
}
