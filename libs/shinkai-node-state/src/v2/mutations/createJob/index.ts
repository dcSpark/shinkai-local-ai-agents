import {
  createJob as createJobApi,
  sendMessageToJob,
  updateChatConfig,
  uploadFilesToJob,
} from '@shinkai_network/shinkai-message-ts/api/jobs/index';

import { type CreateJobInput } from './types';

export const createJob = async ({
  nodeAddress,
  token,
  llmProvider,
  content,
  toolKey,
  isHidden,
  files,
  selectedVRFiles,
  selectedVRFolders,
  chatConfig,
}: CreateJobInput) => {
  const { job_id: jobId } = await createJobApi(nodeAddress, token, {
    llm_provider: llmProvider,
    job_creation_info: {
      scope: {
        vector_fs_items: selectedVRFiles ?? [],
        vector_fs_folders: selectedVRFolders ?? [],
      },
      associated_ui: null,
      is_hidden: isHidden,
    },
  });

  if (chatConfig) {
    await updateChatConfig(nodeAddress, token, {
      job_id: jobId,
      config: chatConfig,
    });
  }

  let filenames: string[] = [];
  if (files && files.length > 0) {
    const uploadResponses = await uploadFilesToJob(
      nodeAddress,
      token,
      jobId,
      files,
    );
    filenames = uploadResponses.map((response) => response.filename);
  }

  await sendMessageToJob(nodeAddress, token, {
    job_message: {
      content,
      job_id: jobId,
      parent: '',
      tool_key: toolKey,
      fs_files_paths: [],
      job_filenames: filenames,
      reasoning_content: null,
    },
  });

  return { jobId };
};
