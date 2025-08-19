import {
  createJob as createJobApi,
  updateChatConfig,
} from '@shinkai_network/shinkai-message-ts/api/jobs/index';
import { toolImplementation as createToolCodeApi } from '@shinkai_network/shinkai-message-ts/api/tools/index';

import { DEFAULT_CHAT_CONFIG } from '../../constants';
import { type CreateToolCodeInput } from './types';

export const createToolCode = async ({
  nodeAddress,
  token,
  llmProviderId,
  message,
  jobId,
  tools,
  language,
}: CreateToolCodeInput) => {
  let currentJobId = jobId;
  if (!currentJobId) {
    const { job_id: newJobId } = await createJobApi(nodeAddress, token, {
      llm_provider: llmProviderId,
      job_creation_info: {
        scope: {
          vector_fs_items: [],
          vector_fs_folders: [],
        },
        associated_ui: 'Playground',
        is_hidden: true,
      },
    });

    await updateChatConfig(nodeAddress, token, {
      job_id: newJobId,
      config: {
        custom_system_prompt: '',
        stream: DEFAULT_CHAT_CONFIG.stream,
        custom_prompt: '',
        top_k: DEFAULT_CHAT_CONFIG.top_k,
        top_p: DEFAULT_CHAT_CONFIG.top_p,
        temperature: DEFAULT_CHAT_CONFIG.temperature,
        use_tools: DEFAULT_CHAT_CONFIG.use_tools,
        thinking: DEFAULT_CHAT_CONFIG.thinking,
        reasoning_effort: DEFAULT_CHAT_CONFIG.reasoning_effort,
        web_search_enabled: DEFAULT_CHAT_CONFIG.web_search_enabled,
      },
    });
    currentJobId = newJobId;
  }

  return await createToolCodeApi(nodeAddress, token, {
    raw: false,
    message: {
      job_id: currentJobId,
      content: message,
      parent: null,
    },
    tools,
    language,
  });
};
