import { publishAgent as publishAgentApi } from '@shinkai_network/shinkai-message-ts/api/agents/index';

import { type PublishAgentInput } from './types';

export const publishAgent = async ({
  nodeAddress,
  token,
  agentId,
}: PublishAgentInput) => {
  const response = await publishAgentApi(nodeAddress, token, {
    agent_id: agentId,
  });
  return response;
};

