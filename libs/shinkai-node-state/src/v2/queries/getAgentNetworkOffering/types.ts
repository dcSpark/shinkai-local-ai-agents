import { type Token } from '@shinkai_network/shinkai-message-ts/api/general/types';
import { type GetAgentNetworkOfferingResponse } from '@shinkai_network/shinkai-message-ts/api/agents/types';

export type GetAgentNetworkOfferingInput = Token & {
  nodeAddress: string;
  agentId: string;
};

export type GetAgentNetworkOfferingOutput = GetAgentNetworkOfferingResponse;
