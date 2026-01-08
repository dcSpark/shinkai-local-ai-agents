import { type Token } from '@shinkai_network/shinkai-message-ts/api/general/types';
import {
  type JobMessageResponse,
  type ProviderDetails,
} from '@shinkai_network/shinkai-message-ts/api/jobs/types';

export type SendMessageToJobInput = Token & {
  nodeAddress: string;
  jobId: string;
  message: string;
  files?: File[];
  parent: string | null;
  toolKey?: string;
  /** Provider details for optimistic assistant message (optional) */
  provider?: ProviderDetails;
};

export type SendMessageToJobOutput = JobMessageResponse;
