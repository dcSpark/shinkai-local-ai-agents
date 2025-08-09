import { killJob as killJobApi } from '@shinkai_network/shinkai-message-ts/api/jobs/index';

import { type KillJobInput } from './types';

export const killJob = async ({
  nodeAddress,
  token,
  conversationInboxName,
}: KillJobInput) => {
  const response = await killJobApi(nodeAddress, token, {
    conversation_inbox_name: conversationInboxName,
  });
  return response;
};
