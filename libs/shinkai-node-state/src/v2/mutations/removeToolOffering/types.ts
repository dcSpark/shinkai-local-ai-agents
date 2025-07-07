import { type Token } from '@shinkai_network/shinkai-message-ts/api/general/types';

export type RemoveToolOutput = {
  status: string;
};

export type RemoveToolInput = Token & {
  nodeAddress: string;
  toolKey: string;
};
