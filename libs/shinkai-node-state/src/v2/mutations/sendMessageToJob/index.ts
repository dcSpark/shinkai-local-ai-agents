import {
  sendMessageToJob as sendMessageToJobApi,
  uploadFilesToJob,
} from '@shinkai_network/shinkai-message-ts/api/jobs/index';

import { type SendMessageToJobInput } from './types';

export const sendMessageToJob = async ({
  nodeAddress,
  token,
  jobId,
  message,
  parent,
  files,
  toolKey,
}: SendMessageToJobInput) => {
  let filenames: string[] = [];
  if (files && files.length > 0) {
    const uploadResponses = await uploadFilesToJob(nodeAddress, token, jobId, files);
    filenames = uploadResponses.map(response => response.filename);
  }

  return await sendMessageToJobApi(nodeAddress, token, {
    job_message: {
      content: message,
      job_id: jobId,
      parent: parent,
      tool_key: toolKey,
      fs_files_paths: [],
      job_filenames: filenames,
      reasoning_content: null,
    },
  });
};
