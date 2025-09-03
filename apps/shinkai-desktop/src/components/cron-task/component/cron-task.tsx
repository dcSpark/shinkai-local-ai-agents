import { zodResolver } from '@hookform/resolvers/zod';
import { useTranslation } from '@shinkai_network/shinkai-i18n';
import { type RecurringTask } from '@shinkai_network/shinkai-message-ts/api/recurring-tasks/types';
import { DEFAULT_CHAT_CONFIG } from '@shinkai_network/shinkai-node-state/v2/constants';
import { useCreateRecurringTask } from '@shinkai_network/shinkai-node-state/v2/mutations/createRecurringTask/useCreateRecurringTask';
import { useUpdateRecurringTask } from '@shinkai_network/shinkai-node-state/v2/mutations/updateRecurringTask/useUpdateRecurringTask';
import { useGetTools } from '@shinkai_network/shinkai-node-state/v2/queries/getToolsList/useGetToolsList';
import {
  Badge,
  Button,
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
  Form,
  FormControl,
  FormDescription,
  FormField,
  FormItem,
  FormLabel,
  HoverCard,
  HoverCardContent,
  HoverCardTrigger,
  Label,
  Slider,
  Switch,
  Textarea,
  TextField,
} from '@shinkai_network/shinkai-ui';
import {
  ScheduledTasksIcon,
  ToolsIcon,
} from '@shinkai_network/shinkai-ui/assets';
import { formatText } from '@shinkai_network/shinkai-ui/helpers';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import cronstrue from 'cronstrue';
import { ChevronDownIcon } from 'lucide-react';
import { useEffect, useMemo } from 'react';
import { useForm } from 'react-hook-form';
import { useNavigate } from 'react-router';
import { toast } from 'sonner';
import { z } from 'zod';

import { SubpageLayout } from '../../../pages/layout/simple-layout';
import { useAuth } from '../../../store/auth';
import { useSettings } from '../../../store/settings';
import { AIModelSelector } from '../../chat/chat-action-bar/ai-update-selection-action-bar';
import { actionButtonClassnames } from '../../chat/conversation-footer';

const createTaskFormSchema = z.object({
  name: z.string(),
  description: z.string(),
  llmOrAgentId: z.string(),
  cronExpression: z.string().refine(
    (value) => {
      try {
        cronstrue.toString(value, {
          throwExceptionOnParseError: true,
        });
        return true;
      } catch {
        return false;
      }
    },
    {
      message:
        'Invalid cron expression. Please provide a valid cron expression.',
    },
  ),
  jobConfig: z.object({
    custom_system_prompt: z.string().optional(),
    custom_prompt: z.string(),
    temperature: z.number(),
    max_tokens: z.number().optional(),
    seed: z.number().optional(),
    top_k: z.number(),
    top_p: z.number(),
    stream: z.boolean().optional(),
    use_tools: z.boolean().optional(),
    thinking: z.boolean().optional(),
    reasoning_effort: z.enum(['low', 'medium', 'high']).optional(),
    web_search_enabled: z.boolean().optional(),
  }),
  jobMessage: z.object({
    content: z.string(),
    tool_key: z.string().optional(),
  }),
});
type CreateTaskForm = z.infer<typeof createTaskFormSchema>;

