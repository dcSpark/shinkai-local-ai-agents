import { DialogClose } from '@radix-ui/react-dialog';
import { DotsVerticalIcon } from '@radix-ui/react-icons';
import { useTranslation } from '@shinkai_network/shinkai-i18n';
import { type JobConfig } from '@shinkai_network/shinkai-message-ts/api/jobs/types';
import { useRemoveRecurringTask } from '@shinkai_network/shinkai-node-state/v2/mutations/removeRecurringTask/useRemoveRecurringTask';
import { useRunTaskNow } from '@shinkai_network/shinkai-node-state/v2/mutations/runTaskNow/useRunTaskNow';
import { useUpdateRecurringTask } from '@shinkai_network/shinkai-node-state/v2/mutations/updateRecurringTask/useUpdateRecurringTask';
import { useGetRecurringTaskNextExecutionTime } from '@shinkai_network/shinkai-node-state/v2/queries/getRecurringTaskNextExecutionTime/useGetRecurringTaskNextExecutionTime';
import { useGetRecurringTasks } from '@shinkai_network/shinkai-node-state/v2/queries/getRecurringTasks/useGetRecurringTasks';
import {
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
  Popover,
  PopoverContent,
  PopoverTrigger,
  Skeleton,
  Switch,
} from '@shinkai_network/shinkai-ui';
import {
  ScheduledTasksComingSoonIcon,
  ScheduledTasksIcon,
} from '@shinkai_network/shinkai-ui/assets';
import { cn } from '@shinkai_network/shinkai-ui/utils';
import cronstrue from 'cronstrue';
import { formatDistance } from 'date-fns';
import {
  Edit,
  PlayIcon,
  PlusIcon,
  RefreshCwIcon,
  TrashIcon,
} from 'lucide-react';
import React from 'react';
import { Link, useNavigate } from 'react-router';
import { toast } from 'sonner';

import { useAuth } from '../store/auth';
import { SimpleLayout } from './layout/simple-layout';

