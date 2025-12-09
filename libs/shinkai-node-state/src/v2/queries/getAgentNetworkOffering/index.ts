import { getAgentNetworkOffering as getAgentNetworkOfferingApi } from '@shinkai_network/shinkai-message-ts/api/agents/index';
import { type GetAgentNetworkOfferingInput } from './types';

export const getAgentNetworkOffering = async ({
  nodeAddress,
  token,
  agentId,
}: GetAgentNetworkOfferingInput) => {
  const result = await getAgentNetworkOfferingApi(nodeAddress, token, agentId);
  return result;
};
