import { type Token } from '@shinkai_network/shinkai-message-ts/api/general/types';
import { type RemoveJobsResponse } from '@shinkai_network/shinkai-message-ts/api/jobs/types';

export type RemoveJobsOutput = RemoveJobsResponse;

export type RemoveJobsInput = Token & {
  nodeAddress: string;
  jobIds: string[];
};
