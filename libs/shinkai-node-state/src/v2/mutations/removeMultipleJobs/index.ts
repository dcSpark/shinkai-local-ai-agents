import { removeJobs as removeJobsApi } from '@shinkai_network/shinkai-message-ts/api/jobs/index';

import { type RemoveJobsInput } from './types';

export const removeJobs = async ({
  nodeAddress,
  token,
  jobIds,
}: RemoveJobsInput) => {
  return await removeJobsApi(nodeAddress, token, {
    job_ids: jobIds,
  });
};
