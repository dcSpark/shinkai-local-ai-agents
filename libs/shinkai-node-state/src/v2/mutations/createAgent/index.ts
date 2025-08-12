import { createAgent as createAgentApi } from '@shinkai_network/shinkai-message-ts/api/agents/index';

import { createRecurringTask } from '../createRecurringTask';
import { type CreateAgentInput } from './types';

export const createAgent = async ({
  nodeAddress,
  token,
  agent,
  cronExpression,
}: CreateAgentInput) => {
  const response = await createAgentApi(nodeAddress, token, {
    ...agent,
  });

  if (cronExpression) {
    await createRecurringTask({
      nodeAddress,
      token,
      name: agent.agent_id,
      description: agent.ui_description,
      llmProvider: agent.agent_id,
      cronExpression,
      chatConfig: {
        custom_prompt: agent.config?.custom_prompt ?? '',
        temperature: agent.config?.temperature,
        top_p: agent.config?.top_p,
        top_k: agent.config?.top_k,
        use_tools: agent.tools.length > 0,
        thinking: agent.config?.thinking,
        reasoning_effort: agent.config?.reasoning_effort,
      },
      message: agent.ui_description,
      toolKey: agent.tools.length > 0 ? agent.tools[0] : '',
    });
  }
  return response;
};
