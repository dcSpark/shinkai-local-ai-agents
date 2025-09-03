import { setRecurringTask as setRecurringTaskApi } from '@shinkai_network/shinkai-message-ts/api/recurring-tasks/index';

import { type UpdateRecurringTaskInput } from './types';

export const updateRecurringTask = async ({
  nodeAddress,
  token,
  name,
  description,
  llmProvider,
  cronExpression,
  chatConfig,
  message,
  toolKey,
  taskId,
  jobId,
  active,
}: UpdateRecurringTaskInput) => {
  const response = await setRecurringTaskApi(nodeAddress, token, {
    cron_task_id: taskId,
    name: name,
    description: description,
    cron: cronExpression,
    paused: !active,
    action: {
      CreateJobWithConfigAndMessage: {
        config: chatConfig,
        job_creation_info: {
          associated_ui: null,
          is_hidden: false,
          scope: {
            vector_fs_folders: [],
            vector_fs_items: [],
          },
        },
        llm_provider: llmProvider,
        message: {
          job_id: jobId,
          content: message,
          reasoning_content: null,
          tool_key: toolKey,
          parent: null,
        },
      },
    },
  });
  return response;
};
