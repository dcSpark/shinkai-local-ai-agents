import { type Token } from '@shinkai_network/shinkai-message-ts/api/general/types';
import { type KillJobResponse } from '@shinkai_network/shinkai-message-ts/api/jobs/types';

export type KillJobOutput = KillJobResponse;

export type KillJobInput = Token & {
  nodeAddress: string;
  conversationInboxName: string;
};