type CronTaskProps = {
  mode: 'create' | 'edit';
  initialValues?: RecurringTask;
};
function CronTask({ mode, initialValues }: CronTaskProps) {
  const { t } = useTranslation();
  const defaultAgentId = useSettings((state) => state.defaultAgentId);
  const navigate = useNavigate();
  const auth = useAuth((state) => state.auth);
  const form = useForm<CreateTaskForm>({
    resolver: zodResolver(createTaskFormSchema),
    defaultValues: {
      name: '',
      description: '',
      cronExpression: '',
      jobConfig: {
        custom_system_prompt: '',
        custom_prompt: '',
        temperature: DEFAULT_CHAT_CONFIG.temperature,
        top_k: DEFAULT_CHAT_CONFIG.top_k,
        top_p: DEFAULT_CHAT_CONFIG.top_p,
        stream: DEFAULT_CHAT_CONFIG.stream ?? false,
        use_tools: DEFAULT_CHAT_CONFIG.use_tools ?? false,
        thinking: DEFAULT_CHAT_CONFIG.thinking,
        reasoning_effort: DEFAULT_CHAT_CONFIG.reasoning_effort,
        web_search_enabled: DEFAULT_CHAT_CONFIG.web_search_enabled,
      },
      llmOrAgentId: defaultAgentId,
      jobMessage: {
        content: '',
        tool_key: '',
      },
    },
  });

  useEffect(() => {
    if (
      initialValues &&
      'CreateJobWithConfigAndMessage' in initialValues.action
    ) {
      const initialConfig =
        initialValues.action.CreateJobWithConfigAndMessage.config;
      form.reset({
        cronExpression: initialValues.cron,
        description: initialValues.description,
        name: initialValues.name,
        jobConfig: {
          custom_system_prompt: initialConfig.custom_system_prompt ?? '',
          custom_prompt: initialConfig.custom_prompt,
          temperature: initialConfig.temperature,
          top_k: initialConfig.top_k,
          top_p: initialConfig.top_p,
          stream: initialConfig.stream ?? false,
          use_tools: initialConfig.use_tools ?? false,
          thinking: initialConfig.thinking,
          reasoning_effort: initialConfig.reasoning_effort,
          web_search_enabled: initialConfig.web_search_enabled,
        },
        jobMessage: {
          content:
            'CreateJobWithConfigAndMessage' in initialValues.action
              ? initialValues.action.CreateJobWithConfigAndMessage.message
                  .content
              : '',
          tool_key:
            'CreateJobWithConfigAndMessage' in initialValues.action
              ? initialValues.action.CreateJobWithConfigAndMessage.message
                  .tool_key
              : '',
        },
        llmOrAgentId:
          'CreateJobWithConfigAndMessage' in initialValues.action
            ? initialValues.action.CreateJobWithConfigAndMessage.llm_provider
            : defaultAgentId,
      });
    }
  }, [form, initialValues, defaultAgentId]);

  const { data: toolsList, isSuccess: isToolListSuccess } = useGetTools({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
  });

  const { mutateAsync: createRecurringTask, isPending } =
    useCreateRecurringTask({
      onSuccess: () => {
        void navigate('/tasks');
      },
      onError: (error) => {
        toast.error('Failed to create task', {
          description: error.response?.data?.message ?? error.message,
        });
      },
    });
  const {
    mutateAsync: updateRecurringTask,
    isPending: isUpdateRecurringTaskPending,
  } = useUpdateRecurringTask({
    onSuccess: () => {
      toast.success('Task updated successfully');
      void navigate('/tasks');
    },
    onError: (error) => {
      toast.error('Failed to updated task', {
        description: error.response?.data?.message ?? error.message,
      });
    },
  });

  const submit = async (values: CreateTaskForm) => {
    if (mode === 'create') {
      await createRecurringTask({
        nodeAddress: auth?.node_address ?? '',
        token: auth?.api_v2_key ?? '',
        cronExpression: values.cronExpression,
        chatConfig: values.jobConfig,
        message: values.jobMessage.content,
        toolKey: values.jobMessage.tool_key,
        llmProvider: values.llmOrAgentId,
        name: values.name,
        description: values.description,
      });
      return;
    }
    if (mode === 'edit' && initialValues) {
      await updateRecurringTask({
        nodeAddress: auth?.node_address ?? '',
        token: auth?.api_v2_key ?? '',
        cronExpression: values.cronExpression,
        chatConfig: values.jobConfig,
        message: values.jobMessage.content,
        toolKey: values.jobMessage.tool_key,
        llmProvider: values.llmOrAgentId,
        name: values.name,
        description: values.description,
        taskId: initialValues.task_id.toString() ?? '',
        jobId:
          'CreateJobWithConfigAndMessage' in initialValues.action
            ? initialValues?.action.CreateJobWithConfigAndMessage.message.job_id
            : '',
        active: !initialValues.paused,
      });
    }
  };

  useEffect(() => {
    if (defaultAgentId) {
      form.setValue('llmOrAgentId', defaultAgentId);
    }
  }, [defaultAgentId]);

  const currentCronExpression = form.watch('cronExpression');

  const readableCronExpression = useMemo(() => {
    const readableCron = cronstrue.toString(currentCronExpression, {
      throwExceptionOnParseError: false,
    });
    if (readableCron.toLowerCase().includes('error')) {
      return null;
    }
    return readableCron;
  }, [currentCronExpression, form]);
  return (
    <SubpageLayout
      className="container"
      title={`${mode === 'create' ? 'Create' : 'Edit'} Scheduled Task`}
    >
      <p className="text-text-secondary -mt-8 py-3 pb-6 text-center text-sm">
        Schedule recurring tasks at a specified time
      </p>
      <Form {...form}>
        <form
          className="flex grid grid-cols-2 flex-col justify-between gap-8 pt-4"
          onSubmit={form.handleSubmit(submit, (errors) => {
            console.error('Form validation errors:', errors);
            toast.error('Validation failed. Please check the form fields.', {
              description: Object.values(errors)
                .map((e) => e.message)
                .join('\n'),
            });
          })}
        >
          <div className="space-y-4">
            <FormField
              control={form.control}
              name="name"
              render={({ field }) => (
                <TextField autoFocus field={field} label="Task Name" />
              )}
            />
            <FormField
              control={form.control}
              name="description"
              render={({ field }) => (
                <TextField
                  field={field}
                  helperMessage={t('cronTask.taskDescriptionHelper')}
                  label={t('cronTask.taskDescription')}
                />
              )}
            />
            <FormField
              control={form.control}
              name="jobMessage.content"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>{t('cronTask.taskPrompt')}</FormLabel>
                  <FormControl>
                    <Textarea
                      className="!min-h-[130px] text-sm"
                      placeholder={t('cronTask.promptPlaceholder')}
                      resize="vertical"
                      spellCheck={false}
                      {...field}
                    />
                  </FormControl>
                  <FormDescription>
                    {t('cronTask.promptExample')}
                  </FormDescription>
                </FormItem>
              )}
            />
            <FormField
              control={form.control}
              name="cronExpression"
              render={({ field }) => (
                <TextField
                  field={field}
                  helperMessage={t('cronTask.cronExample')}
                  label={t('cronTask.cronExpression')}
                />
              )}
            />
            {readableCronExpression && (
              <div className="flex items-center gap-2 text-xs">
                <ScheduledTasksIcon className="size-4" />
                <span>
                  {t('cronTask.cronWillRun', {
                    schedule: readableCronExpression.toLowerCase(),
                    expression: form.watch('cronExpression'),
                  })}
                </span>
              </div>
            )}
            <div className="flex flex-wrap gap-2">
              {[
                {
                  label: t('cronTask.scheduleOptions.every5Min'),
                  cron: '*/5 * * * *',
                },
                {
                  label: t('cronTask.scheduleOptions.every5Hours'),
                  cron: '0 */5 * * *',
                },
                {
                  label: t('cronTask.scheduleOptions.everyMonday8am'),
                  cron: '0 8 * * 1',
                },
                {
                  label: t('cronTask.scheduleOptions.everyJanuary1st12am'),
                  cron: '0 0 1 1 *',
                },
                {
                  label: t('cronTask.scheduleOptions.every1stMonth12pm'),
                  cron: '0 12 1 * *',
                },
              ].map((item) => (
                <Badge
                  key={item.cron}
                  onClick={() => {
                    form.setValue('cronExpression', item.cron);
                  }}
                  variant="outline"
                >
                  <span className="text-xs">{item.label}</span>
                </Badge>
              ))}
            </div>
          </div>
          <div className="space-y-4">
            <div className="space-y-4">
              <div className="bg-bg-secondary space-y-6 rounded-lg px-4 py-4 pb-7">
                <span className="text-text-default flex-1 items-center gap-1 truncate py-2 text-left text-xs font-semibold">
                  {t('cronTask.aiModelConfiguration')}
                </span>

                <div className="space-y-4">
                  <div className="grid grid-cols-[1fr_auto] items-center">
                    <span className="text-text-secondary text-xs">
                      {t('cronTask.aiAgent')}
                    </span>
                    <AIModelSelector
                      onValueChange={(value) => {
                        form.setValue('llmOrAgentId', value);
                      }}
                      value={form.watch('llmOrAgentId')}
                    />
                  </div>
                  <div className="grid grid-cols-[1fr_auto] items-center">
                    <span className="text-text-secondary text-xs">
                      {t('cronTask.forceToolUsage')}
                    </span>

                    <DropdownMenu>
                      <DropdownMenuTrigger
                        className={cn(
                          actionButtonClassnames,
                          'w-auto max-w-[250px] justify-between truncate [&[data-state=open]>.icon]:rotate-180',
                        )}
                      >
                        <div className="flex items-center gap-1 truncate">
                          <ToolsIcon className="mr-1 h-4 w-4" />
                          <span>
                            {form.watch('jobMessage.tool_key')
                              ? formatText(
                                  form
                                    .watch('jobMessage.tool_key')
                                    ?.split(':::')?.[2] ?? '',
                                )
                              : 'None'}
                          </span>
                        </div>
                        <ChevronDownIcon className="icon h-3 w-3" />
                      </DropdownMenuTrigger>
                      <DropdownMenuContent
                        align="start"
                        className="max-h-[400px] min-w-[330px] overflow-y-auto p-1 py-2"
                        side="top"
                      >
                        <DropdownMenuRadioGroup
                          onValueChange={(value) => {
                            form.setValue('jobMessage.tool_key', value);
                          }}
                          value={form.watch('jobMessage.tool_key')}
                        >
                          <DropdownMenuRadioItem
                            className="text-text-default flex cursor-pointer items-center gap-1.5 rounded-md px-2 py-2 transition-colors"
                            value=""
                          >
                            <ToolsIcon className="h-3.5 w-3.5 shrink-0" />
                            <div className="flex flex-col gap-1">
                              <span className="text-xs">
                                {t('common.none')}
                              </span>
                            </div>
                          </DropdownMenuRadioItem>
                          {isToolListSuccess &&
                            toolsList.length > 0 &&
                            toolsList?.map((tool) => (
                              <DropdownMenuRadioItem
                                className="text-text-default flex cursor-pointer items-center gap-1.5 rounded-md px-2 py-2 transition-colors"
                                key={tool.tool_router_key}
                                value={tool.tool_router_key}
                              >
                                <ToolsIcon className="h-3.5 w-3.5 shrink-0" />
                                <div className="flex flex-col gap-1">
                                  <span className="text-xs">
                                    {formatText(tool.name)}
                                  </span>
                                </div>
                              </DropdownMenuRadioItem>
                            ))}
                        </DropdownMenuRadioGroup>
                      </DropdownMenuContent>
                    </DropdownMenu>
                  </div>
                </div>

                <Collapsible className="space-y-4">
                  <CollapsibleTrigger className="text-text-secondary hover:text-text-default flex items-center gap-1 text-sm [&[data-state=open]>svg]:rotate-90">
                    {t('common.advanced')}
                    <ChevronDownIcon className="h-3 w-3" />
                  </CollapsibleTrigger>
                  <CollapsibleContent className="space-y-4">
                    <FormField
                      control={form.control}
                      name="jobConfig.custom_system_prompt"
                      render={({ field }) => (
                        <FormItem>
                          <FormLabel>System Prompt</FormLabel>
                          <FormControl>
                            <Textarea
                              className="!min-h-[130px] text-sm"
                              resize="vertical"
                              spellCheck={false}
                              {...field}
                            />
                          </FormControl>
                        </FormItem>
                      )}
                    />

                    <FormField
                      control={form.control}
                      name="jobConfig.stream"
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
                              <FormLabel className="text-text-default static space-y-1.5 text-sm">
                                {t('cronTask.enableStream')}
                              </FormLabel>
                            </div>
                          </div>
                        </FormItem>
                      )}
                    />
                    <FormField
                      control={form.control}
                      name="jobConfig.use_tools"
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
                              <FormLabel className="text-text-default static space-y-1.5 text-sm">
                                {t('cronTask.enableTools')}
                              </FormLabel>
                            </div>
                          </div>
                        </FormItem>
                      )}
                    />
                    <FormField
                      control={form.control}
                      name="jobConfig.temperature"
                      render={({ field }) => (
                        <FormItem className="flex gap-2.5">
                          <FormControl>
                            <HoverCard openDelay={200}>
                              <HoverCardTrigger asChild>
                                <div className="grid w-full gap-4">
                                  <div className="flex items-center justify-between">
                                    <Label htmlFor="temperature">
                                      {t('cronTask.temperature')}
                                    </Label>
                                    <span className="text-text-secondary hover:border-border w-12 rounded-md border border-transparent px-2 py-0.5 text-right text-sm">
                                      {field.value}
                                    </span>
                                  </div>
                                  <Slider
                                    aria-label="Temperature"
                                    className="[&_[role=slider]]:h-4 [&_[role=slider]]:w-4"
                                    id="temperature"
                                    max={1}
                                    onValueChange={(vals) => {
                                      field.onChange(vals[0]);
                                    }}
                                    step={0.1}
                                    value={[field.value]}
                                  />
                                </div>
                              </HoverCardTrigger>
                              <HoverCardContent
                                align="start"
                                className="w-[260px] px-2 py-3 text-xs"
                                side="left"
                              >
                                {t('cronTask.temperatureInfo')}
                              </HoverCardContent>
                            </HoverCard>
                          </FormControl>
                        </FormItem>
                      )}
                    />
                    <FormField
                      control={form.control}
                      name="jobConfig.top_p"
                      render={({ field }) => (
                        <FormItem className="flex gap-2.5">
                          <FormControl>
                            <HoverCard openDelay={200}>
                              <HoverCardTrigger asChild>
                                <div className="grid w-full gap-4">
                                  <div className="flex items-center justify-between">
                                    <Label htmlFor="topP">
                                      {t('cronTask.topP')}
                                    </Label>
                                    <span className="text-text-secondary hover:border-border w-12 rounded-md border border-transparent px-2 py-0.5 text-right text-sm">
                                      {field.value}
                                    </span>
                                  </div>
                                  <Slider
                                    aria-label="Top P"
                                    className="[&_[role=slider]]:h-4 [&_[role=slider]]:w-4"
                                    id="topP"
                                    max={1}
                                    min={0}
                                    onValueChange={(vals) => {
                                      field.onChange(vals[0]);
                                    }}
                                    step={0.1}
                                    value={[field.value]}
                                  />
                                </div>
                              </HoverCardTrigger>
                              <HoverCardContent
                                align="start"
                                className="w-[260px] px-2 py-3 text-xs"
                                side="left"
                              >
                                {t('cronTask.topPInfo')}
                              </HoverCardContent>
                            </HoverCard>
                          </FormControl>
                        </FormItem>
                      )}
                    />
                    <FormField
                      control={form.control}
                      name="jobConfig.top_k"
                      render={({ field }) => (
                        <FormItem className="flex gap-2.5">
                          <FormControl>
                            <HoverCard openDelay={200}>
                              <HoverCardTrigger asChild>
                                <div className="grid w-full gap-4">
                                  <div className="flex items-center justify-between">
                                    <Label htmlFor="topK">
                                      {t('cronTask.topK')}
                                    </Label>
                                    <span className="text-text-secondary hover:border-border w-12 rounded-md border border-transparent px-2 py-0.5 text-right text-sm">
                                      {field.value}
                                    </span>
                                  </div>
                                  <Slider
                                    aria-label="Top K"
                                    className="[&_[role=slider]]:h-4 [&_[role=slider]]:w-4"
                                    id="topK"
                                    max={100}
                                    onValueChange={(vals) => {
                                      field.onChange(vals[0]);
                                    }}
                                    step={1}
                                    value={[field.value]}
                                  />
                                </div>
                              </HoverCardTrigger>
                              <HoverCardContent
                                align="start"
                                className="w-[260px] px-2 py-3 text-xs"
                                side="left"
                              >
                                {t('cronTask.topKInfo')}
                              </HoverCardContent>
                            </HoverCard>
                          </FormControl>
                        </FormItem>
                      )}
                    />
                  </CollapsibleContent>
                </Collapsible>
              </div>
            </div>
          </div>
          <div className="col-span-2 flex items-center justify-end gap-2">
            <Button
              className="min-w-[120px]"
              disabled={isPending || isUpdateRecurringTaskPending}
              onClick={() => navigate('/tasks')}
              size="sm"
              type="button"
              variant="outline"
            >
              {t('common.cancel')}
            </Button>
            <Button
              className="min-w-[120px]"
              disabled={isPending || isUpdateRecurringTaskPending}
              isLoading={isPending || isUpdateRecurringTaskPending}
              size="sm"
              type="submit"
            >
              {mode === 'create' ? t('common.save') : t('common.update')}
            </Button>
          </div>
        </form>
      </Form>
    </SubpageLayout>
  );
}

export default CronTask;