export const Tasks = () => {
  const auth = useAuth((state) => state.auth);
  const { t } = useTranslation();

  const {
    data: tasks,
    isPending,
    isSuccess,
  } = useGetRecurringTasks({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
  });

  const {
    data: cronTasksNextExecutionTime,
    isSuccess: isCronTasksNextExecutionTimeSuccess,
    refetch,
    isRefetching,
  } = useGetRecurringTaskNextExecutionTime({
    nodeAddress: auth?.node_address ?? '',
    token: auth?.api_v2_key ?? '',
  });

  const { mutateAsync: updateRecurringTask } = useUpdateRecurringTask({
    onError: (error) => {
      toast.error('Failed to updated task', {
        description: error.response?.data?.message ?? error.message,
      });
    },
  });

  return (
    <SimpleLayout
      headerRightElement={
        <div className="flex items-center gap-3">
          {isCronTasksNextExecutionTimeSuccess &&
            cronTasksNextExecutionTime.length > 0 && (
              <Popover>
                <PopoverTrigger
                  className={cn(
                    buttonVariants({
                      variant: 'outline',
                      size: 'xs',
                      rounded: 'lg',
                    }),
                  )}
                  onClick={() => refetch()}
                >
                  <ScheduledTasksComingSoonIcon className="size-3.5" />
                  <span className="text-xs">Upcoming</span>
                </PopoverTrigger>
                <PopoverContent
                  align="end"
                  alignOffset={-4}
                  className="flex w-[400px] flex-col gap-2 px-3.5 py-4 text-xs"
                  onOpenAutoFocus={(e) => e.preventDefault()}
                >
                  <div className="flex items-center justify-between gap-2">
                    <h1 className="text-sm">Upcoming Tasks </h1>
                    <Button
                      className="w-auto"
                      disabled={isRefetching}
                      isLoading={isRefetching}
                      onClick={() => refetch()}
                      rounded="lg"
                      size="xs"
                      variant="outline"
                    >
                      {!isRefetching && (
                        <RefreshCwIcon className="h-3.5 w-3.5" />
                      )}
                    </Button>
                  </div>
                  {cronTasksNextExecutionTime?.map(([task, date]) => (
                    <div
                      className="flex items-start gap-2 py-1"
                      key={task.task_id}
                    >
                      <ScheduledTasksIcon className="text-text-secondary mt-1 size-4" />
                      <div className="flex flex-col gap-1 text-left">
                        <span className="text-text-default text-sm">
                          {task.name}
                          <span className="text-text-secondary border-divider mx-1 rounded-lg border px-1.5 py-1 text-xs">
                            {cronstrue.toString(task.cron, {
                              throwExceptionOnParseError: false,
                            })}
                          </span>
                        </span>
                        <span className="text-text-secondary">
                          {' '}
                          Next execution in{' '}
                          <span className="text-text-secondary font-semibold">
                            {formatDistance(new Date(date), new Date(), {
                              addSuffix: false,
                            })}
                          </span>
                        </span>
                      </div>
                    </div>
                  ))}
                </PopoverContent>
              </Popover>
            )}

          <Link
            className={cn(
              buttonVariants({
                variant: 'default',
                size: 'sm',
              }),
            )}
            to="/tasks/create"
          >
            <PlusIcon className="size-4" />
            Create New
          </Link>
        </div>
      }
      title="Scheduled Tasks"
    >
      <div className="flex flex-col gap-2.5 pt-4">
        {isPending &&
          Array.from({ length: 8 }).map((_, idx) => (
            <Skeleton
              className={cn(
                'grid h-[100px] animate-pulse grid-cols-[1fr_115px_36px] items-center gap-5 rounded-xs px-2 py-4 text-left text-sm',
              )}
              key={idx}
            ></Skeleton>
          ))}
        {isSuccess &&
          tasks.length > 0 &&
          tasks?.map((task) => (
            <TaskCard
              cronExpression={task.cron}
              description={task.description}
              key={task.task_id}
              name={task.name}
              onCheckedChange={async (active) => {
                if ('CreateJobWithConfigAndMessage' in task.action) {
                  const config: JobConfig =
                    task.action.CreateJobWithConfigAndMessage.config;
                  const message =
                    task.action.CreateJobWithConfigAndMessage.message.content;
                  const llmProvider =
                    task.action.CreateJobWithConfigAndMessage.llm_provider;
                  const jobId =
                    task.action.CreateJobWithConfigAndMessage.message.job_id;
                  await updateRecurringTask({
                    nodeAddress: auth?.node_address ?? '',
                    token: auth?.api_v2_key ?? '',
                    taskId: task.task_id.toString(),
                    active,
                    chatConfig: config,
                    cronExpression: task.cron,
                    description: task.description,
                    jobId,
                    toolKey:
                      task.action?.CreateJobWithConfigAndMessage?.message
                        ?.tool_key,
                    llmProvider,
                    name: task.name,
                    message,
                  });
                  return;
                }
              }}
              paused={task.paused}
              prompt={
                'CreateJobWithConfigAndMessage' in task.action
                  ? task.action.CreateJobWithConfigAndMessage.message.content
                  : ''
              }
              taskId={task.task_id}
            />
          ))}
        {isSuccess && tasks?.length === 0 && (
          <div className="mx-auto flex h-28 max-w-lg flex-col items-center justify-center gap-2 text-center">
            <h1 className="text-base font-medium">
              {t('tasksPage.noTasksTitle')}
            </h1>
            <p className="text-text-secondary text-sm">
              {t('tasksPage.noTasksDescription')}
            </p>
          </div>
        )}
      </div>
    </SimpleLayout>
  );
};

