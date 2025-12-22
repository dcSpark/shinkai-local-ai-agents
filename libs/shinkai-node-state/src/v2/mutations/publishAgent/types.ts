import { type PublishAgentResponse } from '@shinkai_network/shinkai-message-ts/api/agents/types';
import { type Token } from '@shinkai_network/shinkai-message-ts/api/general/types';

export type PublishAgentOutput = PublishAgentResponse;

export type PublishAgentInput = Token & {
  nodeAddress: string;
  agentId: string;
};

