import { createJob as createJobApi } from '@shinkai_network/shinkai-message-ts/api/jobs/index';
import { createRecurringTask as createRecurringTaskApi } from '@shinkai_network/shinkai-message-ts/api/recurring-tasks/index';

import { type CreateRecurringTaskInput } from './types';

export const createRecurringTask = async ({
  nodeAddress,
  token,
  name,
  description,
  llmProvider,
  cronExpression,
  chatConfig,
  message,
  toolKey,
}: CreateRecurringTaskInput) => {
  const { job_id: jobId } = await createJobApi(nodeAddress, token, {
    llm_provider: llmProvider,
    job_creation_info: {
      scope: {
        vector_fs_items: [],
        vector_fs_folders: [],
      },
      associated_ui: null,
      is_hidden: true,
    },
  });

  const response = await createRecurringTaskApi(nodeAddress, token, {
    cron: cronExpression,
    action: {
      CreateJobWithConfigAndMessage: {
        config: chatConfig,
        job_creation_info: {
          associated_ui: null,
          is_hidden: true,
          scope: {
            vector_fs_folders: [],
            vector_fs_items: [],
          },
        },
        llm_provider: llmProvider,
        message: {
          job_id: jobId,
          content: message,
          tool_key: toolKey,
          parent: null,
          reasoning_content: null,
        },
      },

      // SendMessageToJob: {
      //   job_id: jobId,
      //   message: {
      //     job_id: jobId,
      //     files_inbox: '',
      //     content: 'Recurring task message',
      //     tool_router_key: '',
      //   },
      // },
    },
    description,
    name,
  });
  return response;
};