const TaskCard = ({
  taskId,
  name,
  description,
  cronExpression,
  prompt,
  paused,
  onCheckedChange,
}: {
  taskId: number;
  name: string;
  description?: string;
  cronExpression: string;
  prompt: string;
  paused: boolean;
  onCheckedChange: (active: boolean) => void;
}) => {
  const navigate = useNavigate();
  const { t } = useTranslation();
  const auth = useAuth((state) => state.auth);

  const [isDeleteTaskDrawerOpen, setIsDeleteTaskDrawerOpen] =
    React.useState(false);

  const readableCron = cronstrue.toString(cronExpression, {
    throwExceptionOnParseError: false,
  });

  const { mutateAsync: runTaskNow } = useRunTaskNow({
    onSuccess: () => {
      toast.success('Task run successfully');
    },
    onError: (error: any) => {
      toast.error('Failed to run task', {
        description: error.response?.data?.message ?? error.message,
      });
    },
  });

  return (
    <div
      className={cn(
        'border-divider bg-bg-secondary grid grid-cols-[1fr_100px_120px_40px] items-start gap-5 rounded-lg border p-3.5 text-left text-sm',
      )}
    >
      <div className="flex flex-col gap-2.5">
        <div className="flex items-center gap-2">
          <span className="text-sm font-medium text-white capitalize">
            {name}
          </span>
        </div>
        <p className="text-text-secondary line-clamp-2 text-xs">
          {description ?? '-'}
        </p>
        <p className="text-text-default line-clamp-2 text-xs">
          <span className="text-text-secondary mr-2">Prompt</span>
          {prompt}
        </p>
        <p className="text-text-default line-clamp-2 text-xs">
          <span className="text-text-secondary mr-2">Schedule</span>
          {readableCron}
        </p>
      </div>
      <div className="flex items-center gap-3 pt-1">
        <Switch checked={!paused} onCheckedChange={onCheckedChange} />
        <label className="text-text-default text-xs" htmlFor="all">
          {paused ? 'Inactive' : 'Active'}
        </label>
      </div>
      <Link
        className={cn(
          buttonVariants({
            variant: 'outline',
            size: 'xs',
            rounded: 'lg',
          }),
        )}
        to={`/tasks/${taskId}`}
      >
        <svg
          className="size-4"
          fill="none"
          height="24"
          stroke="currentColor"
          strokeLinecap="round"
          strokeLinejoin="round"
          strokeWidth="2"
          viewBox="0 0 24 24"
          width="24"
          xmlns="http://www.w3.org/2000/svg"
        >
          <path d="M13 12h8" />
          <path d="M13 18h8" />
          <path d="M13 6h8" />
          <path d="M3 12h1" />
          <path d="M3 18h1" />
          <path d="M3 6h1" />
          <path d="M8 12h1" />
          <path d="M8 18h1" />
          <path d="M8 6h1" />
        </svg>
        View Logs
      </Link>
      <DropdownMenu modal={false}>
        <DropdownMenuTrigger asChild>
          <div
            className={cn(
              buttonVariants({
                variant: 'outline',
                size: 'auto',
              }),
              'size-[34px] rounded-md border p-1',
            )}
            onClick={(event) => {
              event.stopPropagation();
            }}
            role="button"
            tabIndex={0}
          >
            <span className="sr-only">{t('common.moreOptions')}</span>
            <DotsVerticalIcon className="text-text-secondary" />
          </div>
        </DropdownMenuTrigger>
        <DropdownMenuContent
          align="end"
          className="w-[160px] border px-2.5 py-2"
        >
          {[
            {
              name: t('common.edit'),
              icon: <Edit className="mr-3 h-4 w-4" />,
              onClick: () => {
                void navigate(`/tasks/edit/${taskId}`);
              },
            },
            {
              name: t('tasks.runNow'),
              icon: <PlayIcon className="mr-3 h-4 w-4" />,
              onClick: async () => {
                await runTaskNow({
                  nodeAddress: auth?.node_address ?? '',
                  token: auth?.api_v2_key ?? '',
                  taskId: taskId.toString(),
                });
              },
            },
            {
              name: t('common.delete'),
              icon: <TrashIcon className="mr-3 h-4 w-4" />,
              onClick: () => {
                setIsDeleteTaskDrawerOpen(true);
              },
            },
          ].map((option) => (
            <React.Fragment key={option.name}>
              {option.name === 'Delete' && <DropdownMenuSeparator />}
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
      <RemoveTaskDrawer
        onOpenChange={setIsDeleteTaskDrawerOpen}
        open={isDeleteTaskDrawerOpen}
        taskId={taskId}
        taskName={name}
      />
    </div>
  );
};

const RemoveTaskDrawer = ({
  open,
  onOpenChange,
  taskId,
  taskName,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  taskId: number;
  taskName: string;
}) => {
  const { t } = useTranslation();
  const auth = useAuth((state) => state.auth);
  const { mutateAsync: removeTask, isPending } = useRemoveRecurringTask({
    onSuccess: () => {
      onOpenChange(false);
      toast.success('Delete task successfully');
    },
    onError: (error) => {
      toast.error('Failed remove task', {
        description: error.response?.data?.message ?? error.message,
      });
    },
  });

  return (
    <Dialog onOpenChange={onOpenChange} open={open}>
      <DialogContent className="sm:max-w-[425px]">
        <DialogTitle className="pb-0">
          Delete Task <span className="font-mono text-base"> {taskName}</span> ?
        </DialogTitle>
        <DialogDescription>
          The task will be permanently deleted. This action cannot be undone.
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
                await removeTask({
                  nodeAddress: auth?.node_address ?? '',
                  token: auth?.api_v2_key ?? '',
                  recurringTaskId: taskId.toString(),
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
